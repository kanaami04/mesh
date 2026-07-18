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

  test("wait ブロックをパースできる", () => {
    const [stmt] = parseBody(`wait {\nspawn f(1)\n}`);
    expect(stmt.kind).toBe("wait");
  });

  test("トップレベルは fn のみ", () => {
    expect(() => parse(`x := 1`)).toThrow(CompileError);
  });
});
