// Parser: トークン列を AST に組み立てる。
// 手法は「再帰下降構文解析」— 文法規則ひとつが関数ひとつに対応する、
// Go 本家や TypeScript 本家のコンパイラでも使われている定番の書き方。

import type {
  Block,
  CallExpr,
  Expr,
  FnDecl,
  IfStmt,
  InterpSegment,
  MatchArm,
  MatchPattern,
  Param,
  Program,
  Stmt,
  StructFieldNode,
  TypeDecl,
  TypeNode,
} from "./ast";
import { lex } from "./lexer";
import { CompileError, type Pos, type Token, type TokenType } from "./token";

// 二項演算子の優先順位(大きいほど強く結合する)
const PRECEDENCE: Record<string, number> = {
  or: 1, // f() or fallback は最も弱く結合する
  "||": 2,
  "&&": 3,
  "==": 4,
  "!=": 4,
  is: 4, // x is none
  "<": 5,
  "<=": 5,
  ">": 5,
  ">=": 5,
  "+": 6,
  "-": 6,
  "*": 7,
  "/": 7,
  "%": 7,
};

export function parse(source: string): Program {
  return new Parser(lex(source)).parseProgram();
}

class Parser {
  private pos = 0;
  // if/for/match のヘッダでは `User{...}` を禁止する(ブロック開始の `{` と曖昧になるため。Goと同じ規則)
  private allowStructLit = true;

  constructor(private tokens: Token[]) {}

  private withoutStructLit<T>(parse: () => T): T {
    const saved = this.allowStructLit;
    this.allowStructLit = false;
    try {
      return parse();
    } finally {
      this.allowStructLit = saved;
    }
  }

  // ---- トークン操作ユーティリティ ----

  private peek(offset = 0): Token {
    return this.tokens[Math.min(this.pos + offset, this.tokens.length - 1)];
  }

  private next(): Token {
    const t = this.peek();
    if (t.type !== "eof") this.pos++;
    return t;
  }

  private check(type: TokenType): boolean {
    return this.peek().type === type;
  }

  private match(type: TokenType): boolean {
    if (this.check(type)) {
      this.next();
      return true;
    }
    return false;
  }

  private expect(type: TokenType, context: string): Token {
    if (this.check(type)) return this.next();
    const t = this.peek();
    const got = t.type === "eof" ? "end of file" : `'${t.value}'`;
    throw new CompileError(`expected '${type}' ${context}, but got ${got}`, t.pos);
  }

  private skipSemis() {
    while (this.match(";")) {}
  }

  // ---- 宣言 ----

  parseProgram(): Program {
    const fns: FnDecl[] = [];
    const types: TypeDecl[] = [];
    this.skipSemis();
    while (!this.check("eof")) {
      if (this.check("type")) {
        types.push(this.parseTypeDecl());
      } else if (this.check("struct")) {
        types.push(this.parseStructDecl());
      } else if (this.check("fn")) {
        fns.push(this.parseFnDecl());
      } else {
        throw new CompileError(
          "only 'fn', 'struct' and 'type' declarations are allowed at the top level",
          this.peek().pos,
        );
      }
      this.skipSemis();
    }
    return { kind: "program", types, fns };
  }

  // struct User { name: string  age: int } — 意味的には型への名付け(typeと同じ)なので
  // TypeDecl として登録する。フィールドは改行区切り
  private parseStructDecl(): TypeDecl {
    const start = this.expect("struct", "at start of struct declaration");
    const name = this.expect("ident", "as struct name").value;
    this.expect("{", "after struct name");
    this.skipSemis();
    const fields: StructFieldNode[] = [];
    while (!this.check("}") && !this.check("eof")) {
      const fname = this.expect("ident", "as field name");
      this.expect(":", "after field name");
      const type = this.parseType();
      fields.push({ name: fname.value, type, pos: fname.pos });
      this.skipSemis();
    }
    this.expect("}", "at end of struct declaration");
    return {
      kind: "typeDecl",
      name,
      node: { kind: "structType", fields, pos: start.pos },
      pos: start.pos,
    };
  }

  // type Status = "active" | "banned"
  private parseTypeDecl(): TypeDecl {
    const start = this.expect("type", "at start of type declaration");
    const name = this.expect("ident", "as type name").value;
    this.expect("=", "after type name");
    if (this.check("{")) {
      // B-5決定: データの形は struct で定義する(typeの右辺に裸の {...} は書けない)
      throw new CompileError(
        `use 'struct ${name} { ... }' to define a data shape ('type' is for unions and aliases)`,
        this.peek().pos,
      );
    }
    const node = this.parseType();
    return { kind: "typeDecl", name, node, pos: start.pos };
  }

  private parseFnDecl(): FnDecl {
    const start = this.expect("fn", "at start of function declaration");
    const name = this.expect("ident", "as function name").value;
    const params = this.parseParams();
    const ret = this.parseReturnType();
    const body = this.parseBlock();
    return { kind: "fnDecl", name, params, ret, body, pos: start.pos };
  }

  private parseParams(): Param[] {
    this.expect("(", "after function name");
    const params: Param[] = [];
    while (!this.check(")")) {
      const nameTok = this.expect("ident", "as parameter name");
      this.expect(":", "after parameter name");
      const type = this.parseType();
      params.push({ name: nameTok.value, type, pos: nameTok.pos });
      if (!this.check(")")) this.expect(",", "between parameters");
    }
    this.expect(")", "after parameters");
    return params;
  }

  // 戻り値の型: なし / 単一 `int` / union `int | error`(多値戻りは廃止)
  private parseReturnType(): TypeNode | null {
    if (this.check("{")) return null;
    if (this.check("(")) {
      throw new CompileError(
        "multiple return values were removed — return one value (use a union type like 'int | error')",
        this.peek().pos,
      );
    }
    return this.parseType();
  }

  // 型 = 単一型を "|" でつないだもの: User | none | error
  private parseType(): TypeNode {
    const first = this.parseSingleType();
    if (!this.check("|")) return first;
    const members: TypeNode[] = [first];
    while (this.match("|")) members.push(this.parseSingleType());
    return { kind: "union", members, pos: first.pos };
  }

  private parseMatchPattern(): MatchPattern {
    const t = this.peek();
    if (t.type === "ident" && t.value === "_") {
      this.next();
      return { kind: "wildcard", pos: t.pos };
    }
    return { kind: "type", type: this.parseSingleType() };
  }

  private parseSingleType(): TypeNode {
    // 文字列リテラル型: "active"
    if (this.check("string")) {
      const t = this.next();
      if (t.parts) {
        throw new CompileError("interpolation cannot be used in a type", t.pos);
      }
      return { kind: "literal", value: t.value, pos: t.pos };
    }
    if (this.check("chan")) {
      const start = this.next();
      this.expect("<", "after 'chan'");
      const elem = this.parseType();
      this.expect(">", "after channel element type");
      return { kind: "chan", elem, pos: start.pos };
    }
    if (this.check("none")) {
      const t = this.next();
      return { kind: "name", name: "none", pos: t.pos };
    }
    const nameTok = this.expect("ident", "as type name");
    let type: TypeNode = { kind: "name", name: nameTok.value, pos: nameTok.pos };
    // int[] / int[][] のような配列型
    while (this.check("[") && this.peek(1).type === "]") {
      this.next();
      this.next();
      type = { kind: "array", elem: type, pos: nameTok.pos };
    }
    return type;
  }

  // ---- 文 ----

  private parseBlock(): Block {
    this.expect("{", "at start of block");
    const stmts: Stmt[] = [];
    this.skipSemis();
    while (!this.check("}") && !this.check("eof")) {
      stmts.push(this.parseStatement());
      this.skipSemis();
    }
    this.expect("}", "at end of block");
    return { kind: "block", stmts };
  }

  private parseStatement(): Stmt {
    const t = this.peek();
    switch (t.type) {
      case "return": {
        this.next();
        let value: Expr | null = null;
        if (!this.check(";") && !this.check("}")) {
          value = this.parseExpr();
          if (this.check(",")) {
            throw new CompileError(
              "multiple return values were removed — return one value (use a union type like 'int | error')",
              this.peek().pos,
            );
          }
        }
        return { kind: "return", value, pos: t.pos };
      }
      case "if":
        return this.parseIf();
      case "for":
        return this.parseFor();
      case "go": {
        this.next();
        const call = this.parseExpr();
        if (call.kind !== "call") {
          throw new CompileError("'go' must be followed by a function call", t.pos);
        }
        return { kind: "go", call: call as CallExpr, pos: t.pos };
      }
      case "break":
        this.next();
        return { kind: "break", pos: t.pos };
      case "continue":
        this.next();
        return { kind: "continue", pos: t.pos };
      default:
        return this.parseSimpleStmt();
    }
  }

  // 「単純文」: 代入 / 短縮変数宣言 / インクリメント / チャネル送信 / 式文。
  // for 文のヘッダにも現れるので独立した関数にしている。
  private parseSimpleStmt(): Stmt {
    const start = this.peek();

    // mut x := ...(可変宣言)。mut は := 宣言の前にしか置けない
    const mutable = this.match("mut");

    const first = this.parseExpr();

    // x := ... / x, y := ... / x = ... / x, y = f()
    if (this.check(",") || this.check(":=") || this.check("=")) {
      const targets: Expr[] = [first];
      while (this.match(",")) targets.push(this.parseExpr());

      if (this.match(":=")) {
        const names = targets.map((e) => {
          if (e.kind !== "ident") throw new CompileError("left side of ':=' must be a name", e.pos);
          return e.name;
        });
        const values = [this.parseExpr()];
        while (this.match(",")) values.push(this.parseExpr());
        return { kind: "shortVarDecl", names, values, mutable, pos: start.pos };
      }
      if (mutable) {
        throw new CompileError("'mut' can only be used with a ':=' declaration", start.pos);
      }

      this.expect("=", "in assignment");
      for (const e of targets) {
        if (e.kind !== "ident" && e.kind !== "index" && e.kind !== "member") {
          throw new CompileError("invalid assignment target", e.pos);
        }
      }
      const values = [this.parseExpr()];
      while (this.match(",")) values.push(this.parseExpr());
      return { kind: "assign", targets, values, pos: start.pos };
    }

    if (mutable) {
      throw new CompileError("'mut' can only be used with a ':=' declaration", start.pos);
    }

    // i++ / i--
    if (this.check("++") || this.check("--")) {
      const op = this.next().type as "++" | "--";
      return { kind: "incDec", target: first, op, pos: start.pos };
    }

    // ch <- v
    if (this.match("<-")) {
      const value = this.parseExpr();
      return { kind: "send", channel: first, value, pos: start.pos };
    }

    return { kind: "exprStmt", expr: first, pos: start.pos };
  }

  private parseIf(): IfStmt {
    const start = this.expect("if", "at start of if statement");
    const cond = this.withoutStructLit(() => this.parseExpr()); // Go と同じく条件に丸括弧は不要
    const then = this.parseBlock();
    let else_: IfStmt | Block | null = null;
    if (this.match("else")) {
      else_ = this.check("if") ? this.parseIf() : this.parseBlock();
    }
    return { kind: "if", cond, then, else_, pos: start.pos };
  }

  // for の3形態: `for { }` / `for cond { }` / `for init; cond; post { }`
  private parseFor(): Stmt {
    const start = this.expect("for", "at start of for statement");

    if (this.check("{")) {
      return { kind: "for", init: null, cond: null, post: null, body: this.parseBlock(), pos: start.pos };
    }

    const first = this.withoutStructLit(() => this.parseSimpleStmt());

    if (this.check("{")) {
      if (first.kind !== "exprStmt") {
        throw new CompileError("for condition must be an expression", start.pos);
      }
      return { kind: "for", init: null, cond: first.expr, post: null, body: this.parseBlock(), pos: start.pos };
    }

    this.expect(";", "after for init statement");
    const cond = this.check(";") ? null : this.withoutStructLit(() => this.parseExpr());
    this.expect(";", "after for condition");
    const post = this.check("{") ? null : this.withoutStructLit(() => this.parseSimpleStmt());
    return { kind: "for", init: first, cond, post, body: this.parseBlock(), pos: start.pos };
  }

  // ---- 式(優先順位法 / Pratt parsing) ----

  // 文字列補間の中身のように「式1つで完結する」入力をパースする
  parseStandaloneExpr(): Expr {
    const expr = this.parseExpr();
    this.skipSemis();
    if (!this.check("eof")) {
      const t = this.peek();
      throw new CompileError(`unexpected '${t.value}' in string interpolation`, t.pos);
    }
    return expr;
  }

  private parseExpr(): Expr {
    return this.parseBinary(1);
  }

  private parseBinary(minPrec: number): Expr {
    let left = this.parseUnary();
    while (true) {
      const op = this.peek().type;
      const prec = PRECEDENCE[op];
      if (prec === undefined || prec < minPrec) return left;
      const opTok = this.next();
      // x is none — 右辺は式ではなく型
      if (op === "is") {
        const target = this.parseSingleType();
        left = { kind: "is", operand: left, target, pos: opTok.pos };
        continue;
      }
      const right = this.parseBinary(prec + 1); // 左結合
      // f() or fallback — none/error なら右辺の値
      if (op === "or") {
        left = { kind: "orElse", left, right, pos: opTok.pos };
        continue;
      }
      left = { kind: "binary", op, left, right, pos: opTok.pos };
    }
  }

  private parseUnary(): Expr {
    const t = this.peek();
    if (t.type === "!" || t.type === "-") {
      this.next();
      return { kind: "unary", op: t.type, operand: this.parseUnary(), pos: t.pos };
    }
    if (t.type === "<-") {
      this.next();
      return { kind: "recv", channel: this.parseUnary(), pos: t.pos };
    }
    return this.parsePostfix(this.parsePrimary());
  }

  // 呼び出し・添字・メンバアクセス・伝播は後置で連鎖する: f(x)[0].name / f()!
  private parsePostfix(expr: Expr): Expr {
    while (true) {
      // structリテラル: User{name: "alice", age: 30}(カンマまたは改行区切り)
      if (expr.kind === "ident" && this.allowStructLit && this.check("{")) {
        const name = expr.name;
        this.next();
        this.skipSemis();
        const fields: { name: string; value: Expr; pos: Pos }[] = [];
        const saved = this.allowStructLit;
        this.allowStructLit = true; // フィールド値の中では再び許可(ネストした literal 用)
        while (!this.check("}") && !this.check("eof")) {
          const fname = this.expect("ident", "as field name");
          this.expect(":", "after field name");
          const value = this.parseExpr();
          fields.push({ name: fname.value, value, pos: fname.pos });
          this.match(",");
          this.skipSemis();
        }
        this.allowStructLit = saved;
        this.expect("}", "at end of struct literal");
        expr = { kind: "structLit", name, fields, pos: expr.pos };
        continue;
      }
      if (this.match("!")) {
        expr = { kind: "prop", operand: expr, pos: expr.pos };
        continue;
      }
      if (this.match("(")) {
        const args: Expr[] = [];
        while (!this.check(")")) {
          args.push(this.parseExpr());
          if (!this.check(")")) this.expect(",", "between arguments");
        }
        this.expect(")", "after arguments");
        expr = { kind: "call", callee: expr, args, pos: expr.pos };
      } else if (this.match("[")) {
        const index = this.parseExpr();
        this.expect("]", "after index");
        expr = { kind: "index", target: expr, index, pos: expr.pos };
      } else if (this.match(".")) {
        const name = this.expect("ident", "after '.'").value;
        expr = { kind: "member", target: expr, name, pos: expr.pos };
      } else {
        return expr;
      }
    }
  }

  private parsePrimary(): Expr {
    const t = this.peek();
    switch (t.type) {
      case "int":
        this.next();
        return { kind: "int", value: t.value, pos: t.pos };
      case "float":
        this.next();
        return { kind: "float", value: t.value, pos: t.pos };
      case "string": {
        this.next();
        if (t.parts) {
          // 補間つき文字列: 式の断片を(元の位置情報つきで)再帰的にパースする
          const segments: InterpSegment[] = t.parts.map((p) =>
            p.kind === "text"
              ? { kind: "text", text: p.text }
              : { kind: "expr", expr: new Parser(lex(p.source, p.pos)).parseStandaloneExpr() },
          );
          return { kind: "interp", segments, pos: t.pos };
        }
        return { kind: "string", value: t.value, pos: t.pos };
      }
      case "true":
      case "false":
        this.next();
        return { kind: "bool", value: t.type === "true", pos: t.pos };
      case "none":
        this.next();
        return { kind: "none", pos: t.pos };
      case "ident":
        this.next();
        return { kind: "ident", name: t.value, pos: t.pos };
      case "(": {
        this.next();
        const expr = this.parseExpr();
        this.expect(")", "after expression");
        return expr;
      }
      case "[": {
        this.next();
        const elems: Expr[] = [];
        while (!this.check("]")) {
          elems.push(this.parseExpr());
          if (!this.check("]")) this.expect(",", "between array elements");
        }
        this.expect("]", "after array elements");
        return { kind: "arrayLit", elems, pos: t.pos };
      }
      case "fn": {
        // 無名関数: fn(x: int) int { ... }
        this.next();
        const params = this.parseParams();
        const ret = this.parseReturnType();
        const body = this.parseBlock();
        return { kind: "fnExpr", params, ret, body, pos: t.pos };
      }
      case "match": {
        // match式: match r { error => "failed"  int => "ok"  _ => "?" }
        this.next();
        const subject = this.withoutStructLit(() => this.parseExpr());
        this.expect("{", "after match subject");
        this.skipSemis();
        const arms: MatchArm[] = [];
        while (!this.check("}") && !this.check("eof")) {
          const armPos = this.peek().pos;
          const patterns: MatchPattern[] = [this.parseMatchPattern()];
          while (this.match(",")) patterns.push(this.parseMatchPattern());
          this.expect("=>", "after match pattern");
          const body = this.parseExpr();
          arms.push({ patterns, body, pos: armPos });
          this.skipSemis();
        }
        this.expect("}", "at end of match");
        return { kind: "match", subject, arms, pos: t.pos };
      }
      case "chan": {
        // チャネル生成: chan<int>()
        this.next();
        this.expect("<", "after 'chan'");
        const elem = this.parseType();
        this.expect(">", "after channel element type");
        this.expect("(", "to create a channel: chan<T>()");
        this.expect(")", "to create a channel: chan<T>()");
        return { kind: "chanExpr", elem, pos: t.pos };
      }
      default:
        throw new CompileError(`unexpected '${t.value === "" ? t.type : t.value}'`, t.pos);
    }
  }
}
