// Checker: AST を歩いて型の矛盾を探す。ここが「TypeScriptらしさ」の心臓部。
// - `:=` の右辺から型を推論して変数に記録する
// - 関数呼び出しの引数の数と型を照合する
// - 検査しながら式に resolvedType を書き込み、Codegen へ引き継ぐ

import type { Block, Expr, FnDecl, FnExpr, Program, Stmt, TypeNode } from "./ast";
import type { Pos } from "./token";
import {
  ANY,
  BOOL,
  ERROR,
  FLOAT,
  INT,
  NONE,
  STRING,
  VOID,
  assignable,
  isFailure,
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
const BUILTIN_TYPE_NAMES = new Set(["int", "float", "string", "bool", "void", "error", "none", "any"]);

export interface Diagnostic {
  pos: Pos;
  message: string;
}

// 組み込み関数。特殊な検査(可変長引数など)は checkCall 内で行う。
export const BUILTINS = new Set(["print", "len", "push", "str", "error", "sleep", "delete"]);

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

    // 先に全関数のシグネチャを登録する(前方参照・相互再帰を許すため)
    for (const fn of program.fns) {
      this.declare(fn.name, this.fnType(fn.params, fn.ret), fn.pos);
    }

    const main = program.fns.find((f) => f.name === "main");
    if (!main) {
      this.error({ line: 1, col: 1 }, "missing 'fn main()' — Mesh programs start from main");
    } else if (main.params.length > 0 || main.ret !== null) {
      this.error(main.pos, "'fn main()' must take no parameters and return nothing");
    }

    for (const fn of program.fns) this.checkFn(fn);
    return this.diagnostics;
  }

  private checkFn(fn: FnDecl | FnExpr) {
    this.pushScope();
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
        if (!isFailure(target)) {
          this.error(expr.pos, "right side of 'is' must be 'none' or 'error' (for now)");
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
        if (ch.kind === "chan") return ch.elem;
        if (ch.kind !== "any") {
          this.error(expr.pos, `cannot receive from non-channel type ${typeToString(ch)}`);
        }
        return ANY;
      }

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
        if (t.kind === "struct") {
          const field = t.fields.find((f) => f.name === expr.name);
          if (!field) {
            this.error(
              expr.pos,
              `${t.name} has no field '${expr.name}'` +
                ` (fields: ${t.fields.map((f) => f.name).join(", ")})`,
            );
            return ANY;
          }
          return field.type;
        }
        if (t.kind === "union") {
          this.error(
            expr.pos,
            `cannot access field on ${typeToString(t)} — narrow it first (with 'is' or 'match')`,
          );
          return ANY;
        }
        if (t.kind !== "any") {
          this.error(expr.pos, `${typeToString(t)} has no fields`);
        }
        return ANY;
      }

      case "structLit": {
        const t = this.resolveAlias(expr.name, expr.pos);
        if (t.kind !== "struct") {
          this.error(expr.pos, `'${expr.name}' is not a struct`);
          for (const f of expr.fields) this.checkExprSingle(f.value);
          return ANY;
        }
        const seen = new Set<string>();
        for (const f of expr.fields) {
          if (seen.has(f.name)) {
            this.error(f.pos, `duplicate field '${f.name}'`);
            continue;
          }
          seen.add(f.name);
          const decl = t.fields.find((d) => d.name === f.name);
          const vt = this.checkExprSingle(f.value);
          if (!decl) {
            this.error(
              f.pos,
              `${t.name} has no field '${f.name}' (fields: ${t.fields.map((d) => d.name).join(", ")})`,
            );
            continue;
          }
          if (!assignable(vt, decl.type)) {
            this.error(f.value.pos, `field '${f.name}': cannot use ${typeToString(vt)} as ${typeToString(decl.type)}`);
          }
        }
        // 全フィールド必須(v1。ゼロ値・デフォルト値は導入しない)
        const missing = t.fields.filter((d) => !seen.has(d.name));
        if (missing.length > 0) {
          this.error(expr.pos, `missing field(s) in ${t.name}: ${missing.map((d) => d.name).join(", ")}`);
        }
        return t;
      }

      case "fnExpr": {
        const t = this.fnType(expr.params, expr.ret);
        this.checkFn(expr);
        return t;
      }

      case "chanExpr":
        return { kind: "chan", elem: this.resolveType(expr.elem) };
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
        if (members) {
          if (!members.some((m) => typeEquals(m, pt))) {
            this.error(arm.pos, `${typeToString(subject)} can never be ${typeToString(pt)}`);
          } else if (covered.some((c) => typeEquals(c, pt))) {
            this.error(arm.pos, `unreachable pattern — ${typeToString(pt)} is already covered`);
          }
        }
        covered.push(pt);
        patternTypes.push(pt);
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

  private inferCall(expr: Expr & { kind: "call" }): Type {
    // 組み込み関数(シャドーイング禁止なので名前で判定できる)
    if (expr.callee.kind === "ident" && BUILTINS.has(expr.callee.name)) {
      return this.inferBuiltinCall(expr.callee.name, expr);
    }

    const callee = this.checkExprSingle(expr.callee);
    const args = expr.args.map((a) => this.checkExprSingle(a));

    if (callee.kind === "any") return ANY;
    if (callee.kind !== "fn") {
      this.error(expr.pos, `cannot call non-function type ${typeToString(callee)}`);
      return ANY;
    }
    if (args.length !== callee.params.length) {
      this.error(expr.pos, `expected ${callee.params.length} argument(s), got ${args.length}`);
    }
    for (let i = 0; i < Math.min(args.length, callee.params.length); i++) {
      if (!assignable(args[i], callee.params[i])) {
        this.error(
          expr.args[i].pos,
          `argument ${i + 1}: cannot use ${typeToString(args[i])} as ${typeToString(callee.params[i])}`,
        );
      }
    }
    return callee.ret;
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
      default:
        return ANY;
    }
  }
}
