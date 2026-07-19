// Parser: トークン列を AST に組み立てる。
// 手法は「再帰下降構文解析」— 文法規則ひとつが関数ひとつに対応する、
// Go 本家や TypeScript 本家のコンパイラでも使われている定番の書き方。

import type {
  Block,
  CallExpr,
  Expr,
  FnDecl,
  IfStmt,
  ImportDecl,
  InterpSegment,
  MatchArm,
  MatchPattern,
  Param,
  Program,
  Receiver,
  SelectArm,
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
    const imports: ImportDecl[] = [];
    const fns: FnDecl[] = [];
    const types: TypeDecl[] = [];
    this.skipSemis();
    // import はファイル先頭にまとめる(宣言が始まったら以後の import はエラー)
    while (this.check("import")) {
      imports.push(this.parseImportDecl());
      this.skipSemis();
    }
    while (!this.check("eof")) {
      if (this.check("import")) {
        throw new CompileError("imports must come before all declarations", this.peek().pos);
      }
      const exported = this.match("export");
      // error type X = ... / error struct X { ... }(F-2後半): "error" は予約語ではなく
      // ("error"は組み込み型名としてチェッカー側で守られている)、直後が type/struct のときだけ
      // マーカーとして読む文脈依存キーワード。1トークン先読みで曖昧さなく判定できる
      const isError =
        this.check("ident") && this.peek().value === "error" &&
        (this.peek(1).type === "type" || this.peek(1).type === "struct");
      if (isError) this.next();
      if (this.check("type")) {
        types.push(this.parseTypeDecl(exported, isError));
      } else if (this.check("struct")) {
        types.push(this.parseStructDecl(exported, isError));
      } else if (this.check("fn")) {
        fns.push(this.parseFnDecl(exported));
      } else {
        throw new CompileError(
          exported
            ? "'export' must be followed by a 'fn', 'struct' or 'type' declaration"
            : "only 'fn', 'struct' and 'type' declarations are allowed at the top level",
          this.peek().pos,
        );
      }
      this.skipSemis();
    }
    return { kind: "program", imports, types, fns };
  }

  // import "math" — パッケージ(プロジェクトルート直下のディレクトリ)の取り込み
  private parseImportDecl(): ImportDecl {
    const start = this.expect("import", "at start of import");
    const pathTok = this.expect("string", "as import path (like: import \"math\")");
    if (pathTok.parts) {
      throw new CompileError("import path cannot use string interpolation", pathTok.pos);
    }
    const path = pathTok.value;
    const alias = path.split("/").pop() ?? path;
    if (alias === "") {
      throw new CompileError("import path cannot be empty", pathTok.pos);
    }
    return { kind: "importDecl", path, alias, pos: start.pos };
  }

  // struct User { name: string  age: int } — 意味的には型への名付け(typeと同じ)なので
  // TypeDecl として登録する。フィールドは改行区切り
  private parseStructDecl(exported: boolean, isError: boolean): TypeDecl {
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
      exported,
      isError,
      pos: start.pos,
    };
  }

  // type Status = "active" | "banned"
  // type GetUserResponse = { kind: "ok", user: User } | { kind: "notFound" } — 判別可能union
  // (C-1)。無名の {...} 型式は union の中でだけ有効(B-5): 単独で書いたら struct を使えと誘導する
  private parseTypeDecl(exported: boolean, isError: boolean): TypeDecl {
    const start = this.expect("type", "at start of type declaration");
    const name = this.expect("ident", "as type name").value;
    this.expect("=", "after type name");
    const first = this.parseUnionMember();
    if (!this.check("|")) {
      if (first.kind === "structType") {
        throw new CompileError(
          `use 'struct ${name} { ... }' to define a data shape ('{...}' alone is only allowed inside a union)`,
          first.pos,
        );
      }
      return { kind: "typeDecl", name, node: first, exported, isError, pos: start.pos };
    }
    const members: TypeNode[] = [first];
    while (this.match("|")) members.push(this.parseUnionMember());
    return {
      kind: "typeDecl",
      name,
      node: { kind: "union", members, pos: first.pos },
      exported,
      isError,
      pos: start.pos,
    };
  }

  // union の1メンバー: 無名struct型 {...}(判別可能union用)か、通常の単一型
  private parseUnionMember(): TypeNode {
    if (this.check("{")) return this.parseInlineStructType();
    return this.parseSingleType();
  }

  // { kind: "ok", user: User } — 無名の構造体型リテラル。フィールドはカンマまたは改行区切り
  // (struct宣言は改行区切りのみだが、union内では1行に並べて書きたいのでカンマも許可する)
  private parseInlineStructType(): TypeNode {
    const start = this.expect("{", "at start of inline struct type");
    this.skipSemis();
    const fields: StructFieldNode[] = [];
    while (!this.check("}") && !this.check("eof")) {
      const fname = this.expect("ident", "as field name");
      this.expect(":", "after field name");
      const type = this.parseType();
      fields.push({ name: fname.value, type, pos: fname.pos });
      this.match(",");
      this.skipSemis();
    }
    this.expect("}", "at end of inline struct type");
    return { kind: "structType", fields, pos: start.pos };
  }

  private parseFnDecl(exported: boolean): FnDecl {
    const start = this.expect("fn", "at start of function declaration");
    // fn (u: User) describe() ... — 直後が '(' ならメソッドのレシーバ節(Goスタイル)。
    // 関数名は常に ident なので、'(' との1トークン先読みで曖昧さなく判定できる
    const receiver = this.check("(") ? this.parseReceiver() : null;
    if (exported && receiver) {
      // メソッドの可視性は struct に従う(structが見える場所ならメソッドも呼べる)ので
      // 個別の export は意味を持たない。書き方を1通りに保つため誘導エラーにする
      throw new CompileError(
        "methods are visible wherever their struct is — export the struct instead of the method",
        start.pos,
      );
    }
    const name = this.expect("ident", "as function name").value;
    // fn first<T>(...) — メソッド(receiver付き)には generics を許さない(v1はfn限定)
    const typeParams = receiver ? [] : this.parseTypeParams();
    const params = this.parseParams();
    const ret = this.parseReturnType();
    const body = this.parseBlock();
    return { kind: "fnDecl", name, receiver, typeParams, params, ret, body, exported, pos: start.pos };
  }

  // fn first<T>(...) / fn zip<A, B>(...) の <T, ...> 部分。無ければ空配列
  private parseTypeParams(): string[] {
    if (!this.check("<")) return [];
    this.next();
    const names: string[] = [];
    while (!this.check(">")) {
      names.push(this.expect("ident", "as type parameter name").value);
      if (!this.check(">")) this.expect(",", "between type parameters");
    }
    this.expect(">", "after type parameters");
    return names;
  }

  private parseReceiver(): Receiver {
    this.expect("(", "at start of method receiver");
    const nameTok = this.expect("ident", "as receiver name");
    this.expect(":", "after receiver name");
    const type = this.parseType();
    this.expect(")", "after receiver type");
    return { name: nameTok.value, type, pos: nameTok.pos };
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

  // `for` の直後が「ident (, ident)? := range」の形かを先読みで判定する
  private isRangeHeader(): boolean {
    if (this.peek(0).type !== "ident") return false;
    if (this.peek(1).type === ":=" && this.peek(2).type === "range") return true;
    return (
      this.peek(1).type === "," &&
      this.peek(2).type === "ident" &&
      this.peek(3).type === ":=" &&
      this.peek(4).type === "range"
    );
  }

  private parseMatchPattern(): MatchPattern {
    const t = this.peek();
    if (t.type === "ident" && t.value === "_") {
      this.next();
      return { kind: "wildcard", pos: t.pos };
    }
    // 判別可能union用の部分構造パターン: { kind: "ok" }。書いたフィールドが一致する
    // union メンバーへ絞り込む(絞り込んだ後は subject.field で普通にアクセスする)
    if (t.type === "{") {
      return { kind: "type", type: this.parseInlineStructType() };
    }
    return { kind: "type", type: this.parseSingleType() };
  }

  private parseSingleType(): TypeNode {
    const atom = this.parseTypeAtom();
    return this.parseArraySuffix(atom);
  }

  // このトークンから型が始まりうるか(fn型の「戻り値があるか」の判定に使う)
  private canStartType(): boolean {
    const t = this.peek().type;
    return (
      t === "ident" || t === "string" || t === "chan" || t === "map" ||
      t === "none" || t === "fn" || t === "("
    );
  }

  // 配列サフィックスを除いた単体の型(chan<T> / map<K,V> / fn(..) / name / リテラル / none / 括弧)
  private parseTypeAtom(): TypeNode {
    // (T) — グループ化。fn型をunionに入れる時などの曖昧さ解消用: (fn(int) int) | none
    if (this.check("(")) {
      this.next();
      const inner = this.parseType();
      this.expect(")", "after type");
      return inner;
    }
    // fn(int, string) bool — 関数型。関数宣言と同じ読みで、戻り値のunionは戻り値側に束縛される
    // (fn(int) int | error の戻り値は int | error。関数自体をunionに入れるなら括弧で包む)
    if (this.check("fn")) {
      const start = this.next();
      this.expect("(", "after 'fn' in a function type");
      const params: TypeNode[] = [];
      while (!this.check(")")) {
        // パラメータ名は書かない(型のみ)。書いたら書き方1通りへ誘導する
        if (this.check("ident") && this.peek(1).type === ":") {
          throw new CompileError(
            "parameter names are not used in function types — write the types only, like fn(int, string) bool",
            this.peek().pos,
          );
        }
        params.push(this.parseType());
        if (!this.check(")")) this.expect(",", "between parameter types");
      }
      this.expect(")", "after parameter types");
      const ret = this.canStartType() ? this.parseType() : null;
      return { kind: "fnType", params, ret, pos: start.pos };
    }
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
    if (this.check("map")) {
      const start = this.next();
      this.expect("<", "after 'map'");
      const key = this.parseType();
      this.expect(",", "between map key and value types");
      const value = this.parseType();
      this.expect(">", "after map value type");
      return { kind: "mapType", key, value, pos: start.pos };
    }
    if (this.check("none")) {
      const t = this.next();
      return { kind: "name", name: "none", pos: t.pos };
    }
    const nameTok = this.expect("ident", "as type name");
    // math.User — パッケージ修飾された型名(import したパッケージの exported 型)
    if (this.check(".") && this.peek(1).type === "ident") {
      this.next();
      const typeName = this.next();
      return { kind: "name", name: typeName.value, pkg: nameTok.value, pos: nameTok.pos };
    }
    return { kind: "name", name: nameTok.value, pos: nameTok.pos };
  }

  // T[] / T[][] のような配列サフィックス。要素型が chan<T>/map<K,V> でも同じく効く
  // (chan<int>[] / map<string, int>[] のような「総称型の配列」を書けるようにするため)
  private parseArraySuffix(base: TypeNode): TypeNode {
    let type = base;
    while (this.check("[") && this.peek(1).type === "]") {
      this.next();
      this.next();
      type = { kind: "array", elem: type, pos: base.pos };
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
      case "wait": {
        this.next();
        const body = this.parseBlock();
        return { kind: "wait", body, pos: t.pos };
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

    // mut x := ...(可変宣言)。mut は := / 型注釈宣言の前にしか置けない
    const mutable = this.match("mut");

    // 型注釈つき宣言: x: T = v  /  mut best: string | none = none
    if (this.check("ident") && this.peek(1).type === ":") {
      const nameTok = this.next();
      this.next(); // :
      const typeNode = this.parseType();
      this.expect("=", "in typed declaration ('name: T = value')");
      const value = this.parseExpr();
      return { kind: "typedVarDecl", name: nameTok.value, typeNode, value, mutable, pos: start.pos };
    }

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

    // range形: for i, v := range arr / for k, v := range m / for i := range 10
    if (this.isRangeHeader()) {
      const names: string[] = [this.expect("ident", "as range variable").value];
      if (this.match(",")) {
        names.push(this.expect("ident", "as range variable").value);
      }
      this.expect(":=", "in range header");
      this.expect("range", "in range header");
      const subject = this.withoutStructLit(() => this.parseExpr());
      const body = this.parseBlock();
      return { kind: "rangeFor", names, subject, body, pos: start.pos };
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
        // is の右辺は match のパターンと同じ: 型名・文字列リテラル・部分構造 { kind: "ok" }
        const target = this.check("{") ? this.parseInlineStructType() : this.parseSingleType();
        left = { kind: "is", operand: left, target, pos: opTok.pos };
        continue;
      }
      // f() or fallback(noneのみ) / f() or e => fallback(失敗値を束縛。errorを含むなら必須)
      if (op === "or") {
        let binding: string | undefined;
        if (this.check("ident") && this.peek(1).type === "=>") {
          binding = this.next().value;
          this.next(); // =>
        }
        const right = this.parseBinary(prec + 1);
        left = { kind: "orElse", left, right, binding, pos: opTok.pos };
        continue;
      }
      const right = this.parseBinary(prec + 1); // 左結合
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
    // spawn f(x) / detach f(x) — 並行起動して受取口を返す式。
    // spawn は今の関数が所有(関数を抜けるとき暗黙wait)、detach はプログラムが所有
    if (t.type === "spawn" || t.type === "detach") {
      this.next();
      const call = this.parseUnary();
      if (call.kind !== "call") {
        throw new CompileError(`'${t.type}' must be followed by a function call`, t.pos);
      }
      return { kind: "spawn", call, detached: t.type === "detach", pos: t.pos };
    }
    return this.parsePostfix(this.parsePrimary());
  }

  // structリテラルの中身 `{ field: value, ... }` を読む(名前の直後の `{` から)。
  // 素の User{...} と修飾つき math.Point{...} の両方から使う共通部分
  private parseStructLitBody(name: string, pos: Pos): Extract<Expr, { kind: "structLit" }> {
    this.next(); // {
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
    return { kind: "structLit", name, fields, pos };
  }

  // 呼び出し・添字・メンバアクセス・伝播は後置で連鎖する: f(x)[0].name / f()!
  private parsePostfix(expr: Expr): Expr {
    while (true) {
      // 型付き配列リテラル: Todo[]{}(空) / int[]{1, 2} / int[][]{...}(多次元)
      if (
        expr.kind === "ident" &&
        this.allowStructLit &&
        this.check("[") &&
        this.peek(1).type === "]"
      ) {
        // [] の連なりを数え、その後が { なら型付き配列リテラルと確定する
        let dims = 0;
        while (this.peek(2 * dims).type === "[" && this.peek(2 * dims + 1).type === "]") dims++;
        if (this.peek(2 * dims).type === "{") {
          // 要素型: T を (dims-1) 回 array で包んだもの(int[]{} なら要素は int)
          let elemType: TypeNode = { kind: "name", name: expr.name, pos: expr.pos };
          for (let k = 0; k < dims - 1; k++) elemType = { kind: "array", elem: elemType, pos: expr.pos };
          for (let k = 0; k < dims; k++) {
            this.next();
            this.next();
          }
          this.next(); // {
          this.skipSemis();
          const elems: Expr[] = [];
          while (!this.check("}") && !this.check("eof")) {
            elems.push(this.parseExpr());
            this.match(",");
            this.skipSemis();
          }
          this.expect("}", "at end of array literal");
          expr = { kind: "arrayLit", elems, elemType, pos: expr.pos };
          continue;
        }
      }
      // 修飾structリテラル: math.Point{x: 1, y: 2}(import したパッケージの exported struct)
      if (
        expr.kind === "member" &&
        expr.target.kind === "ident" &&
        this.allowStructLit &&
        this.check("{")
      ) {
        const lit = this.parseStructLitBody(expr.name, expr.pos);
        lit.pkg = expr.target.name;
        expr = lit;
        continue;
      }
      // structリテラル: User{name: "alice", age: 30}(カンマまたは改行区切り)
      if (expr.kind === "ident" && this.allowStructLit && this.check("{")) {
        expr = this.parseStructLitBody(expr.name, expr.pos);
        continue;
      }
      if (this.match("?")) {
        // f()? — 伝播。直後が文字列リテラルなら文脈つき: f() ? "line ${i}: bad"
        // (文脈は文字列リテラル/補間のみ。任意の式を許すと `f()? - 1` 等が曖昧になる)
        let context: Expr | undefined;
        if (this.check("string")) {
          context = this.parsePrimary();
        }
        expr = { kind: "prop", operand: expr, context, pos: expr.pos };
        continue;
      }
      if (this.check("!")) {
        // 旧記法(2026-07-19に ? へ改名)。負の転移対策の誘導エラー
        throw new CompileError(
          "postfix '!' was renamed — use '?' to propagate none/error to the caller",
          this.peek().pos,
        );
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
        this.skipSemis(); // 複数行の配列リテラルで、要素末尾のASI挿入セミコロンを読み飛ばす
        const elems: Expr[] = [];
        while (!this.check("]") && !this.check("eof")) {
          elems.push(this.parseExpr());
          this.match(",");
          this.skipSemis();
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
      case "select": {
        // select { v := <-ch1 => ...  v := <-ch2 => ...  _ => ... }
        // "_" は非ブロッキング用の default アーム(あれば最大1つ)。matchと見た目は揃えるが、
        // パターンが「型」ではなく「どのchannel操作が先に終わったか」なので独立構文にしてある
        this.next();
        this.expect("{", "after 'select'");
        this.skipSemis();
        const arms: SelectArm[] = [];
        let defaultArm: Expr | null = null;
        while (!this.check("}") && !this.check("eof")) {
          const armPos = this.peek().pos;
          if (this.check("ident") && this.peek().value === "_") {
            this.next();
            if (defaultArm !== null) {
              throw new CompileError("select can only have one default ('_') arm", armPos);
            }
            this.expect("=>", "after '_' in select");
            defaultArm = this.parseExpr();
          } else {
            const nameTok = this.expect("ident", "as select binding name");
            this.expect(":=", "in select arm ('name := <-ch => body')");
            this.expect("<-", "select arms receive from a channel ('name := <-ch => body')");
            const channel = this.parseExpr();
            this.expect("=>", "after select arm channel");
            const body = this.parseExpr();
            arms.push({ name: nameTok.value, channel, body, pos: armPos });
          }
          this.skipSemis();
        }
        this.expect("}", "at end of select");
        return { kind: "select", arms, defaultArm, pos: t.pos };
      }
      case "chan": {
        // チャネル生成: chan<int>()(無制限バッファ) / chan<int>(n)(容量n、送信がブロックしうる)
        this.next();
        this.expect("<", "after 'chan'");
        const elem = this.parseType();
        this.expect(">", "after channel element type");
        this.expect("(", "to create a channel: chan<T>() or chan<T>(capacity)");
        const capacity = this.check(")") ? null : this.parseExpr();
        this.expect(")", "to create a channel: chan<T>() or chan<T>(capacity)");
        return { kind: "chanExpr", elem, capacity, pos: t.pos };
      }
      case "map": {
        // mapリテラル: map<string, int>{"a": 1, "b": 2}(空は {} )
        this.next();
        this.expect("<", "after 'map'");
        const key = this.parseType();
        this.expect(",", "between map key and value types");
        const value = this.parseType();
        this.expect(">", "after map value type");
        this.expect("{", "to create a map: map<K, V>{ ... }");
        this.skipSemis();
        const entries: { key: Expr; value: Expr; pos: Pos }[] = [];
        while (!this.check("}") && !this.check("eof")) {
          const entryPos = this.peek().pos;
          const k = this.parseExpr();
          this.expect(":", "after map key");
          const v = this.parseExpr();
          entries.push({ key: k, value: v, pos: entryPos });
          this.match(",");
          this.skipSemis();
        }
        this.expect("}", "at end of map literal");
        return { kind: "mapLit", key, value, entries, pos: t.pos };
      }
      default:
        throw new CompileError(`unexpected '${t.value === "" ? t.type : t.value}'`, t.pos);
    }
  }
}
