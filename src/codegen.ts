// Codegen: 検査済みの AST から JavaScript を出力する。
//
// 設計の要:
// - Mesh の関数はすべて async function として出力し、呼び出しは常に await する。
//   これにより <-ch (チャネル受信) を await に変換でき、Go の
//   「ブロックして待つ」を JS の「イベントループに譲って待つ」へ対応させられる。
// - go f(x) は await しない = 裏で走り続ける Promise になる。これが goroutine。
// - 多値戻り `return a, err` は配列 [a, err]、受け側 `v, err := f()` は分割代入。

import type { Block, Expr, FnDecl, MatchPattern, Program, Stmt, TypeNode } from "./ast";
import { BUILTINS } from "./checker";
import { PRELUDE } from "./runtime";
import type { Pos } from "./token";

export function generate(program: Program, file = "main.mesh"): string {
  return generateModules([{ pkg: "main", file, program }]);
}

// 全パッケージを1つの.mjsへまとめて出力する(バンドル)。
// 非mainパッケージのトップレベル関数は `pkg$name` に改名して衝突を防ぐ
// (Meshの識別子に $ は使えないので、ユーザーコードと衝突しない)
export function generateModules(modules: { pkg: string; file: string; program: Program }[]): string {
  return new Codegen().generateAll(modules);
}

class Codegen {
  private out: string[] = [];
  private indent = 0;
  // 関数ごとに「本体で ! を使ったか」を記録する(使った関数だけ try/catch で包む)
  private propStack: boolean[] = [];
  // 関数ごとに「本体で spawn(detachではない)を使ったか」を記録する。
  // 使った関数だけ、本体全体を暗黙の wait スコープで包む(2段スコープ設計)
  private spawnStack: boolean[] = [];
  // 今出力中のモジュールの文脈(パニック位置・関数名のマングルに使う)
  private file = "main.mesh";
  private pkg = "main";
  // 今のパッケージのトップレベル関数名(同一パッケージ内の参照もマングルするため。
  // 別ファイルの同パッケージ関数も含む)
  private localFns = new Set<string>();

  private emit(line: string) {
    this.out.push("  ".repeat(this.indent) + line);
  }

  // パニックメッセージに埋め込む位置情報: "main.mesh:3:8"
  private at(pos: Pos): string {
    return JSON.stringify(`${this.file}:${pos.line}:${pos.col}`);
  }

  generateAll(modules: { pkg: string; file: string; program: Program }[]): string {
    // パッケージごとのトップレベル関数名を先に集める(ファイル横断)
    const fnsByPkg = new Map<string, Set<string>>();
    for (const m of modules) {
      const set = fnsByPkg.get(m.pkg) ?? new Set();
      for (const fn of m.program.fns) {
        if (!fn.receiver) set.add(fn.name);
      }
      fnsByPkg.set(m.pkg, set);
    }

    this.out.push(PRELUDE.trimEnd());
    this.out.push("");
    for (const m of modules) {
      this.file = m.file;
      this.pkg = m.pkg;
      this.localFns = fnsByPkg.get(m.pkg) ?? new Set();
      for (const fn of m.program.fns) {
        this.genFnDecl(fn);
        this.out.push("");
      }
    }
    this.out.push("main().catch(__panic);");
    return this.out.join("\n") + "\n";
  }

  // トップレベル関数の生成JS名: mainパッケージは素の名前、それ以外は pkg$name
  private fnJsName(pkg: string, name: string): string {
    return pkg === "main" ? name : `${pkg}$${name}`;
  }

  private genFnDecl(fn: FnDecl) {
    const recvParams = fn.receiver ? [fn.receiver.name] : [];
    const params = [...recvParams, ...fn.params.map((p) => p.name)].join(", ");
    const name = fn.receiver
      ? this.methodJsName(this.receiverStructName(fn.receiver), fn.name)
      : this.fnJsName(this.pkg, fn.name);
    this.emit(`async function ${name}(${params}) {`);
    this.genFnBody(fn.body);
    this.emit("}");
  }

  // メソッドの生成JS名: struct名+メソッド名で一意にする(他structの同名メソッドと衝突しないように)。
  // struct名はパッケージ修飾で "math.User" の形になりうるので、JS識別子に使えるよう "." を "$" にする
  private methodJsName(structName: string, methodName: string): string {
    return `__m_${structName.replace(/\./g, "$")}_${methodName}`;
  }

  // レシーバの型は checker が struct であることを保証済み(v1は名前で直接参照する形のみ)。
  // 呼び出し側は checker が解決した修飾名(math.User)でメソッドを引くので、宣言側も揃える
  private receiverStructName(receiver: NonNullable<FnDecl["receiver"]>): string {
    const bare = receiver.type.kind === "name" ? receiver.type.name : "(anonymous)";
    return this.pkg === "main" ? bare : `${this.pkg}.${bare}`;
  }

  // 関数本体を出力する。必要に応じて2種類のラッパーで包む:
  // - `!` を使っていたら try/catch で包み、__Propagate(伝播シグナル)を受け取ったら即 return
  // - `spawn` を使っていたら本体全体を暗黙の wait スコープにする(2段スコープ設計)。
  //   関数を抜けるとき(早期 return でも)自分が spawn したタスクを必ず待つので、
  //   「発射しっぱなしのタスク」が構文的に存在できない。detach はここに登録されない
  private genFnBody(body: Block) {
    this.propStack.push(false);
    this.spawnStack.push(false);
    const saved = this.out;
    this.out = [];
    this.genBlockBody(body);
    const lines = this.out;
    this.out = saved;
    const usesProp = this.propStack.pop()!;
    const usesSpawn = this.spawnStack.pop()!;

    if (!usesProp && !usesSpawn) {
      this.out.push(...lines);
      return;
    }
    if (usesSpawn) this.emit("  __waitStack.push([]);");
    this.emit("  try {");
    this.out.push(...lines.map((l) => "  " + l));
    if (usesProp) {
      this.emit("  } catch (e) {");
      this.emit("    if (e instanceof __Propagate) return e.value;");
      this.emit("    throw e;");
    }
    if (usesSpawn) {
      this.emit("  } finally {");
      this.emit("    await Promise.all(__waitStack.pop());");
    }
    this.emit("  }");
  }

  private genBlockBody(block: Block) {
    this.indent++;
    for (const stmt of block.stmts) this.genStmt(stmt);
    this.indent--;
  }

  private genStmt(stmt: Stmt) {
    switch (stmt.kind) {
      case "shortVarDecl": {
        // 不変(デフォルト)は const、mut は let に出す。JSエンジン側でも再代入を二重に防ぐ
        const kw = stmt.mutable ? "let" : "const";
        if (stmt.names.length === 1) {
          if (stmt.names[0] === "_") {
            this.emit(`${this.genExpr(stmt.values[0])};`);
          } else {
            this.emit(`${kw} ${stmt.names[0]} = ${this.genExpr(stmt.values[0])};`);
          }
          break;
        }
        // 多値: const [v, err] = await f(...);  ("_" は分割代入の穴にする)
        const names = stmt.names.map((n) => (n === "_" ? "" : n)).join(", ");
        const value =
          stmt.values.length === 1
            ? this.genExpr(stmt.values[0])
            : `[${stmt.values.map((v) => this.genExpr(v)).join(", ")}]`;
        this.emit(`${kw} [${names}] = ${value};`);
        break;
      }

      case "typedVarDecl": {
        const kw = stmt.mutable ? "let" : "const";
        this.emit(`${kw} ${stmt.name} = ${this.genExpr(stmt.value)};`);
        break;
      }

      case "assign": {
        if (stmt.targets.length === 1) {
          const target = stmt.targets[0];
          const value = this.genExpr(stmt.values[0]);
          if (target.kind === "index" && target.target.resolvedType?.kind === "map") {
            // map への書き込みは新キーの追加も正当なので検査なしの set
            this.emit(`${this.genExpr(target.target)}.set(${this.genExpr(target.index)}, ${value});`);
          } else if (target.kind === "index") {
            // 添字への書き込みも範囲検査する(範囲外は黙って配列を伸ばさず panic)
            this.emit(
              `__idxset(${this.genExpr(target.target)}, ${this.genExpr(target.index)}, ${value}, ${this.at(target.pos)});`,
            );
          } else {
            this.emit(`${this.genLValue(target)} = ${value};`);
          }
          break;
        }
        const targets = stmt.targets
          .map((t) => (t.kind === "ident" && t.name === "_" ? "" : this.genLValue(t)))
          .join(", ");
        const value =
          stmt.values.length === 1
            ? this.genExpr(stmt.values[0])
            : `[${stmt.values.map((v) => this.genExpr(v)).join(", ")}]`;
        this.emit(`([${targets}] = ${value});`);
        break;
      }

      case "exprStmt":
        this.emit(`${this.genExpr(stmt.expr)};`);
        break;

      case "return": {
        if (stmt.value === null) this.emit("return;");
        else this.emit(`return ${this.genExpr(stmt.value)};`);
        break;
      }

      case "if": {
        this.emit(`if (${this.genExpr(stmt.cond)}) {`);
        this.genBlockBody(stmt.then);
        this.genElse(stmt.else_);
        break;
      }

      case "for": {
        const init = stmt.init ? this.genSimpleStmt(stmt.init) : "";
        const cond = stmt.cond ? this.genExpr(stmt.cond) : "";
        const post = stmt.post ? this.genSimpleStmt(stmt.post) : "";
        this.emit(`for (${init}; ${cond}; ${post}) {`);
        this.genBlockBody(stmt.body);
        this.emit("}");
        break;
      }

      case "rangeFor": {
        const subjectType = stmt.subject.resolvedType;
        const subject = this.genExpr(stmt.subject);
        const names = stmt.names.map((n) => (n === "_" ? "" : n));
        if (subjectType?.kind === "map") {
          this.emit(`for (const [${names.join(", ")}] of ${subject}) {`);
        } else if (stmt.names.length === 1) {
          // for i := range n(0..n-1)。上限は最初に一度だけ評価する
          const i = names[0] === "" ? "__i" : names[0];
          this.emit(`for (let ${i} = 0, __n = ${subject}; ${i} < __n; ${i}++) {`);
        } else {
          // 配列: entries() が [添字, 値] を返す
          this.emit(`for (const [${names.join(", ")}] of ${subject}.entries()) {`);
        }
        this.genBlockBody(stmt.body);
        this.emit("}");
        break;
      }

      case "wait": {
        // ブロック内で spawn したタスクを集めて、抜けるときに全部待つ。
        // 早期 return でも待ち漏れないよう try/finally で保証する
        this.emit("__waitStack.push([]);");
        this.emit("try {");
        this.genBlockBody(stmt.body);
        this.emit("} finally {");
        this.emit("  await Promise.all(__waitStack.pop());");
        this.emit("}");
        break;
      }

      case "send":
        // 容量指定チャネルは満杯だと本当にブロックしうるので await する
        this.emit(`(await ${this.genExpr(stmt.channel)}.send(${this.genExpr(stmt.value)}));`);
        break;

      case "incDec": {
        const t = stmt.target;
        if (t.kind === "index") {
          // a[i]++ は検査つきの読み書きに展開する(添字式の二重評価は v1 の割り切り)
          const arr = this.genExpr(t.target);
          const idx = this.genExpr(t.index);
          const at = this.at(t.pos);
          const op = stmt.op === "++" ? "+" : "-";
          this.emit(`__idxset(${arr}, ${idx}, __idx(${arr}, ${idx}, ${at}) ${op} 1, ${at});`);
        } else {
          this.emit(`${this.genLValue(t)}${stmt.op};`);
        }
        break;
      }

      case "break":
        this.emit("break;");
        break;
      case "continue":
        this.emit("continue;");
        break;
    }
  }

  private genElse(else_: (Stmt & { kind: "if" }) | Block | null) {
    if (!else_) {
      this.emit("}");
      return;
    }
    if (else_.kind === "if") {
      this.emit(`} else if (${this.genExpr(else_.cond)}) {`);
      this.genBlockBody(else_.then);
      this.genElse(else_.else_);
    } else {
      this.emit("} else {");
      this.genBlockBody(else_);
      this.emit("}");
    }
  }

  // for ヘッダ内の init/post 用: セミコロンなしの1行表現にする
  private genSimpleStmt(stmt: Stmt): string {
    switch (stmt.kind) {
      case "shortVarDecl":
        return `let ${stmt.names[0]} = ${this.genExpr(stmt.values[0])}`;
      case "typedVarDecl":
        return `let ${stmt.name} = ${this.genExpr(stmt.value)}`;
      case "assign":
        return `${this.genLValue(stmt.targets[0])} = ${this.genExpr(stmt.values[0])}`;
      case "incDec":
        return `${this.genLValue(stmt.target)}${stmt.op}`;
      case "exprStmt":
        return this.genExpr(stmt.expr);
      default:
        return "";
    }
  }

  // match の型パターンを実行時テストに変換する(__m は match 対象の値)
  private genMatchTest(pattern: MatchPattern): string {
    if (pattern.kind === "wildcard") return "true";
    return this.genTypeTest("__m", pattern.type);
  }

  // ref(式の文字列)が型ノード t に合致するかの実行時テストを組み立てる。
  // 判別可能unionの部分構造パターン({ kind: "ok" } 等)ではフィールドごとに
  // ref.fieldName を対象に再帰して、全フィールドの一致を && でつなぐ
  private genTypeTest(ref: string, t: TypeNode): string {
    if (t.kind === "literal") return `(${ref} === ${JSON.stringify(t.value)})`;
    if (t.kind === "array") return `Array.isArray(${ref})`;
    if (t.kind === "chan") return `(${ref} instanceof __Channel)`;
    if (t.kind === "union") return "false"; // checker が弾いている(単一型のみ)
    if (t.kind === "mapType") return `(${ref} instanceof Map)`;
    if (t.kind === "fnType") return `(typeof ${ref} === "function")`;
    if (t.kind === "structType") {
      const objTest = `(typeof ${ref} === "object" && ${ref} !== null && !(${ref} instanceof Error) && !Array.isArray(${ref}))`;
      const fieldTests = t.fields.map((f) => this.genTypeTest(`${ref}.${f.name}`, f.type));
      return [objTest, ...fieldTests].join(" && ");
    }
    switch (t.name) {
      case "none": return `(${ref} === null)`;
      case "closed": return `(${ref} === __CLOSED)`;
      case "error": return `(${ref} instanceof Error)`;
      case "int": return `Number.isInteger(${ref})`;
      case "float": return `(typeof ${ref} === "number")`;
      case "string": return `(typeof ${ref} === "string")`;
      case "bool": return `(typeof ${ref} === "boolean")`;
      default:
        // ユーザー定義のstruct: JSオブジェクト(null/error/配列以外)かどうかで判定
        return `(typeof ${ref} === "object" && ${ref} !== null && !(${ref} instanceof Error) && !Array.isArray(${ref}))`;
    }
  }

  // 代入先(lvalue)用: 添字は検査ヘルパではなく素の a[i] 形で出す必要がある
  private genLValue(expr: Expr): string {
    if (expr.kind === "index") {
      return `${this.genExpr(expr.target)}[${this.genExpr(expr.index)}]`;
    }
    return this.genExpr(expr);
  }

  // ---- 式 ----

  private genExpr(expr: Expr): string {
    switch (expr.kind) {
      case "int":
      case "float":
        return expr.value;
      case "string":
        return JSON.stringify(expr.value);

      case "interp": {
        // "worker ${id} done" → ("worker " + __fmt(id) + " done")
        const pieces = expr.segments.map((s) =>
          s.kind === "text" ? JSON.stringify(s.text) : `__fmt(${this.genExpr(s.expr)})`,
        );
        if (expr.segments[0].kind !== "text") pieces.unshift('""'); // 先頭が式でも文字列連結になるように
        return `(${pieces.join(" + ")})`;
      }
      case "bool":
        return String(expr.value);
      case "none":
        return "null"; // none の実行時表現は null(JSON にもそのまま乗る)
      case "ident":
        // 非mainパッケージ内では、自パッケージのトップレベル関数への参照もマングル名になる
        // (シャドーイング禁止により、同名のローカル変数は存在し得ない)
        if (this.localFns.has(expr.name)) return this.fnJsName(this.pkg, expr.name);
        return expr.name;

      case "is": {
        // is のパターンは match と同じなので実行時テストも genTypeTest を共用する。
        // テストが operand を複数回参照しうる(structパターン・struct名の判定)ため、
        // 変数名でない operand は一度だけ評価して束縛してからテストする
        const operand = this.genExpr(expr.operand);
        if (expr.operand.kind !== "ident") {
          return `((__v) => ${this.genTypeTest("__v", expr.target)})(${operand})`;
        }
        return this.genTypeTest(operand, expr.target);
      }

      case "prop": {
        this.propStack[this.propStack.length - 1] = true;
        if (expr.context) {
          return `(await __propCtx(${this.genExpr(expr.operand)}, async () => ${this.genExpr(expr.context)}))`;
        }
        return `__prop(${this.genExpr(expr.operand)})`;
      }

      case "orElse": {
        // 右辺は失敗時にだけ評価する(遅延評価)。束縛形は失敗値を引数で受ける
        const param = expr.binding !== undefined && expr.binding !== "_" ? expr.binding : "";
        return `(await __or(${this.genExpr(expr.left)}, async (${param}) => ${this.genExpr(expr.right)}))`;
      }

      case "match": {
        // match r { error => A  int => B } は
        // (await (async (__m) => TEST_error ? A : B)(r)) という三項演算子の連鎖になる。
        // 網羅性は checker が保証しているので、最後のアームは無条件(else)でよい
        const subject = this.genExpr(expr.subject);
        const parts: string[] = [];
        for (let i = 0; i < expr.arms.length; i++) {
          const arm = expr.arms[i];
          const body = this.genExpr(arm.body);
          if (i === expr.arms.length - 1) {
            parts.push(body);
          } else {
            const test = arm.patterns.map((p) => this.genMatchTest(p)).join(" || ");
            parts.push(`${test} ? ${body} :`);
          }
        }
        return `(await (async (__m) => ${parts.join(" ")})(${subject}))`;
      }

      case "arrayLit":
        return `[${expr.elems.map((e) => this.genExpr(e)).join(", ")}]`;

      case "binary": {
        const left = this.genExpr(expr.left);
        const right = this.genExpr(expr.right);
        if (expr.intDiv) return `__idiv(${left}, ${right}, ${this.at(expr.pos)})`; // 切り捨て+ゼロ検査
        if (expr.intMod) return `__imod(${left}, ${right}, ${this.at(expr.pos)})`;
        const op = expr.op === "==" ? "===" : expr.op === "!=" ? "!==" : expr.op;
        return `(${left} ${op} ${right})`;
      }

      case "unary":
        return `(${expr.op}${this.genExpr(expr.operand)})`;

      case "recv":
        // 受信は常に T | closed。__recv が {value, closed} を Mesh の値(closed なら __CLOSED)に変換する
        return `(await __recv(${this.genExpr(expr.channel)}))`;

      case "select": {
        const channels = expr.arms.map((a) => this.genExpr(a.channel));
        const handlers = expr.arms.map((a) => `(async (${a.name}) => ${this.genExpr(a.body)})`);
        const defaultHandler = expr.defaultArm ? `(async () => ${this.genExpr(expr.defaultArm)})` : "null";
        return `(await __select([${channels.join(", ")}], [${handlers.join(", ")}], ${defaultHandler}))`;
      }

      case "call":
        return this.genCall(expr);

      case "index":
        // map の読みは V | none(無いキーは null)。配列・文字列は範囲外で panic(層1)
        if (expr.target.resolvedType?.kind === "map") {
          return `__mget(${this.genExpr(expr.target)}, ${this.genExpr(expr.index)})`;
        }
        return `__idx(${this.genExpr(expr.target)}, ${this.genExpr(expr.index)}, ${this.at(expr.pos)})`;

      case "mapLit": {
        if (expr.entries.length === 0) return "new Map()";
        const entries = expr.entries.map((e) => `[${this.genExpr(e.key)}, ${this.genExpr(e.value)}]`);
        return `new Map([${entries.join(", ")}])`;
      }

      case "member":
        // math.add のようなパッケージ修飾参照(checkerが解決済み)はマングル名への直接参照
        if (expr.resolvedPkg) return this.fnJsName(expr.resolvedPkg, expr.name);
        return `${this.genExpr(expr.target)}.${expr.name}`;

      case "fnExpr": {
        const params = expr.params.map((p) => p.name).join(", ");
        const saved = this.out;
        const savedIndent = this.indent;
        this.out = [];
        this.indent = 0;
        this.genFnBody(expr.body);
        const body = this.out.join("\n");
        this.out = saved;
        this.indent = savedIndent;
        const pad = "  ".repeat(this.indent);
        const indented = body
          .split("\n")
          .map((l) => (l ? pad + l : l))
          .join("\n");
        return `(async (${params}) => {\n${indented}\n${pad}})`;
      }

      case "chanExpr":
        return expr.capacity ? `new __Channel(${this.genExpr(expr.capacity)})` : "new __Channel()";

      case "spawn": {
        // 引数は spawn の時点で評価する(Goと同じ)。await せず起動し、受取口を返す。
        // spawn は現在の wait スコープ(=囲む関数 or waitブロック)に登録され、detach はされない
        const callee = this.genExpr(expr.call.callee);
        const args = expr.call.args.map((a) => this.genExpr(a)).join(", ");
        if (expr.detached) {
          return `__detach(${callee}, [${args}])`;
        }
        this.spawnStack[this.spawnStack.length - 1] = true;
        return `__spawn(${callee}, [${args}])`;
      }

      case "structLit": {
        // User{name: "alice"} → ({ name: "alice" })。文の先頭でもブロックと誤読されないよう括弧で包む
        const fields = expr.fields.map((f) => `${f.name}: ${this.genExpr(f.value)}`);
        const obj = `{ ${fields.join(", ")} }`;
        // F-2後半: error type/struct のメンバーは実行時マーカーを付け、'?'/'or' が
        // 値だけを見て「これは失敗だ」と判定できるようにする(checkerのisErrorInstance参照)
        return expr.isErrorInstance ? `__errTag(${obj})` : `(${obj})`;
      }
    }
  }

  private genCall(expr: Expr & { kind: "call" }): string {
    const args = expr.args.map((a) => this.genExpr(a));

    // パッケージ修飾の関数呼び出し: math.add(args) → (await math$add(args))
    if (expr.callee.kind === "member" && expr.callee.resolvedPkg) {
      const jsName = this.fnJsName(expr.callee.resolvedPkg, expr.callee.name);
      return `(await ${jsName}(${args.join(", ")}))`;
    }

    // メソッド呼び出し: recv.method(args) → __m_Struct_method(recv, args)。
    // struct のフィールドが関数値のケース(recv.fieldFn(args))とは checker が既に区別済みで、
    // その場合は target.resolvedType の中に対象の名前が「フィールド」として現れるので
    // ここでは再チェックせず、struct型 かつ フィールドに無い名前のときだけメソッドと判定する
    if (expr.callee.kind === "member") {
      const member = expr.callee;
      const targetType = member.target.resolvedType;
      if (targetType?.kind === "struct" && !targetType.fields.some((f) => f.name === member.name)) {
        const recv = this.genExpr(member.target);
        const jsName = this.methodJsName(targetType.name, member.name);
        return `(await ${jsName}(${[recv, ...args].join(", ")}))`;
      }
    }

    // 組み込み関数はランタイムの同期ヘルパへ直接変換
    if (expr.callee.kind === "ident" && BUILTINS.has(expr.callee.name)) {
      switch (expr.callee.name) {
        case "print":
          return `__print(${args.join(", ")})`;
        case "str":
          return `__fmt(${args[0]})`;
        case "len":
          // map は .size、配列・文字列は .length
          return expr.args[0].resolvedType?.kind === "map" ? `${args[0]}.size` : `${args[0]}.length`;
        case "push":
          return `${args[0]}.push(${args[1]})`;
        case "delete":
          return `${args[0]}.delete(${args[1]})`;
        case "error":
          return `__error(${args[0]})`;
        case "sleep":
          return `(await __sleep(${args[0]}))`;
        case "contains":
          return `${args[0]}.includes(${args[1]})`;
        case "indexOf":
          return `__indexOf(${args[0]}, ${args[1]})`;
        case "keys":
          return `Array.from(${args[0]}.keys())`;
        case "values":
          return `Array.from(${args[0]}.values())`;
        case "sort":
          return `__sorted(${args[0]})`;
        case "split":
          return `${args[0]}.split(${args[1]})`;
        case "join":
          return `${args[0]}.join(${args[1]})`;
        case "trim":
          return `${args[0]}.trim()`;
        case "upper":
          return `${args[0]}.toUpperCase()`;
        case "lower":
          return `${args[0]}.toLowerCase()`;
        case "toInt":
          return `__toInt(${args[0]})`;
        case "filter":
          return `(await __filter(${args[0]}, ${args[1]}))`;
        case "transform":
          return `(await __map(${args[0]}, ${args[1]}))`;
        case "reduce":
          return `(await __reduce(${args[0]}, ${args[1]}, ${args[2]}))`;
        case "close":
          return `${args[0]}.close()`;
      }
    }

    // ユーザー定義関数はすべて async なので常に await
    return `(await ${this.genExpr(expr.callee)}(${args.join(", ")}))`;
  }
}
