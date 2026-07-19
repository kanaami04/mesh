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

  test("後置の ! と or をパースできる", () => {
    const [stmt] = parseBody(`x := f()! or 0`);
    if (stmt.kind !== "shortVarDecl") throw new Error("unexpected");
    expect(stmt.values[0].kind).toBe("orElse");
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
});
