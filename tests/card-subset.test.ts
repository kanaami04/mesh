import { describe, expect, test } from "bun:test";
import { LANGUAGE_CARD } from "../src/card";
import { buildSubsetCard, detectFeatures } from "../src/card-subset";

describe("サブセットカード(F-13後半): detectFeatures", () => {
  test("何も特別な機能を使わないソースは何も検出しない", () => {
    const features = detectFeatures(`fn main() {\n\tx := 1\n\tprint(x + 2)\n}`);
    expect(features.size).toBe(0);
  });

  test("ジェネリクス: fn name<T>(...) を検出する", () => {
    const features = detectFeatures(`fn first<T>(arr: T[], pred: fn(T) bool) T | none { return none }`);
    expect(features.has("generics")).toBe(true);
  });

  test("判別可能union: type X = { ... } | { ... } を検出する", () => {
    const features = detectFeatures(`type Resp = { kind: "ok" } | { kind: "notFound" }`);
    expect(features.has("discriminatedUnions")).toBe(true);
  });

  test("構造化エラー: error type / error struct を検出する", () => {
    expect(detectFeatures(`error type X = { kind: "a" } | { kind: "b" }`).has("structuredErrors")).toBe(true);
    expect(detectFeatures(`error struct X { message: string }`).has("structuredErrors")).toBe(true);
  });

  test("struct宣言を検出する", () => {
    const features = detectFeatures(`struct User { name: string }`);
    expect(features.has("structs")).toBe(true);
  });

  test("配列型を検出する(型付き配列リテラル・型サフィックスどちらも)", () => {
    expect(detectFeatures(`xs := Todo[]{a, b}`).has("arrays")).toBe(true);
    expect(detectFeatures(`fn f(xs: int[]) {}`).has("arrays")).toBe(true);
  });

  test("並行処理: spawn/detach/chan/select/wait のいずれかを検出する", () => {
    expect(detectFeatures(`t := spawn f()`).has("concurrency")).toBe(true);
    expect(detectFeatures(`ch := chan<int>(none)`).has("concurrency")).toBe(true);
    expect(detectFeatures(`x := select { }`).has("concurrency")).toBe(true);
  });

  test("モジュール: import/export を検出する", () => {
    expect(detectFeatures(`import "util"`).has("modules")).toBe(true);
    expect(detectFeatures(`export fn f() {}`).has("modules")).toBe(true);
  });

  test("複数ファイル分を渡すと、どれか1つが使っていれば検出される(呼び出し側でjoinする想定)", () => {
    const features = detectFeatures(["fn main() { print(1) }", "struct User { name: string }"].join("\n"));
    expect(features.has("structs")).toBe(true);
  });
});

describe("サブセットカード(F-13後半): buildSubsetCard", () => {
  // 見出し行そのもの(先頭 "## ")で判定する — 見出し名を裸の語句で探すと、他セクションの
  // 地の文にある相互参照(例: Typesセクション中の「(see "Discriminated unions" below)」)を
  // 誤検出してしまう
  const ALWAYS_HEADINGS = [
    "## Program structure",
    "## Bindings (immutable by default)",
    "## Types",
    "## Absence & failure",
    "## Control flow",
    "## Operators",
    "## Strings",
    "## Builtins (complete list)",
    "## Does NOT exist in Mesh",
    "## Diagnostic codes",
    "## Common compile errors",
    "## Verify your code",
  ];
  const FEATURE_HEADINGS = [
    "## Generic functions",
    "## Discriminated unions",
    "## Structured errors",
    "## Structs, maps & methods",
    "## Arrays",
    "## Concurrency",
    "## Modules (import / export)",
  ];

  test("何も使わない小さなプログラムでは、常時セクションだけが残り機能セクションは全部落ちる", () => {
    const card = buildSubsetCard([`fn main() {\n\tx := 1\n\tprint(x + 2)\n}`]);
    for (const h of ALWAYS_HEADINGS) expect(card).toContain(h);
    for (const h of FEATURE_HEADINGS) expect(card).not.toContain(h);
  });

  test("使っている機能のセクションだけ追加で入る(ここでは構造体のみ)", () => {
    const card = buildSubsetCard([`struct User { name: string }\nfn main() { print(1) }`]);
    expect(card).toContain("Structs, maps & methods");
    expect(card).not.toContain("Concurrency (structured");
    expect(card).not.toContain("Generic functions");
  });

  test("全部載りカードの「COMPLETE reference」主張は、サブセットである旨の注記に置き換わる", () => {
    const card = buildSubsetCard([`fn main() { print(1) }`]);
    expect(card).not.toContain("COMPLETE reference");
    expect(card).toContain("PROJECT-SCOPED SUBSET");
    expect(card).toContain("mesh card");
  });

  test("何も使わない小さなプログラムでは、フルカードより明確に短くなる", () => {
    const subset = buildSubsetCard([`fn main() { print(1) }`]);
    expect(subset.length).toBeLessThan(LANGUAGE_CARD.length * 0.6);
  });

  test("全部の機能を使うソースを渡すと、フルカードと同じ内容になる", () => {
    const everything = [
      `fn first<T>(arr: T[], pred: fn(T) bool) T | none { return none }`,
      `type Resp = { kind: "ok" } | { kind: "notFound" }`,
      `error type X = { kind: "a" } | { kind: "b" }`,
      `struct User { name: string }`,
      `t := spawn f()`,
      `import "util"`,
    ];
    expect(buildSubsetCard(everything)).toBe(LANGUAGE_CARD);
  });
});
