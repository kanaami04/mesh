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
  return new Codegen(file).generate(program);
}

class Codegen {
  private out: string[] = [];
  private indent = 0;
  // 関数ごとに「本体で ! を使ったか」を記録する(使った関数だけ try/catch で包む)
  private propStack: boolean[] = [];

  constructor(private file: string) {}

  private emit(line: string) {
    this.out.push("  ".repeat(this.indent) + line);
  }

  // パニックメッセージに埋め込む位置情報: "main.mesh:3:8"
  private at(pos: Pos): string {
    return JSON.stringify(`${this.file}:${pos.line}:${pos.col}`);
  }

  generate(program: Program): string {
    this.out.push(PRELUDE.trimEnd());
    this.out.push("");
    for (const fn of program.fns) {
      this.genFnDecl(fn);
      this.out.push("");
    }
    this.out.push("main().catch(__panic);");
    return this.out.join("\n") + "\n";
  }

  private genFnDecl(fn: FnDecl) {
    const params = fn.params.map((p) => p.name).join(", ");
    this.emit(`async function ${fn.name}(${params}) {`);
    this.genFnBody(fn.body);
    this.emit("}");
  }

  // 関数本体を出力する。`!` を使っていたら全体を try/catch で包み、
  // __Propagate(伝播シグナル)を受け取ったら即 return する
  private genFnBody(body: Block) {
    this.propStack.push(false);
    const saved = this.out;
    this.out = [];
    this.genBlockBody(body);
    const lines = this.out;
    this.out = saved;
    const usesProp = this.propStack.pop()!;

    if (!usesProp) {
      this.out.push(...lines);
      return;
    }
    this.emit("  try {");
    this.out.push(...lines.map((l) => "  " + l));
    this.emit("  } catch (e) {");
    this.emit("    if (e instanceof __Propagate) return e.value;");
    this.emit("    throw e;");
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

      case "assign": {
        if (stmt.targets.length === 1) {
          const target = stmt.targets[0];
          const value = this.genExpr(stmt.values[0]);
          if (target.kind === "index") {
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

      case "go": {
        // 引数は go 文の時点で評価する(Go と同じ)。関数は await せず起動 = goroutine
        const callee = this.genExpr(stmt.call.callee);
        const args = stmt.call.args.map((a) => this.genExpr(a)).join(", ");
        this.emit(`__go(${callee}, [${args}]);`);
        break;
      }

      case "send":
        this.emit(`${this.genExpr(stmt.channel)}.send(${this.genExpr(stmt.value)});`);
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
    const t: TypeNode = pattern.type;
    if (t.kind === "literal") return `(__m === ${JSON.stringify(t.value)})`;
    if (t.kind === "array") return "Array.isArray(__m)";
    if (t.kind === "chan") return "(__m instanceof __Channel)";
    if (t.kind === "union" || t.kind === "structType") return "false"; // checker が弾いている(単一型のみ)
    switch (t.name) {
      case "none": return "(__m === null)";
      case "error": return "(__m instanceof Error)";
      case "int": return "Number.isInteger(__m)";
      case "float": return '(typeof __m === "number")';
      case "string": return '(typeof __m === "string")';
      case "bool": return '(typeof __m === "boolean")';
      default:
        // ユーザー定義のstruct: JSオブジェクト(null/error/配列以外)かどうかで判定
        return '(typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m))';
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
        return expr.name;

      case "is": {
        const operand = this.genExpr(expr.operand);
        // v1 の is は none / error のみ(checker が保証)
        if (expr.target.kind === "name" && expr.target.name === "none") {
          return `(${operand} === null)`;
        }
        return `(${operand} instanceof Error)`;
      }

      case "prop": {
        this.propStack[this.propStack.length - 1] = true;
        return `__prop(${this.genExpr(expr.operand)})`;
      }

      case "orElse":
        // 右辺は失敗時にだけ評価する(遅延評価)
        return `(await __or(${this.genExpr(expr.left)}, async () => ${this.genExpr(expr.right)}))`;

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
        return `(await ${this.genExpr(expr.channel)}.recv())`;

      case "call":
        return this.genCall(expr);

      case "index":
        // 範囲外アクセスは undefined を返さず panic する(層1)
        return `__idx(${this.genExpr(expr.target)}, ${this.genExpr(expr.index)}, ${this.at(expr.pos)})`;

      case "member":
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
        return "new __Channel()";

      case "structLit": {
        // User{name: "alice"} → ({ name: "alice" })。文の先頭でもブロックと誤読されないよう括弧で包む
        const fields = expr.fields.map((f) => `${f.name}: ${this.genExpr(f.value)}`);
        return `({ ${fields.join(", ")} })`;
      }
    }
  }

  private genCall(expr: Expr & { kind: "call" }): string {
    const args = expr.args.map((a) => this.genExpr(a));

    // 組み込み関数はランタイムの同期ヘルパへ直接変換
    if (expr.callee.kind === "ident" && BUILTINS.has(expr.callee.name)) {
      switch (expr.callee.name) {
        case "print":
          return `__print(${args.join(", ")})`;
        case "str":
          return `__fmt(${args[0]})`;
        case "len":
          return `${args[0]}.length`;
        case "push":
          return `${args[0]}.push(${args[1]})`;
        case "error":
          return `__error(${args[0]})`;
        case "sleep":
          return `(await __sleep(${args[0]}))`;
      }
    }

    // ユーザー定義関数はすべて async なので常に await
    return `(await ${this.genExpr(expr.callee)}(${args.join(", ")}))`;
  }
}
