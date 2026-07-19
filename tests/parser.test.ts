import { describe, expect, test } from "bun:test";
import { parse } from "../src/parser";
import { CompileError } from "../src/token";

const parseBody = (body: string) => parse(`fn main() {\n${body}\n}`).fns[0].body.stmts;

describe("parser", () => {
  test("関数宣言: 名前・引数・union戻り値", () => {
    const program = parse(`fn divide(a: int, b: int) int | error { return a }`);
    const fn = program.fns[0];
    expect(fn.name).toBe("divide");
    expect(fn.params.map((p) => p.name)).toEqual(["a", "b"]);
    expect(fn.ret).toMatchObject({ kind: "union" });
  });

  test("union型: 3メンバー(User | none | error 相当)", () => {
    const program = parse(`fn f() int | none | error { return none }`);
    const ret = program.fns[0].ret;
    if (ret?.kind !== "union") throw new Error("unexpected");
    expect(ret.members.length).toBe(3);
  });

  test("配列型: chan<T>[] / map<K,V>[] のような総称型を要素にする配列型も書ける", () => {
    const chanArr = parse(`fn f(x: chan<int>[]) {}`).fns[0].params[0].type;
    expect(chanArr).toMatchObject({ kind: "array", elem: { kind: "chan", elem: { kind: "name", name: "int" } } });

    const mapArr = parse(`fn f(x: map<string, int>[]) {}`).fns[0].params[0].type;
    expect(mapArr).toMatchObject({
      kind: "array",
      elem: { kind: "mapType", key: { kind: "name", name: "string" }, value: { kind: "name", name: "int" } },
    });

    const chanArr2d = parse(`fn f(x: chan<int>[][]) {}`).fns[0].params[0].type;
    expect(chanArr2d).toMatchObject({ kind: "array", elem: { kind: "array", elem: { kind: "chan" } } });
  });

  test("配列リテラル: 複数行でも末尾カンマ無しでパースできる(struct/mapリテラルと同じくASIを読み飛ばす)", () => {
    const [stmt] = parseBody(`xs := [\n1,\n2,\n3\n]`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const arr = stmt.values[0];
    if (arr.kind !== "arrayLit") throw new Error("unexpected");
    expect(arr.elems.length).toBe(3);

    // struct リテラルを要素にした複数行配列(末尾カンマ無し)も同様に通る
    const [stmt2] = parseBody(`ys := [\nUser{name: "a"},\nUser{name: "b"}\n]`);
    if (stmt2.kind !== "shortVarDecl") throw new Error("unexpected");
    const arr2 = stmt2.values[0];
    if (arr2.kind !== "arrayLit") throw new Error("unexpected");
    expect(arr2.elems.length).toBe(2);
  });

  test("関数型注釈: fn(int, string) bool をパースできる(void戻り・greedy union戻り・括弧グループ化)", () => {
    const p1 = parse(`fn f(g: fn(int, string) bool) {}`).fns[0].params[0].type;
    expect(p1).toMatchObject({
      kind: "fnType",
      params: [{ kind: "name", name: "int" }, { kind: "name", name: "string" }],
      ret: { kind: "name", name: "bool" },
    });

    // 戻り値なし
    const p2 = parse(`fn f(g: fn(int)) {}`).fns[0].params[0].type;
    expect(p2).toMatchObject({ kind: "fnType", ret: null });

    // 宣言と同じ読み: fn(int) int | error の union は戻り値側に束縛
    const p3 = parse(`fn f(g: fn(int) int | error) {}`).fns[0].params[0].type;
    expect(p3).toMatchObject({ kind: "fnType", ret: { kind: "union" } });

    // 関数自体をunionに入れるなら括弧: (fn(int) int) | none
    const p4 = parse(`fn f(g: (fn(int) int) | none) {}`).fns[0].params[0].type;
    if (p4.kind !== "union") throw new Error("unexpected");
    expect(p4.members[0].kind).toBe("fnType");
    expect(p4.members[1]).toMatchObject({ kind: "name", name: "none" });
  });

  test("関数型注釈: パラメータ名を書くと型のみへ誘導エラー", () => {
    expect(() => parse(`fn f(g: fn(x: int) int) {}`)).toThrow(
      "parameter names are not used in function types",
    );
  });

  test("ジェネリクス(F-1後半): fn first<T>(...) の型パラメータをパースできる", () => {
    const fn1 = parse(`fn first<T>(arr: T[], pred: fn(T) bool) T | none { return none }`).fns[0];
    expect(fn1.typeParams).toEqual(["T"]);
    expect(fn1.params[0].type).toMatchObject({ kind: "array", elem: { kind: "name", name: "T" } });

    // 複数の型パラメータ
    const fn2 = parse(`fn mapKeys<K, V>(m: map<K, V>) K[] { return [] }`).fns[0];
    expect(fn2.typeParams).toEqual(["K", "V"]);

    // <> 無しは空配列(通常の関数と同じ)
    const fn3 = parse(`fn plain(x: int) int { return x }`).fns[0];
    expect(fn3.typeParams).toEqual([]);
  });

  test("union宣言: 行頭 | の複数行フォーマット(TSの定番)で書ける", () => {
    // ベンチ第1ラウンドでMeshが唯一落ちた負転移パターン(bench/tasks/04参照)
    const t1 = parse(`type Expr = { kind: "num", value: int }
    | { kind: "add", left: Expr, right: Expr }
    | { kind: "neg", operand: Expr }`).types[0];
    expect(t1.node).toMatchObject({ kind: "union" });
    if (t1.node.kind !== "union") throw new Error("unexpected");
    expect(t1.node.members.length).toBe(3);

    // 行末 | スタイル(| はASI対象外なので従来から可)も引き続き動く
    const t2 = parse(`type Status = "active" |
    "banned"`).types[0];
    if (t2.node.kind !== "union") throw new Error("unexpected");
    expect(t2.node.members.length).toBe(2);

    // error type マーカーとの組み合わせ
    const t3 = parse(`error type DbError = { kind: "notFound", table: string }
    | { kind: "timeout", ms: int }`).types[0];
    expect(t3.isError).toBe(true);
    if (t3.node.kind !== "union") throw new Error("unexpected");
    expect(t3.node.members.length).toBe(2);

    // 継続と誤読しないこと: type宣言の直後に別の宣言が続く通常ケース
    const prog = parse(`type Status = "active" | "banned"
fn main() {}`);
    expect(prog.types.length).toBe(1);
    expect(prog.fns.length).toBe(1);
  });

  test("構造化エラー(F-2後半): error type X = ... / error struct X { ... } の isError フラグ", () => {
    const t1 = parse(`error type DbError = { kind: "notFound" } | { kind: "timeout" }`).types[0];
    expect(t1.isError).toBe(true);
    expect(t1.name).toBe("DbError");

    const t2 = parse(`error struct DbError { table: string }`).types[0];
    expect(t2.isError).toBe(true);
    expect(t2.node).toMatchObject({ kind: "structType" });

    // "error" マーカー無しは今まで通り false
    const t3 = parse(`type Status = "active" | "banned"`).types[0];
    expect(t3.isError).toBe(false);
    const t4 = parse(`struct User { name: string }`).types[0];
    expect(t4.isError).toBe(false);

    // export と組み合わせても読める(export が先)
    const t5 = parse(`export error type DbError = { kind: "notFound" } | { kind: "timeout" }`).types[0];
    expect(t5.isError).toBe(true);
    expect(t5.exported).toBe(true);
  });

  test("判別可能union: type宣言のunion内に無名{...}型式を書ける", () => {
    const program = parse(
      `type GetUserResponse = { kind: "ok", user: User } | { kind: "notFound" }\nfn main() {}`,
    );
    const decl = program.types.find((t) => t.name === "GetUserResponse");
    if (decl?.node.kind !== "union") throw new Error("unexpected");
    expect(decl.node.members.length).toBe(2);
    expect(decl.node.members[0].kind).toBe("structType");
    if (decl.node.members[0].kind !== "structType") throw new Error("unexpected");
    expect(decl.node.members[0].fields.map((f) => f.name)).toEqual(["kind", "user"]);
  });

  test("判別可能union: 単独の裸{...}はB-5どおりエラー(structを使えと誘導)", () => {
    expect(() => parse(`type Resp = { kind: "ok" }\nfn main() {}`)).toThrow(
      "use 'struct Resp { ... }'",
    );
  });

  test("判別可能union: matchパターンに部分構造{...}を書ける", () => {
    const [stmt] = parseBody(`x := match res {\n{ kind: "ok" } => 1\n_ => 0\n}`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const m = stmt.values[0];
    if (m.kind !== "match") throw new Error("unexpected");
    const pattern = m.arms[0].patterns[0];
    expect(pattern.kind).toBe("type");
    if (pattern.kind !== "type") throw new Error("unexpected");
    expect(pattern.type.kind).toBe("structType");
  });

  test("多値戻りはエラーになる(union路線で廃止)", () => {
    expect(() => parse(`fn f() (int, error) { return 1 }`)).toThrow("multiple return values");
    expect(() => parse(`fn main() { return 1, 2 }`)).toThrow("multiple return values");
  });

  test("後置の ? と or をパースできる(文脈つき・束縛形も)", () => {
    const [stmt] = parseBody(`x := f()? or _ => 0`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const or = stmt.values[0];
    if (or.kind !== "orElse") throw new Error("unexpected");
    expect(or.binding).toBe("_");
    expect(or.left.kind).toBe("prop");

    // 文脈つき伝播: f() ? "ctx"
    const [stmt2] = parseBody(`x := f() ? "line \${i}: bad"`);
    if (stmt2.kind !== "shortVarDecl") throw new Error("unexpected");
    const prop = stmt2.values[0];
    if (prop.kind !== "prop") throw new Error("unexpected");
    expect(prop.context).toBeDefined();

    // 束縛形: or e => 式
    const [stmt3] = parseBody(`x := f() or e => g(e)`);
    if (stmt3.kind !== "shortVarDecl") throw new Error("unexpected");
    const or3 = stmt3.values[0];
    if (or3.kind !== "orElse") throw new Error("unexpected");
    expect(or3.binding).toBe("e");
  });

  test("match式: アーム・複数パターン・ワイルドカード", () => {
    const [stmt] = parseBody(`x := match r {
	none, error => "fail"
	int => "ok"
	_ => "other"
}`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const m = stmt.values[0];
    if (m.kind !== "match") throw new Error("expected match, got " + m.kind);
    expect(m.arms.length).toBe(3);
    expect(m.arms[0].patterns.length).toBe(2);
    expect(m.arms[2].patterns[0].kind).toBe("wildcard");
  });

  test("is は型を右辺に取る", () => {
    const [stmt] = parseBody(`ok := x is none`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const expr = stmt.values[0];
    expect(expr.kind).toBe("is");
  });

  test("短縮変数宣言 :=", () => {
    const [stmt] = parseBody(`x := 1 + 2`);
    expect(stmt.kind).toBe("shortVarDecl");
    if (stmt.kind === "shortVarDecl") {
      expect(stmt.names).toEqual(["x"]);
      expect(stmt.values[0].kind).toBe("binary");
    }
  });

  test("多値の受け取り v, err := f()", () => {
    const [stmt] = parseBody(`v, err := f()`);
    expect(stmt.kind).toBe("shortVarDecl");
    if (stmt.kind === "shortVarDecl") expect(stmt.names).toEqual(["v", "err"]);
  });

  test("演算子の優先順位: 1 + 2 * 3 は 1 + (2 * 3)", () => {
    const [stmt] = parseBody(`x := 1 + 2 * 3`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const expr = stmt.values[0];
    if (expr.kind !== "binary") throw new Error("unexpected");
    expect(expr.op).toBe("+");
    expect(expr.right.kind).toBe("binary");
  });

  test("for の3形態", () => {
    const [three] = parseBody(`for i := 0; i < 10; i++ {\n}`);
    if (three.kind !== "for") throw new Error("unexpected");
    expect(three.init?.kind).toBe("shortVarDecl");
    expect(three.post?.kind).toBe("incDec");

    const [condOnly] = parseBody(`for x < 10 {\n}`);
    if (condOnly.kind !== "for") throw new Error("unexpected");
    expect(condOnly.init).toBeNull();
    expect(condOnly.cond?.kind).toBe("binary");

    const [infinite] = parseBody(`for {\nbreak\n}`);
    if (infinite.kind !== "for") throw new Error("unexpected");
    expect(infinite.cond).toBeNull();
  });

  test("spawn はチャネル送受信・呼び出しと組み合わせられる", () => {
    const stmts = parseBody(`ch := chan<int>()\nspawn f(1, ch)\nx := <-ch\nch <- 2`);
    expect(stmts.map((s) => s.kind)).toEqual(["shortVarDecl", "exprStmt", "shortVarDecl", "send"]);
  });

  test("spawn は式として受取口を返せる(task := spawn f())", () => {
    const [stmt] = parseBody(`task := spawn f(1)`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    expect(stmt.values[0].kind).toBe("spawn");
  });

  test("spawn の後は関数呼び出しのみ", () => {
    expect(() => parseBody(`spawn 1 + 2`)).toThrow(CompileError);
  });

  test("detach は spawn と同形で detached フラグが立つ", () => {
    const [stmt] = parseBody(`task := detach f(1)`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const expr = stmt.values[0];
    if (expr.kind !== "spawn") throw new Error("expected spawn node, got " + expr.kind);
    expect(expr.detached).toBe(true);
    expect(() => parseBody(`detach 1 + 2`)).toThrow(CompileError);
  });

  test("wait ブロックをパースできる", () => {
    const [stmt] = parseBody(`wait {\nspawn f(1)\n}`);
    expect(stmt.kind).toBe("wait");
  });

  test("chan<T>(n) は容量式を持つ", () => {
    const [stmt] = parseBody(`ch := chan<int>(3)`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const expr = stmt.values[0];
    if (expr.kind !== "chanExpr") throw new Error("expected chanExpr, got " + expr.kind);
    expect(expr.capacity?.kind).toBe("int");
  });

  test("chan<T>() は容量なし(null)", () => {
    const [stmt] = parseBody(`ch := chan<int>()`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const expr = stmt.values[0];
    if (expr.kind !== "chanExpr") throw new Error("expected chanExpr, got " + expr.kind);
    expect(expr.capacity).toBeNull();
  });

  test("select式: アームとdefault(_)をパースできる", () => {
    const [stmt] = parseBody(`msg := select {\nv := <-a => v\n_ => "none"\n}`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const expr = stmt.values[0];
    if (expr.kind !== "select") throw new Error("expected select, got " + expr.kind);
    expect(expr.arms.length).toBe(1);
    expect(expr.arms[0].name).toBe("v");
    expect(expr.defaultArm).not.toBeNull();
  });

  test("select式: defaultは1つまで", () => {
    expect(() => parseBody(`msg := select {\n_ => "a"\n_ => "b"\n}`)).toThrow(CompileError);
  });

  test("トップレベルは fn のみ", () => {
    expect(() => parse(`x := 1`)).toThrow(CompileError);
  });

  test("モジュール: import宣言とexport修飾をパースできる", () => {
    const program = parse(`import "mathutil"

export fn add(a: int, b: int) int { return a + b }
fn helper() int { return 1 }
export struct Point { x: int }
export type Status = "on" | "off"
fn main() {}`);
    expect(program.imports).toEqual([
      expect.objectContaining({ kind: "importDecl", path: "mathutil", alias: "mathutil" }),
    ]);
    expect(program.fns.find((f) => f.name === "add")?.exported).toBe(true);
    expect(program.fns.find((f) => f.name === "helper")?.exported).toBe(false);
    expect(program.types.find((t) => t.name === "Point")?.exported).toBe(true);
    expect(program.types.find((t) => t.name === "Status")?.exported).toBe(true);
  });

  test("モジュール: importは宣言より前に置く必要がある", () => {
    expect(() => parse(`fn main() {}\nimport "x"`)).toThrow("imports must come before");
  });

  test("モジュール: メソッドへのexportはstructをexportしろと誘導", () => {
    expect(() => parse(`struct T { x: int }\nexport fn (t: T) m() int { return t.x }\nfn main() {}`)).toThrow(
      "export the struct instead",
    );
  });

  test("モジュール: 修飾型名(math.User)と修飾structリテラル(math.Point{...})をパースできる", () => {
    const param = parse(`fn f(u: math.User) {}`).fns[0].params[0].type;
    expect(param).toMatchObject({ kind: "name", name: "User", pkg: "math" });

    const [stmt] = parseBody(`p := math.Point{x: 1, y: 2}`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    const lit = stmt.values[0];
    expect(lit).toMatchObject({ kind: "structLit", name: "Point", pkg: "math" });
  });
});
