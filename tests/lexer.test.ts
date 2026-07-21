import { describe, expect, test } from "bun:test";
import { lex } from "../src/lexer";

const types = (src: string) => lex(src).tokens.map((t) => t.type);

describe("lexer", () => {
  test("基本的なトークン分解", () => {
    expect(types(`x := 10`)).toEqual(["ident", ":=", "int", ";", "eof"]);
  });

  test("キーワードと識別子を区別する", () => {
    expect(types(`fn foo`)).toEqual(["fn", "ident", ";", "eof"]);
  });

  test("セミコロン自動挿入: 式の行末にだけ入る", () => {
    const src = `fn main() {\n\tprint("hi")\n}\n`;
    expect(types(src)).toEqual([
      "fn", "ident", "(", ")", "{",
      "ident", "(", "string", ")", ";",
      "}", ";", "eof",
    ]);
  });

  test("セミコロン自動挿入: '{' の後には入らない", () => {
    expect(types("if x {\n}")).toEqual(["if", "ident", "{", "}", ";", "eof"]);
  });

  test("<- は隣接時のみアロー、離れていれば比較演算", () => {
    expect(types("ch <- v")).toEqual(["ident", "<-", "ident", ";", "eof"]);
    expect(types("a < -1")).toEqual(["ident", "<", "-", "int", ";", "eof"]);
  });

  test("float と int を区別する", () => {
    expect(types("1.5 2")).toEqual(["float", "int", ";", "eof"]);
  });

  test("文字列のエスケープ", () => {
    const { tokens } = lex(`"a\\nb"`);
    expect(tokens[0].value).toBe("a\nb");
  });

  test("コメントはトークン列には乗らない(セミコロン挿入だけ普通に効く)", () => {
    expect(types("x // comment\ny")).toEqual(["ident", ";", "ident", ";", "eof"]);
  });

  test("コメントは別配列(comments)へ位置つきで退避される(mesh fmt向け)", () => {
    const { comments } = lex(`x := 1 // trailing\n// leading\ny := 2`);
    expect(comments).toEqual([
      { text: "// trailing", pos: { line: 1, col: 8 } },
      { text: "// leading", pos: { line: 2, col: 1 } },
    ]);
  });

  test("文字列補間: text/expr の部品に分解される", () => {
    const [token] = lex(`"worker \${id} done"`).tokens;
    expect(token.parts).toEqual([
      { kind: "text", text: "worker " },
      { kind: "expr", source: "id", pos: { line: 1, col: 11 } },
      { kind: "text", text: " done" },
    ]);
  });

  test("文字列補間: 入れ子の文字列と波括弧を正しく数える", () => {
    const [token] = lex(`"x\${f({"k": 1})}y"`).tokens;
    expect(token.parts?.[1]).toMatchObject({ kind: "expr", source: `f({"k": 1})` });
  });

  test("補間なしの文字列は従来どおり", () => {
    const [token] = lex(`"plain"`).tokens;
    expect(token.parts).toBeUndefined();
    expect(token.value).toBe("plain");
  });

  test("\\$ で補間をエスケープできる", () => {
    const [token] = lex(`"price \\$100"`).tokens;
    expect(token.parts).toBeUndefined();
    expect(token.value).toBe("price $100");
  });

  test("空の補間 ${} はエラー", () => {
    expect(() => lex(`"a\${}b"`)).toThrow("empty interpolation");
  });

  test("閉じていない補間はエラー", () => {
    expect(() => lex(`"a\${x`)).toThrow("interpolation not terminated");
  });
});
