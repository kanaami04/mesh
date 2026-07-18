// Checker: AST を歩いて型の矛盾を探す。ここが「TypeScriptらしさ」の心臓部。
// - `:=` の右辺から型を推論して変数に記録する
// - 関数呼び出しの引数の数と型を照合する
// - 検査しながら式に resolvedType を書き込み、Codegen へ引き継ぐ

import type { Block, Expr, FnDecl, FnExpr, Program, Stmt, TypeNode } from "./ast";
import type { Pos } from "./token";
import {
  ANY,
  BOOL,
  CLOSED,
  ERROR,
  FLOAT,
  INT,
  NONE,
  STRING,
  VOID,
  assignable,
  isFailure,
  isNarrowTarget,
  isNumeric,
  isStringy,
  typeEquals,
  typeToString,
  unionOf,
  unionWithout,
  widenLiteral,
  type Type,
} from "./types";

// 組み込みの型名(type 宣言でこれらの名前は使えない)
const BUILTIN_TYPE_NAMES = new Set([
  "int", "float", "string", "bool", "void", "error", "none", "closed", "any",
]);

export interface Diagnostic {
  pos: Pos;
  message: string;
}

// 組み込み関数。特殊な検査(可変長引数など)は checkCall 内で行う。
export const BUILTINS = new Set([
  "print", "len", "push", "str", "error", "sleep", "delete",
  "contains", "indexOf", "keys", "values", "sort",
  "split", "join", "trim", "upper", "lower", "toInt",
  "filter", "transform", "reduce",
  "close",
]);

// 生成される JavaScript で意味を持ってしまう名前は変数名として禁止する
const RESERVED = new Set([
  "await", "async", "function", "const", "let", "var", "class", "new", "this",
  "typeof", "instanceof", "in", "of", "yield", "delete", "void", "switch",
  "case", "default", "do", "while", "with", "export", "import", "extends",
  "super", "null", "undefined", "try", "catch", "finally", "throw",
]);

export function check(program: Program): Diagnostic[] {
  return new Checker().checkProgram(program);
}

// 変数1つ分の情報。mutable は「mut 宣言されたか」(デフォルト不変、B-4決定)
interface Binding {
  type: Type;
  mutable: boolean;
}

class Checker {
  private diagnostics: Diagnostic[] = [];
  private scopes: Map<string, Binding>[] = [new Map()];
  // 今チェックしている関数の戻り値型(無名関数でネストするのでスタック)
  private retStack: Type[] = [];
  // type 宣言: 名前 → 構文ノード。解決結果は resolvedAliases にメモ化
  private typeTable = new Map<string, TypeNode>();
  private resolvedAliases = new Map<string, Type>();
  private resolvingAliases = new Set<string>(); // 循環検出用
  // メソッド: struct名 → (メソッド名 → 関数型 [レシーバを含む])。
  // 自由関数のグローバル scope とは別の名前空間(P1: recv.method() と method(recv) が両方
  // 使える「二通りの書き方」を作らないため、メソッド名はここにしか登録しない)
  private methodTable = new Map<string, Map<string, Type>>();

  // ---- ユーティリティ ----

  private error(pos: Pos, message: string) {
    this.diagnostics.push({ pos, message });
  }

  private pushScope() {
    this.scopes.push(new Map());
  }

  private popScope() {
    this.scopes.pop();
  }

  private declare(name: string, type: Type, pos: Pos, mutable = false) {
    if (name === "_") return; // ブランク識別子は捨てる用
    if (RESERVED.has(name)) {
      this.error(pos, `'${name}' is a reserved word and cannot be used as a name`);
      return;
    }
    if (BUILTINS.has(name)) {
      this.error(pos, `'${name}' is a builtin function and cannot be redeclared`);
      return;
    }
    const scope = this.scopes[this.scopes.length - 1];
    if (scope.has(name)) {
      this.error(pos, `'${name}' is already declared in this scope`);
      return;
    }
    // シャドーイング禁止(2026-07-17決定): 外側スコープ(関数名を含む)に同名があれば
    // 「隠しただけで更新していない」バグの温床になるので拒否する。更新したいなら '=' を使う。
    if (this.lookup(name) !== undefined) {
      this.error(pos, `'${name}' shadows an outer binding — use '=' to update it, or pick a different name`);
      return;
    }
    scope.set(name, { type, mutable });
  }

  private lookup(name: string): Binding | undefined {
    for (let i = this.scopes.length - 1; i >= 0; i--) {
      const b = this.scopes[i].get(name);
      if (b) return b;
    }
    return undefined;
  }

  // 型注釈(構文)を内部表現の型へ変換
  private resolveType(node: TypeNode): Type {
    switch (node.kind) {
      case "array":
        return { kind: "array", elem: this.resolveType(node.elem) };
      case "chan":
        return { kind: "chan", elem: this.resolveType(node.elem) };
      case "mapType":
        return { kind: "map", key: this.resolveType(node.key), value: this.resolveType(node.value) };
      case "union":
        return unionOf(node.members.map((m) => this.resolveType(m)));
      case "literal":
        return { kind: "literal", value: node.value };
      case "structType": {
        // 名前なし文脈で来た場合(通常は resolveAlias 経由で来る)
        return {
          kind: "struct",
          name: "(anonymous)",
          fields: node.fields.map((f) => ({ name: f.name, type: this.resolveType(f.type) })),
        };
      }
      case "name":
        switch (node.name) {
          case "int": return INT;
          case "float": return FLOAT;
          case "string": return STRING;
          case "bool": return BOOL;
          case "void": return VOID;
          case "error": return ERROR;
          case "none": return NONE;
          case "closed": return CLOSED;
          case "any": return ANY;
          default:
            return this.resolveAlias(node.name, node.pos);
        }
    }
  }

  // type 宣言された名前の解決(メモ化+循環検出)
  private resolveAlias(name: string, pos: Pos): Type {
    const memo = this.resolvedAliases.get(name);
    if (memo) return memo;
    const node = this.typeTable.get(name);
    if (!node) {
      this.error(pos, `unknown type '${name}'`);
      return ANY;
    }
    // struct は「先に器を登録 → 後からフィールドを埋める」(knot-tying)。
    // これにより struct Node { next: Node | none } のような再帰型が書ける
    if (node.kind === "structType") {
      const struct: Type = { kind: "struct", name, fields: [] };
      this.resolvedAliases.set(name, struct);
      for (const f of node.fields) {
        struct.fields.push({ name: f.name, type: this.resolveType(f.type) });
      }
      return struct;
    }
    if (this.resolvingAliases.has(name)) {
      this.error(pos, `type alias cycle involving '${name}'`);
      return ANY;
    }
    this.resolvingAliases.add(name);
    const resolved = this.resolveType(node);
    this.resolvingAliases.delete(name);
    this.resolvedAliases.set(name, resolved);
    return resolved;
  }

  private fnType(params: { type: TypeNode }[], ret: TypeNode | null): Type {
    return {
      kind: "fn",
      params: params.map((p) => this.resolveType(p.type)),
      ret: ret ? this.resolveType(ret) : VOID,
    };
  }

  // ---- プログラム全体 ----

  checkProgram(program: Program): Diagnostic[] {
    // 先に type 宣言を登録する(関数シグネチャがエイリアスを参照できるように)
    for (const td of program.types) {
      if (BUILTIN_TYPE_NAMES.has(td.name)) {
        this.error(td.pos, `'${td.name}' is a builtin type and cannot be redeclared`);
        continue;
      }
      if (this.typeTable.has(td.name)) {
        this.error(td.pos, `type '${td.name}' is already declared`);
        continue;
      }
      this.typeTable.set(td.name, td.node);
    }
    // 全エイリアスを解決しておく(未使用でも循環や未知型を報告するため)
    for (const td of program.types) {
      if (this.typeTable.get(td.name) === td.node) this.resolveAlias(td.name, td.pos);
    }

    // 先に全関数/メソッドのシグネチャを登録する(前方参照・相互再帰を許すため)
    for (const fn of program.fns) {
      if (fn.receiver) {
        this.declareMethod(fn);
      } else {
        this.declare(fn.name, this.fnType(fn.params, fn.ret), fn.pos);
      }
    }

    const main = program.fns.find((f) => f.name === "main" && !f.receiver);
    if (!main) {
      this.error({ line: 1, col: 1 }, "missing 'fn main()' — Mesh programs start from main");
    } else if (main.params.length > 0 || main.ret !== null) {
      this.error(main.pos, "'fn main()' must take no parameters and return nothing");
    }

    for (const fn of program.fns) this.checkFn(fn);
    return this.diagnostics;
  }

  // fn (u: User) describe() ... のシグネチャを methodTable に登録する(グローバルscopeには置かない)
  private declareMethod(fn: FnDecl) {
    if (!fn.receiver) return;
    const recvType = this.resolveType(fn.receiver.type);
    if (recvType.kind !== "struct") {
      this.error(fn.receiver.pos, `method receiver must be a struct type, got ${typeToString(recvType)}`);
      return;
    }
    if (BUILTINS.has(fn.name)) {
      this.error(fn.pos, `'${fn.name}' is a builtin function and cannot be used as a method name`);
      return;
    }
    if (recvType.fields.some((f) => f.name === fn.name)) {
      this.error(fn.pos, `${recvType.name} already has a field named '${fn.name}'`);
      return;
    }
    let methods = this.methodTable.get(recvType.name);
    if (!methods) {
      methods = new Map();
      this.methodTable.set(recvType.name, methods);
    }
    if (methods.has(fn.name)) {
      this.error(fn.pos, `${recvType.name} already has a method named '${fn.name}'`);
      return;
    }
    const base = this.fnType(fn.params, fn.ret);
    if (base.kind !== "fn") return; // 到達しない(fnTypeは常にkind:"fn"を返す)
    methods.set(fn.name, { kind: "fn", params: [recvType, ...base.params], ret: base.ret });
  }

  private checkFn(fn: FnDecl | FnExpr) {
    this.pushScope();
    if (fn.kind === "fnDecl" && fn.receiver) {
      this.declare(fn.receiver.name, this.resolveType(fn.receiver.type), fn.receiver.pos);
    }
    for (const p of fn.params) this.declare(p.name, this.resolveType(p.type), p.pos);
    this.retStack.push(fn.ret ? this.resolveType(fn.ret) : VOID);
    this.checkBlock(fn.body);
    this.retStack.pop();
    this.popScope();
  }

  // override: narrowing 用に「このブロック内だけ変数の型を差し替える」
  private checkBlock(block: Block, override?: { name: string; type: Type }) {
    this.pushScope();
    if (override) {
      this.scopes[this.scopes.length - 1].set(override.name, { type: override.type, mutable: false });
    }
    for (const stmt of block.stmts) this.checkStmt(stmt);
    this.popScope();
  }

  // ブロックが必ず抜ける(return/break/continue で終わる)か — narrowing の継続判定に使う
  private blockTerminates(block: Block): boolean {
    const last = block.stmts[block.stmts.length - 1];
    return last !== undefined && (last.kind === "return" || last.kind === "break" || last.kind === "continue");
  }

  // ---- 文 ----

  private checkStmt(stmt: Stmt) {
    switch (stmt.kind) {
      case "shortVarDecl": {
        const types = this.checkExprList(stmt.values, stmt.names.length, stmt.pos);
        for (let i = 0; i < stmt.names.length; i++) {
          // mut 宣言はリテラル型を string に広げる(後で別の文字列を代入できるように)
          const t = stmt.mutable ? widenLiteral(types[i] ?? ANY) : (types[i] ?? ANY);
          this.declare(stmt.names[i], t, stmt.pos, stmt.mutable);
        }
        break;
      }
      case "typedVarDecl": {
        // 宣言された型が「正」。値はそれに入れられればよい(none も union なら入る)
        const declared = this.resolveType(stmt.typeNode);
        const valueType = this.checkExprSingle(stmt.value);
        if (!assignable(valueType, declared)) {
          this.error(
            stmt.value.pos,
            `cannot use ${typeToString(valueType)} as ${typeToString(declared)}`,
          );
        }
        this.declare(stmt.name, declared, stmt.pos, stmt.mutable);
        break;
      }
      case "assign": {
        const types = this.checkExprList(stmt.values, stmt.targets.length, stmt.pos);
        for (let i = 0; i < stmt.targets.length; i++) {
          const target = stmt.targets[i];
          if (target.kind === "ident" && target.name === "_") continue;
          const targetType = this.checkExpr(target);
          if (target.kind === "ident") {
            const binding = this.lookup(target.name);
            if (!binding) continue; // 未宣言エラーは checkExpr が報告済み
            if (!binding.mutable) {
              this.error(
                target.pos,
                `'${target.name}' is immutable — declare it with 'mut' to allow reassignment`,
              );
              continue;
            }
          }
          // map への書き込みは「値の型」に対して検査する(読みの V | none ではなく)
          let expected = targetType;
          if (target.kind === "index") {
            const container = target.target.resolvedType;
            if (container?.kind === "map") expected = container.value;
          }
          const valueType = types[i] ?? ANY;
          if (!assignable(valueType, expected)) {
            this.error(stmt.pos, `cannot assign ${typeToString(valueType)} to ${typeToString(expected)}`);
          }
        }
        break;
      }
      case "exprStmt":
        this.checkExpr(stmt.expr);
        break;
      case "return": {
        const expected = this.retStack[this.retStack.length - 1] ?? VOID;
        if (stmt.value === null) {
          if (!typeEquals(expected, VOID)) {
            this.error(stmt.pos, `this function must return ${typeToString(expected)}`);
          }
          break;
        }
        const t = this.checkExprSingle(stmt.value);
        if (typeEquals(expected, VOID)) {
          this.error(stmt.value.pos, "this function has no return value");
        } else if (!assignable(t, expected)) {
          this.error(stmt.value.pos, `cannot return ${typeToString(t)} as ${typeToString(expected)}`);
        }
        break;
      }
      case "if": {
        const cond = this.checkExprSingle(stmt.cond);
        if (!typeEquals(cond, BOOL) && cond.kind !== "any") {
          this.error(stmt.cond.pos, `if condition must be bool, got ${typeToString(cond)}`);
        }

        // narrowing: `if x is none { ... }` — then内は none、else内と(thenが必ず抜ける場合の)
        // 後続は「union から none を除いた型」として扱う
        const narrow = this.narrowFromCond(stmt.cond);
        if (narrow) {
          const { name, member, binding } = narrow;
          const rest = unionWithout(binding.type, (m) => typeEquals(m, member));
          this.checkBlock(stmt.then, { name, type: member });
          if (stmt.else_) {
            if (stmt.else_.kind === "if") {
              this.pushScope();
              this.scopes[this.scopes.length - 1].set(name, { type: rest, mutable: false });
              this.checkStmt(stmt.else_);
              this.popScope();
            } else {
              this.checkBlock(stmt.else_, { name, type: rest });
            }
          } else if (this.blockTerminates(stmt.then)) {
            binding.type = rest; // 早期リターン後の残りの行では絞り込みが効き続ける
          }
          break;
        }

        this.checkBlock(stmt.then);
        if (stmt.else_) {
          if (stmt.else_.kind === "if") this.checkStmt(stmt.else_);
          else this.checkBlock(stmt.else_);
        }
        break;
      }
      case "for": {
        this.pushScope(); // for i := ... の i はループ内スコープ
        // C風forのヘッダ変数は暗黙に可変(B-4決定。デフォルト不変の唯一の構造的例外)
        if (stmt.init?.kind === "shortVarDecl") stmt.init.mutable = true;
        if (stmt.init) this.checkStmt(stmt.init);
        if (stmt.cond) {
          const cond = this.checkExprSingle(stmt.cond);
          if (!typeEquals(cond, BOOL) && cond.kind !== "any") {
            this.error(stmt.cond.pos, `for condition must be bool, got ${typeToString(cond)}`);
          }
        }
        if (stmt.post) this.checkStmt(stmt.post);
        this.checkBlock(stmt.body);
        this.popScope();
        break;
      }
      case "wait":
        this.checkBlock(stmt.body);
        break;
      case "rangeFor": {
        const subject = this.checkExprSingle(stmt.subject);
        this.pushScope();
        const declare2 = (t1: Type, t2: Type, what: string) => {
          if (stmt.names.length !== 2) {
            this.error(
              stmt.pos,
              `range over ${what} needs two names: 'for a, b := range ...' (use _ to ignore one)`,
            );
          }
          this.declare(stmt.names[0] ?? "_", t1, stmt.pos);
          this.declare(stmt.names[1] ?? "_", t2, stmt.pos);
        };
        if (subject.kind === "array") {
          declare2(INT, subject.elem, "an array");
        } else if (subject.kind === "map") {
          declare2(subject.key, subject.value, "a map");
        } else if (typeEquals(subject, INT)) {
          if (stmt.names.length !== 1) {
            this.error(stmt.pos, "range over an int takes exactly one name: 'for i := range n'");
          }
          this.declare(stmt.names[0], INT, stmt.pos);
        } else if (subject.kind === "any") {
          for (const n of stmt.names) this.declare(n, ANY, stmt.pos);
        } else {
          this.error(stmt.subject.pos, `cannot range over ${typeToString(subject)}`);
          for (const n of stmt.names) this.declare(n, ANY, stmt.pos);
        }
        this.checkBlock(stmt.body);
        this.popScope();
        break;
      }
      case "send": {
        const ch = this.checkExprSingle(stmt.channel);
        const value = this.checkExprSingle(stmt.value);
        if (ch.kind === "chan") {
          if (!assignable(value, ch.elem)) {
            this.error(stmt.pos, `cannot send ${typeToString(value)} to ${typeToString(ch)}`);
          }
        } else if (ch.kind !== "any") {
          this.error(stmt.channel.pos, `cannot send to non-channel type ${typeToString(ch)}`);
        }
        break;
      }
      case "incDec": {
        const t = this.checkExprSingle(stmt.target);
        if (!isNumeric(t)) {
          this.error(stmt.pos, `'${stmt.op}' requires int or float, got ${typeToString(t)}`);
        }
        if (stmt.target.kind === "ident") {
          const binding = this.lookup(stmt.target.name);
          if (binding && !binding.mutable) {
            this.error(
              stmt.pos,
              `'${stmt.target.name}' is immutable — declare it with 'mut' to allow '${stmt.op}'`,
            );
          }
        }
        break;
      }
      case "break":
      case "continue":
        break;
    }
  }

  // `a, b := 1, 2` のような「左辺N個 vs 右辺N個」の型リストを求める
  private checkExprList(values: Expr[], targetCount: number, pos: Pos): Type[] {
    if (values.length !== targetCount) {
      this.error(pos, `expected ${targetCount} value(s), got ${values.length}`);
    }
    return values.map((v) => this.checkExprSingle(v));
  }

  // narrowing の対象になる条件か: `x is T` で x が不変な union 変数のとき
  private narrowFromCond(
    cond: Expr,
  ): { name: string; member: Type; binding: Binding } | null {
    if (cond.kind !== "is" || cond.operand.kind !== "ident") return null;
    const binding = this.lookup(cond.operand.name);
    if (!binding || binding.mutable || binding.type.kind !== "union") return null;
    return { name: cond.operand.name, member: this.resolveType(cond.target), binding };
  }

  // ---- 式 ----

  // 「単一の値」が必要な場所用: void が来たらエラー
  private checkExprSingle(expr: Expr): Type {
    const t = this.checkExpr(expr);
    if (t.kind === "prim" && t.name === "void") {
      this.error(expr.pos, "this function has no return value");
      return ANY;
    }
    return t;
  }

  private checkExpr(expr: Expr): Type {
    const t = this.inferExpr(expr);
    expr.resolvedType = t;
    return t;
  }

  private inferExpr(expr: Expr): Type {
    switch (expr.kind) {
      case "int": return INT;
      case "float": return FLOAT;
      case "string":
        // 文字列リテラルはリテラル型として推論する("active" は型 "active")。
        // string が必要な場所へは部分型として入る。mut 宣言では string に widening される
        return { kind: "literal", value: expr.value };
      case "bool": return BOOL;
      case "none": return NONE;

      case "is": {
        const t = this.checkExprSingle(expr.operand);
        const target = this.resolveType(expr.target);
        if (!isNarrowTarget(target)) {
          this.error(expr.pos, "right side of 'is' must be 'none', 'error', or 'closed' (for now)");
          return BOOL;
        }
        if (t.kind === "union") {
          if (!t.members.some((m) => typeEquals(m, target))) {
            this.error(expr.pos, `${typeToString(t)} can never be ${typeToString(target)}`);
          }
        } else if (t.kind !== "any") {
          this.error(
            expr.operand.pos,
            `'is' needs a union-typed value, got ${typeToString(t)}`,
          );
        }
        return BOOL;
      }

      case "prop": {
        const t = this.checkExprSingle(expr.operand);
        if (t.kind === "any") return ANY;
        if (t.kind !== "union") {
          this.error(expr.pos, `'!' needs a union with none/error, got ${typeToString(t)}`);
          return t;
        }
        const failures = t.members.filter(isFailure);
        if (failures.length === 0) {
          this.error(expr.pos, `'!' has nothing to propagate — ${typeToString(t)} has no none/error`);
        }
        const ret = this.retStack[this.retStack.length - 1] ?? VOID;
        for (const f of failures) {
          if (!assignable(f, ret)) {
            this.error(
              expr.pos,
              `'!' propagates ${typeToString(f)}, but this function returns ${typeToString(ret)}` +
                ` — add '${typeToString(f)}' to the return type or handle it with 'is'`,
            );
          }
        }
        return unionWithout(t, isFailure); // 成功だけが残る(空なら void = 文としてのみ使える)
      }

      case "match":
        return this.inferMatch(expr);

      case "spawn": {
        const ret = this.checkExpr(expr.call);
        // 戻り値なしの関数の spawn は「起動するだけ」(受取口なし=文としてのみ意味を持つ)
        if (typeEquals(ret, VOID)) return VOID;
        return { kind: "chan", elem: ret };
      }

      case "orElse": {
        const t = this.checkExprSingle(expr.left);
        if (t.kind === "any") {
          this.checkExprSingle(expr.right);
          return ANY;
        }
        if (t.kind !== "union" || !t.members.some(isFailure)) {
          this.error(expr.pos, `left side of 'or' never fails — it is ${typeToString(t)}`);
          this.checkExprSingle(expr.right);
          return t;
        }
        const rest = unionWithout(t, isFailure);
        if (typeEquals(rest, VOID)) {
          this.error(expr.pos, `left side of 'or' has no success value — handle it with 'is' instead`);
          this.checkExprSingle(expr.right);
          return ANY;
        }
        const r = this.checkExprSingle(expr.right);
        if (!assignable(r, rest)) {
          this.error(
            expr.right.pos,
            `'or' fallback must be ${typeToString(rest)}, got ${typeToString(r)}`,
          );
        }
        return rest;
      }

      case "interp": {
        // 補間される式は(printと同じく)どの型でもよい。結果は常に string
        for (const seg of expr.segments) {
          if (seg.kind === "expr") this.checkExprSingle(seg.expr);
        }
        return STRING;
      }

      case "ident": {
        const binding = this.lookup(expr.name);
        if (!binding) {
          if (BUILTINS.has(expr.name)) {
            this.error(expr.pos, `'${expr.name}' is a builtin function — call it like ${expr.name}(...)`);
          } else {
            this.error(expr.pos, `undefined: '${expr.name}'`);
          }
          return ANY;
        }
        return binding.type;
      }

      case "arrayLit": {
        // Todo[]{...} / int[]{...} — 要素型が明示された配列リテラル(空にできる)
        if (expr.elemType) {
          const elem = this.resolveType(expr.elemType);
          for (const e of expr.elems) {
            const t = this.checkExprSingle(e);
            if (!assignable(t, elem)) {
              this.error(e.pos, `array element must be ${typeToString(elem)}, got ${typeToString(t)}`);
            }
          }
          return { kind: "array", elem };
        }
        if (expr.elems.length === 0) return { kind: "array", elem: ANY };
        // 要素のリテラル型は widening する(["a", "b"] は "a"[] ではなく string[])
        const elem = widenLiteral(this.checkExprSingle(expr.elems[0]));
        for (let i = 1; i < expr.elems.length; i++) {
          const t = this.checkExprSingle(expr.elems[i]);
          if (!assignable(t, elem)) {
            this.error(expr.elems[i].pos, `array element must be ${typeToString(elem)}, got ${typeToString(t)}`);
          }
        }
        return { kind: "array", elem };
      }

      case "binary":
        return this.inferBinary(expr);

      case "unary": {
        const t = this.checkExprSingle(expr.operand);
        if (expr.op === "!") {
          if (!typeEquals(t, BOOL) && t.kind !== "any") {
            this.error(expr.pos, `'!' requires bool, got ${typeToString(t)}`);
          }
          return BOOL;
        }
        if (!isNumeric(t)) {
          this.error(expr.pos, `unary '-' requires int or float, got ${typeToString(t)}`);
        }
        return t;
      }

      case "recv": {
        const ch = this.checkExprSingle(expr.channel);
        // 受信は常に T | closed(mapの V | none と同じ理由: closeされうることを型で強制する)
        if (ch.kind === "chan") return unionOf([ch.elem, CLOSED]);
        if (ch.kind !== "any") {
          this.error(expr.pos, `cannot receive from non-channel type ${typeToString(ch)}`);
        }
        return ANY;
      }

      case "select":
        return this.inferSelect(expr);

      case "call":
        return this.inferCall(expr);

      case "index": {
        const target = this.checkExprSingle(expr.target);
        const index = this.checkExprSingle(expr.index);
        // map の読み取りは V | none を返す(無いキーを無視できない。union路線の帰結)
        if (target.kind === "map") {
          if (!assignable(index, target.key)) {
            this.error(expr.index.pos, `map key must be ${typeToString(target.key)}, got ${typeToString(index)}`);
          }
          return unionOf([target.value, NONE]);
        }
        if (!isNumeric(index) || (index.kind === "prim" && index.name === "float")) {
          if (index.kind !== "any") this.error(expr.index.pos, `index must be int, got ${typeToString(index)}`);
        }
        if (target.kind === "array") return target.elem;
        if (isStringy(target)) return STRING;
        if (target.kind !== "any") {
          this.error(expr.pos, `cannot index into ${typeToString(target)}`);
        }
        return ANY;
      }

      case "mapLit": {
        const key = this.resolveType(expr.key);
        const value = this.resolveType(expr.value);
        for (const e of expr.entries) {
          const kt = this.checkExprSingle(e.key);
          const vt = this.checkExprSingle(e.value);
          if (!assignable(kt, key)) {
            this.error(e.key.pos, `map key must be ${typeToString(key)}, got ${typeToString(kt)}`);
          }
          if (!assignable(vt, value)) {
            this.error(e.value.pos, `map value must be ${typeToString(value)}, got ${typeToString(vt)}`);
          }
        }
        return { kind: "map", key, value };
      }

      case "member": {
        const t = this.checkExprSingle(expr.target);
        return this.memberFieldType(expr, t);
      }

      case "structLit": {
        const resolved = this.resolveAlias(expr.name, expr.pos);
        // フィールド値は先に1回だけ検査する(候補メンバーの絞り込みにも使うので二重評価しない)
        const fieldTypes = expr.fields.map((f) => this.checkExprSingle(f.value));

        let t = resolved;
        // 判別可能union(C-1): GetUserResponse{kind: "ok", user: u} のように、union型の
        // 名前をそのまま struct リテラルの名前として使う。書かれたフィールド集合が
        // ちょうど一致するメンバーへ絞り込み、複数残るならフィールド値の型(判別フィールドの
        // 文字列リテラル値など)でさらに絞り込んで1つに特定する
        if (resolved.kind === "union") {
          const fieldNameSet = new Set(expr.fields.map((f) => f.name));
          const structMembers = resolved.members.filter((m) => m.kind === "struct");
          let candidates = structMembers.filter((m) => {
            if (m.kind !== "struct") return false;
            const memberNames = new Set(m.fields.map((f) => f.name));
            return memberNames.size === fieldNameSet.size && [...fieldNameSet].every((n) => memberNames.has(n));
          });
          if (candidates.length > 1) {
            candidates = candidates.filter(
              (m) =>
                m.kind === "struct" &&
                expr.fields.every((f, i) => {
                  const decl = m.fields.find((d) => d.name === f.name);
                  return decl !== undefined && assignable(fieldTypes[i], decl.type);
                }),
            );
          }
          if (candidates.length !== 1) {
            const shapes = structMembers
              .map((m) => (m.kind === "struct" ? `{ ${m.fields.map((f) => f.name).join(", ")} }` : ""))
              .join(" | ");
            this.error(
              expr.pos,
              candidates.length === 0
                ? `no member of '${expr.name}' matches the field(s) {${[...fieldNameSet].join(", ")}}` +
                    (shapes ? ` (union members: ${shapes})` : "")
                : `ambiguous — multiple members of '${expr.name}' match the field(s) {${[...fieldNameSet].join(", ")}}`,
            );
            return ANY;
          }
          t = candidates[0];
        }
        if (t.kind !== "struct") {
          this.error(expr.pos, `'${expr.name}' is not a struct`);
          return ANY;
        }
        // エラーメッセージ上の名前: union経由なら union の名前(メンバーは無名なので)、
        // 普通の struct ならそのまま struct 名
        const structType = t; // const に束縛し直して、以降 struct であることの絞り込みを効かせる
        const displayName = resolved.kind === "union" ? expr.name : structType.name;
        const seen = new Set<string>();
        expr.fields.forEach((f, i) => {
          if (seen.has(f.name)) {
            this.error(f.pos, `duplicate field '${f.name}'`);
            return;
          }
          seen.add(f.name);
          const decl = structType.fields.find((d) => d.name === f.name);
          if (!decl) {
            this.error(
              f.pos,
              `${displayName} has no field '${f.name}' (fields: ${structType.fields.map((d) => d.name).join(", ")})`,
            );
            return;
          }
          if (!assignable(fieldTypes[i], decl.type)) {
            this.error(
              f.value.pos,
              `field '${f.name}': cannot use ${typeToString(fieldTypes[i])} as ${typeToString(decl.type)}`,
            );
          }
        });
        // 全フィールド必須(v1。ゼロ値・デフォルト値は導入しない)
        const missing = structType.fields.filter((d) => !seen.has(d.name));
        if (missing.length > 0) {
          this.error(expr.pos, `missing field(s) in ${displayName}: ${missing.map((d) => d.name).join(", ")}`);
        }
        // 式全体の型は union 自体(narrow なメンバー型ではない)。match/is で絞り込むまでは
        // 常に union として扱う(mut var再代入・widening等を新規に考えなくて済むようにする)
        return resolved.kind === "union" ? resolved : t;
      }

      case "fnExpr": {
        const t = this.fnType(expr.params, expr.ret);
        this.checkFn(expr);
        return t;
      }

      case "chanExpr": {
        if (expr.capacity) {
          const cap = this.checkExprSingle(expr.capacity);
          if (!typeEquals(cap, INT) && cap.kind !== "any") {
            this.error(expr.capacity.pos, `channel capacity must be int, got ${typeToString(cap)}`);
          }
        }
        return { kind: "chan", elem: this.resolveType(expr.elem) };
      }
    }
  }

  private inferBinary(expr: Expr & { kind: "binary" }): Type {
    const left = this.checkExprSingle(expr.left);
    const right = this.checkExprSingle(expr.right);
    const { op } = expr;

    if (op === "&&" || op === "||") {
      for (const [t, e] of [[left, expr.left], [right, expr.right]] as const) {
        if (!typeEquals(t, BOOL) && t.kind !== "any") {
          this.error(e.pos, `'${op}' requires bool operands, got ${typeToString(t)}`);
        }
      }
      return BOOL;
    }

    if (op === "==" || op === "!=") {
      // none との比較は narrowing が効く 'is' に一本化する(P1)
      if (expr.left.kind === "none" || expr.right.kind === "none") {
        this.error(expr.pos, `use 'is none' to test for none (== does not narrow the type)`);
        return BOOL;
      }
      if (!assignable(left, right) && !assignable(right, left)) {
        this.error(expr.pos, `cannot compare ${typeToString(left)} with ${typeToString(right)}`);
      }
      return BOOL;
    }

    if (op === "<" || op === "<=" || op === ">" || op === ">=") {
      const ok = (isNumeric(left) && isNumeric(right)) ||
        (isStringy(left) && isStringy(right)) ||
        left.kind === "any" || right.kind === "any";
      if (!ok) {
        this.error(expr.pos, `cannot compare ${typeToString(left)} with ${typeToString(right)}`);
      }
      return BOOL;
    }

    // 算術演算: + - * / %
    if (op === "+" && isStringy(left) && isStringy(right)) {
      return STRING;
    }
    if (isNumeric(left) && isNumeric(right)) {
      const isInt = typeEquals(left, INT) && typeEquals(right, INT);
      if (op === "/" && isInt) expr.intDiv = true; // int同士の除算は切り捨て+ゼロ検査
      if (op === "%" && isInt) expr.intMod = true;
      // リテラルの 0 で割るのは実行するまでもなくバグ。コンパイル時に弾く
      if (isInt && (op === "/" || op === "%") && expr.right.kind === "int" && expr.right.value === "0") {
        this.error(expr.right.pos, `integer ${op === "/" ? "division" : "modulo"} by zero`);
      }
      if (left.kind === "any" || right.kind === "any") return ANY;
      return isInt ? INT : FLOAT;
    }
    if (left.kind === "any" || right.kind === "any") return ANY;
    this.error(
      expr.pos,
      `invalid operation: ${typeToString(left)} ${op} ${typeToString(right)}` +
        (op === "+" && (typeEquals(left, STRING) || typeEquals(right, STRING))
          ? " (hint: use str() to convert values to string)"
          : ""),
    );
    return ANY;
  }

  // 判別可能union用: パターンが構造体型メンバーの「部分形」として一致するか。
  // パターンに書かれたフィールドが全部あって型が一致すればよい(書かれてないフィールドは無視。
  // { kind: "ok" } は user フィールドの有無を問わず kind: "ok" を持つメンバーに一致する)
  private structPatternMatches(member: Type, pattern: Type): boolean {
    if (member.kind !== "struct" || pattern.kind !== "struct") return typeEquals(member, pattern);
    return pattern.fields.every((pf) => {
      const mf = member.fields.find((f) => f.name === pf.name);
      return mf !== undefined && typeEquals(mf.type, pf.type);
    });
  }

  // match式: 型パターンで union を分解する。網羅性検査とアーム内 narrowing はここ
  private inferMatch(expr: Expr & { kind: "match" }): Type {
    const subject = this.checkExprSingle(expr.subject);
    if (expr.arms.length === 0) {
      this.error(expr.pos, "match must have at least one arm");
      return ANY;
    }
    if (subject.kind !== "union" && subject.kind !== "any") {
      this.error(expr.subject.pos, `match subject must be a union type, got ${typeToString(subject)}`);
    }
    const members = subject.kind === "union" ? subject.members : null;

    // 対象が不変な変数なら、アーム内でその変数を絞り込める
    const narrowName =
      expr.subject.kind === "ident" && this.lookup(expr.subject.name)?.mutable === false
        ? expr.subject.name
        : null;

    const covered: Type[] = [];
    const armTypes: Type[] = [];
    let sawWildcard = false;

    for (const arm of expr.arms) {
      if (sawWildcard) {
        this.error(arm.pos, "unreachable arm — '_' already matches everything before this");
        continue;
      }
      const patternTypes: Type[] = [];
      for (const p of arm.patterns) {
        if (p.kind === "wildcard") {
          if (arm.patterns.length > 1) this.error(p.pos, "'_' must be the only pattern in its arm");
          sawWildcard = true;
          continue;
        }
        const pt = this.resolveType(p.type);
        // 判別可能union: { kind: "ok" } のような部分構造パターンは、書かれたフィールドが
        // 一致する union メンバー(具体的な形)へ解決してから、通常の型パターンと同じに扱う
        // (1個のパターンが複数メンバーに一致することもある — その場合は両方カバーしたことにする)。
        // 通常の型パターン(int/error/...)は今まで通り「union の実メンバーか」をそのまま検査する
        let resolvedPatterns: Type[];
        if (pt.kind === "struct" && members) {
          resolvedPatterns = members.filter((m) => this.structPatternMatches(m, pt));
        } else if (members && !members.some((m) => typeEquals(m, pt))) {
          resolvedPatterns = [];
        } else {
          resolvedPatterns = [pt];
        }
        if (members && resolvedPatterns.length === 0) {
          this.error(arm.pos, `${typeToString(subject)} can never be ${typeToString(pt)}`);
        }
        for (const rp of resolvedPatterns) {
          if (members && covered.some((c) => typeEquals(c, rp))) {
            this.error(arm.pos, `unreachable pattern — ${typeToString(rp)} is already covered`);
          }
          covered.push(rp);
          patternTypes.push(rp);
        }
      }

      // アーム内の型: 型パターンならその union、ワイルドカードなら「残り全部」
      const narrowedTo = sawWildcard && patternTypes.length === 0 && members
        ? unionOf(members.filter((m) => !covered.some((c) => typeEquals(c, m))))
        : unionOf(patternTypes.length > 0 ? patternTypes : [ANY]);

      this.pushScope();
      if (narrowName) {
        this.scopes[this.scopes.length - 1].set(narrowName, { type: narrowedTo, mutable: false });
      }
      armTypes.push(this.checkExpr(arm.body));
      this.popScope();
    }

    // 網羅性検査: union の全メンバーがカバーされているか
    if (members && !sawWildcard) {
      const missing = members.filter((m) => !covered.some((c) => typeEquals(c, m)));
      if (missing.length > 0) {
        this.error(
          expr.pos,
          `match is not exhaustive — missing: ${missing.map(typeToString).join(", ")}` +
            ` (add arms for them, or a '_' arm)`,
        );
      }
    }

    // 結果型: 全アーム void なら void(文として使う)、そうでなければアームの union
    const voids = armTypes.filter((t) => typeEquals(t, VOID));
    if (voids.length === armTypes.length) return VOID;
    if (voids.length > 0) {
      this.error(expr.pos, "match arms mix values and void — all arms must return a value, or none");
      return ANY;
    }
    return unionOf(armTypes);
  }

  // select式: 複数チャネルのうちどれかが準備できたら、そのアームを評価する。
  // matchと見た目は揃えるが、パターンは「型」ではなく「どのチャネル操作が先に終わったか」
  private inferSelect(expr: Expr & { kind: "select" }): Type {
    if (expr.arms.length === 0 && !expr.defaultArm) {
      this.error(expr.pos, "select must have at least one arm");
      return ANY;
    }
    const armTypes: Type[] = [];
    for (const arm of expr.arms) {
      const chType = this.checkExprSingle(arm.channel);
      let bindingType: Type = ANY;
      if (chType.kind === "chan") {
        bindingType = unionOf([chType.elem, CLOSED]);
      } else if (chType.kind !== "any") {
        this.error(arm.channel.pos, `select arm requires a channel, got ${typeToString(chType)}`);
      }
      this.pushScope();
      this.declare(arm.name, bindingType, arm.pos);
      armTypes.push(this.checkExpr(arm.body));
      this.popScope();
    }
    if (expr.defaultArm) {
      armTypes.push(this.checkExpr(expr.defaultArm));
    }

    const voids = armTypes.filter((t) => typeEquals(t, VOID));
    if (voids.length === armTypes.length) return VOID;
    if (voids.length > 0) {
      this.error(expr.pos, "select arms mix values and void — all arms must return a value, or none");
      return ANY;
    }
    return unionOf(armTypes);
  }

  private inferCall(expr: Expr & { kind: "call" }): Type {
    // 組み込み関数(シャドーイング禁止なので名前で判定できる)
    if (expr.callee.kind === "ident" && BUILTINS.has(expr.callee.name)) {
      return this.inferBuiltinCall(expr.callee.name, expr);
    }

    // recv.method(args) — struct のメソッドとして解決を試みる。
    // target の型はここで一度だけ評価する(呼び出し不成立時のフォールバックでも
    // 再評価しない。二重評価すると undefined 変数などのエラーが2回出てしまう)
    if (expr.callee.kind === "member") {
      const member = expr.callee;
      const targetType = this.checkExprSingle(member.target);

      if (targetType.kind === "struct" && !targetType.fields.some((f) => f.name === member.name)) {
        const methodType = this.methodTable.get(targetType.name)?.get(member.name);
        if (methodType && methodType.kind === "fn") {
          member.resolvedType = methodType;
          const [, ...paramsWithoutReceiver] = methodType.params;
          return this.checkCallArgs(expr, paramsWithoutReceiver, methodType.ret);
        }
        this.error(
          member.pos,
          `${typeToString(targetType)} has no field or method '${member.name}'` +
            ` (fields: ${targetType.fields.map((f) => f.name).join(", ") || "none"})`,
        );
        member.resolvedType = ANY;
        expr.args.forEach((a) => this.checkExprSingle(a)); // 引数側もチェックしてエラーの連鎖を減らす
        return ANY;
      }

      // メソッド対象でなければ、member を通常どおり「値」として評価し、それを呼び出す
      // (struct フィールドが関数値のケース・union未絞り込み・非structなど)
      const memberType = this.memberFieldType(member, targetType);
      member.resolvedType = memberType;
      return this.checkCallOfValue(expr, memberType);
    }

    const callee = this.checkExprSingle(expr.callee);
    return this.checkCallOfValue(expr, callee);
  }

  // struct フィールドアクセスの型検査(member式・メソッド呼び出しの両方から使う共通部分)。
  // target の型は呼び出し元が(二重評価を避けるため)既に確定させて渡す
  private memberFieldType(member: Expr & { kind: "member" }, targetType: Type): Type {
    if (targetType.kind === "struct") {
      const field = targetType.fields.find((f) => f.name === member.name);
      if (field) return field.type;
      if (this.methodTable.get(targetType.name)?.has(member.name)) {
        this.error(member.pos, `'${member.name}' is a method — call it like ${member.name}(...)`);
        return ANY;
      }
      this.error(
        member.pos,
        `${typeToString(targetType)} has no field '${member.name}'` +
          ` (fields: ${targetType.fields.map((f) => f.name).join(", ")})`,
      );
      return ANY;
    }
    if (targetType.kind === "union") {
      this.error(
        member.pos,
        `cannot access field or method on ${typeToString(targetType)} — narrow it first (with 'is' or 'match')`,
      );
      return ANY;
    }
    if (targetType.kind !== "any") {
      this.error(member.pos, `${typeToString(targetType)} has no fields`);
    }
    return ANY;
  }

  // 引数リストを既知の paramTypes と照合する(メソッド呼び出し用。callee自体は常にfnなので
  // 「呼べない型」チェックは不要)
  private checkCallArgs(callExpr: Expr & { kind: "call" }, paramTypes: Type[], retType: Type): Type {
    const args = callExpr.args.map((a) => this.checkExprSingle(a));
    if (args.length !== paramTypes.length) {
      this.error(callExpr.pos, `expected ${paramTypes.length} argument(s), got ${args.length}`);
    }
    for (let i = 0; i < Math.min(args.length, paramTypes.length); i++) {
      if (!assignable(args[i], paramTypes[i])) {
        this.error(
          callExpr.args[i].pos,
          `argument ${i + 1}: cannot use ${typeToString(args[i])} as ${typeToString(paramTypes[i])}`,
        );
      }
    }
    return retType;
  }

  // callee の型が分かっている状態からの呼び出し検査(通常の関数呼び出し・
  // structフィールドが関数値のケースの両方で使う)
  private checkCallOfValue(callExpr: Expr & { kind: "call" }, calleeType: Type): Type {
    const args = callExpr.args.map((a) => this.checkExprSingle(a));
    if (calleeType.kind === "any") return ANY;
    if (calleeType.kind !== "fn") {
      this.error(callExpr.pos, `cannot call non-function type ${typeToString(calleeType)}`);
      return ANY;
    }
    if (args.length !== calleeType.params.length) {
      this.error(callExpr.pos, `expected ${calleeType.params.length} argument(s), got ${args.length}`);
    }
    for (let i = 0; i < Math.min(args.length, calleeType.params.length); i++) {
      if (!assignable(args[i], calleeType.params[i])) {
        this.error(
          callExpr.args[i].pos,
          `argument ${i + 1}: cannot use ${typeToString(args[i])} as ${typeToString(calleeType.params[i])}`,
        );
      }
    }
    return calleeType.ret;
  }

  private inferBuiltinCall(name: string, expr: Expr & { kind: "call" }): Type {
    const args = expr.args.map((a) => this.checkExprSingle(a));
    const expectArity = (n: number): boolean => {
      if (args.length !== n) {
        this.error(expr.pos, `${name}() expects ${n} argument(s), got ${args.length}`);
        return false;
      }
      return true;
    };

    switch (name) {
      case "print":
        return VOID; // 可変長・任意型
      case "str":
        expectArity(1);
        return STRING;
      case "len": {
        if (expectArity(1)) {
          const t = args[0];
          const ok = t.kind === "array" || t.kind === "map" || t.kind === "any" || isStringy(t);
          if (!ok) this.error(expr.args[0].pos, `len() requires string, array or map, got ${typeToString(t)}`);
        }
        return INT;
      }
      case "push": {
        if (expectArity(2)) {
          const arr = args[0];
          if (arr.kind === "array") {
            if (!assignable(args[1], arr.elem)) {
              this.error(expr.args[1].pos, `cannot push ${typeToString(args[1])} into ${typeToString(arr)}`);
            }
          } else if (arr.kind !== "any") {
            this.error(expr.args[0].pos, `push() requires an array, got ${typeToString(arr)}`);
          }
        }
        return VOID;
      }
      case "error": {
        if (expectArity(1) && !assignable(args[0], STRING)) {
          this.error(expr.args[0].pos, `error() requires a string message, got ${typeToString(args[0])}`);
        }
        return ERROR;
      }
      case "delete": {
        if (expectArity(2)) {
          const m = args[0];
          if (m.kind === "map") {
            if (!assignable(args[1], m.key)) {
              this.error(expr.args[1].pos, `map key must be ${typeToString(m.key)}, got ${typeToString(args[1])}`);
            }
          } else if (m.kind !== "any") {
            this.error(expr.args[0].pos, `delete() requires a map, got ${typeToString(m)}`);
          }
        }
        return VOID;
      }
      case "sleep": {
        if (expectArity(1) && !isNumeric(args[0])) {
          this.error(expr.args[0].pos, `sleep() requires milliseconds (int), got ${typeToString(args[0])}`);
        }
        return VOID;
      }
      case "contains": {
        if (expectArity(2)) {
          const arr = args[0];
          if (arr.kind === "array") {
            if (!assignable(args[1], arr.elem)) {
              this.error(
                expr.args[1].pos,
                `contains() second argument must be ${typeToString(arr.elem)}, got ${typeToString(args[1])}`,
              );
            }
          } else if (arr.kind !== "any") {
            this.error(expr.args[0].pos, `contains() requires an array, got ${typeToString(arr)}`);
          }
        }
        return BOOL;
      }
      case "indexOf": {
        if (expectArity(2)) {
          const arr = args[0];
          if (arr.kind === "array") {
            if (!assignable(args[1], arr.elem)) {
              this.error(
                expr.args[1].pos,
                `indexOf() second argument must be ${typeToString(arr.elem)}, got ${typeToString(args[1])}`,
              );
            }
          } else if (arr.kind !== "any") {
            this.error(expr.args[0].pos, `indexOf() requires an array, got ${typeToString(arr)}`);
          }
        }
        return unionOf([INT, NONE]);
      }
      case "keys": {
        if (!expectArity(1)) return { kind: "array", elem: ANY };
        const m = args[0];
        if (m.kind === "map") return { kind: "array", elem: m.key };
        if (m.kind !== "any") this.error(expr.args[0].pos, `keys() requires a map, got ${typeToString(m)}`);
        return { kind: "array", elem: ANY };
      }
      case "values": {
        if (!expectArity(1)) return { kind: "array", elem: ANY };
        const m = args[0];
        if (m.kind === "map") return { kind: "array", elem: m.value };
        if (m.kind !== "any") this.error(expr.args[0].pos, `values() requires a map, got ${typeToString(m)}`);
        return { kind: "array", elem: ANY };
      }
      case "sort": {
        if (expectArity(1)) {
          const arr = args[0];
          if (arr.kind === "array") {
            if (!isNumeric(arr.elem) && !isStringy(arr.elem)) {
              this.error(
                expr.args[0].pos,
                `sort() requires int[], float[] or string[], got ${typeToString(arr)}`,
              );
            }
          } else if (arr.kind !== "any") {
            this.error(expr.args[0].pos, `sort() requires an array, got ${typeToString(arr)}`);
          }
        }
        // 非破壊(new arrayを返す)。引数の配列自体は変わらない
        return args[0]?.kind === "array" ? args[0] : ANY;
      }
      case "split": {
        if (expectArity(2)) {
          if (!isStringy(args[0])) {
            this.error(expr.args[0].pos, `split() requires a string, got ${typeToString(args[0])}`);
          }
          if (!isStringy(args[1])) {
            this.error(expr.args[1].pos, `split() separator must be a string, got ${typeToString(args[1])}`);
          }
        }
        return { kind: "array", elem: STRING };
      }
      case "join": {
        if (expectArity(2)) {
          const arr = args[0];
          if (arr.kind === "array") {
            if (!isStringy(arr.elem) && arr.elem.kind !== "any") {
              this.error(expr.args[0].pos, `join() requires string[], got ${typeToString(arr)}`);
            }
          } else if (arr.kind !== "any") {
            this.error(expr.args[0].pos, `join() requires an array, got ${typeToString(arr)}`);
          }
          if (!isStringy(args[1])) {
            this.error(expr.args[1].pos, `join() separator must be a string, got ${typeToString(args[1])}`);
          }
        }
        return STRING;
      }
      case "trim":
      case "upper":
      case "lower": {
        if (expectArity(1) && !isStringy(args[0])) {
          this.error(expr.args[0].pos, `${name}() requires a string, got ${typeToString(args[0])}`);
        }
        return STRING;
      }
      case "toInt": {
        if (expectArity(1) && !isStringy(args[0])) {
          this.error(expr.args[0].pos, `toInt() requires a string, got ${typeToString(args[0])}`);
        }
        return unionOf([INT, ERROR]);
      }
      case "close": {
        if (expectArity(1)) {
          const ch = args[0];
          if (ch.kind !== "chan" && ch.kind !== "any") {
            this.error(expr.args[0].pos, `close() requires a channel, got ${typeToString(ch)}`);
          }
        }
        return VOID;
      }
      case "filter": {
        if (expectArity(2)) {
          const arr = args[0];
          const pred = args[1];
          if (arr.kind === "array") {
            if (pred.kind === "fn") {
              if (pred.params.length !== 1 || !assignable(arr.elem, pred.params[0])) {
                this.error(
                  expr.args[1].pos,
                  `filter() callback must take a single ${typeToString(arr.elem)} parameter`,
                );
              }
              if (!typeEquals(pred.ret, BOOL) && pred.ret.kind !== "any") {
                this.error(expr.args[1].pos, `filter() callback must return bool, got ${typeToString(pred.ret)}`);
              }
            } else if (pred.kind !== "any") {
              this.error(expr.args[1].pos, `filter() second argument must be a function, got ${typeToString(pred)}`);
            }
          } else if (arr.kind !== "any") {
            this.error(expr.args[0].pos, `filter() requires an array, got ${typeToString(arr)}`);
          }
        }
        return args[0]?.kind === "array" ? args[0] : ANY;
      }
      case "transform": {
        if (expectArity(2)) {
          const arr = args[0];
          const f = args[1];
          if (arr.kind === "array") {
            if (f.kind === "fn") {
              if (f.params.length !== 1 || !assignable(arr.elem, f.params[0])) {
                this.error(
                  expr.args[1].pos,
                  `transform() callback must take a single ${typeToString(arr.elem)} parameter`,
                );
              }
            } else if (f.kind !== "any") {
              this.error(expr.args[1].pos, `transform() second argument must be a function, got ${typeToString(f)}`);
            }
          } else if (arr.kind !== "any") {
            this.error(expr.args[0].pos, `transform() requires an array, got ${typeToString(arr)}`);
          }
        }
        const f = args[1];
        return { kind: "array", elem: f?.kind === "fn" ? f.ret : ANY };
      }
      case "reduce": {
        if (expectArity(3)) {
          const arr = args[0];
          const f = args[1];
          const init = args[2];
          if (arr.kind === "array") {
            if (f.kind === "fn") {
              if (f.params.length !== 2) {
                this.error(expr.args[1].pos, `reduce() callback must take (accumulator, element)`);
              } else {
                if (!assignable(init, f.params[0])) {
                  this.error(
                    expr.args[2].pos,
                    `reduce() initial value must be ${typeToString(f.params[0])}, got ${typeToString(init)}`,
                  );
                }
                if (!assignable(arr.elem, f.params[1])) {
                  this.error(
                    expr.args[1].pos,
                    `reduce() callback's second parameter must accept ${typeToString(arr.elem)}`,
                  );
                }
                if (!assignable(f.ret, f.params[0])) {
                  this.error(
                    expr.args[1].pos,
                    `reduce() callback must return ${typeToString(f.params[0])} (the accumulator type), got ${typeToString(f.ret)}`,
                  );
                }
              }
            } else if (f.kind !== "any") {
              this.error(expr.args[1].pos, `reduce() second argument must be a function, got ${typeToString(f)}`);
            }
          } else if (arr.kind !== "any") {
            this.error(expr.args[0].pos, `reduce() requires an array, got ${typeToString(arr)}`);
          }
        }
        const f = args[1];
        if (f?.kind === "fn" && f.params.length === 2) return f.params[0];
        return args[2] ?? ANY;
      }
      default:
        return ANY;
    }
  }
}
