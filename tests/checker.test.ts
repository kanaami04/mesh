import { describe, expect, test } from "bun:test";
import { check, checkModules } from "../src/checker";
import { compile, compileModules } from "../src/compiler";
import { DIAGNOSTIC_EXPLANATIONS, type DiagnosticCode } from "../src/diagnostic-codes";
import { parse } from "../src/parser";
import { CompileError } from "../src/token";

// H-2: 'json struct'はcompiler.ts(parse→synthesizeJsonDecoders→check)側でのみ
// decode<Name>を合成するので、checker.tsを直接叩くerrorsOf/check(parse(...))では
// 合成が起きない。このテスト群だけはcompile()/compileModules()を使う
const jsonErrorsOf = (src: string) => compile(src).diagnostics.map((d) => d.message);

const diagnosticsOf = (src: string) => check(parse(src));
const errorsOf = (src: string) => check(parse(src)).map((d) => d.message);
const inMain = (body: string) => errorsOf(`fn main() {\n${body}\n}`);
// F-15: テスト関数の発見・シグネチャ検証は "_test.mesh" ファイル限定なので、
// 単一ファイルの check() ではなく checkModules() にファイル名を明示して渡す
const errorsOfTestFile = (src: string) =>
  checkModules([{ pkg: "main", file: "main_test.mesh", program: parse(src) }], { testMode: true }).diagnostics.map(
    (d) => d.message,
  );
const testsOf = (src: string) =>
  checkModules([{ pkg: "main", file: "main_test.mesh", program: parse(src) }], { testMode: true }).tests;

describe("checker", () => {
  test("正しいプログラムはエラーなし", () => {
    expect(inMain(`x := 1\nprint(x + 2)`)).toEqual([]);
  });

  test("未定義の変数を検出", () => {
    expect(inMain(`print(nothing)`)).toEqual([expect.stringContaining("undefined: 'nothing'")]);
  });

  test("型の不一致を検出: string + int", () => {
    expect(inMain(`x := "a" + 1`)).toEqual([expect.stringContaining("invalid operation")]);
  });

  test("str() を使えば文字列連結できる", () => {
    expect(inMain(`x := "a" + str(1)`)).toEqual([]);
  });

  test("引数の数の不一致を検出", () => {
    const errors = errorsOf(`fn f(a: int) int { return a }\nfn main() { x := f(1, 2)\nprint(x) }`);
    expect(errors).toEqual([expect.stringContaining("expected 1 argument(s), got 2")]);
  });

  test("引数の型の不一致を検出", () => {
    const errors = errorsOf(`fn f(a: int) int { return a }\nfn main() { x := f("hi")\nprint(x) }`);
    expect(errors).toEqual([expect.stringContaining(`cannot use "hi" as int`)]);
  });

  test("同一スコープでの再宣言を検出", () => {
    expect(inMain(`x := 1\nx := 2`)).toEqual([expect.stringContaining("already declared")]);
  });

  test("予約語 'eval' は変数名に使えない(strict modeでJS構文エラーになるため)", () => {
    expect(inMain(`eval := 1`)).toEqual([
      expect.stringContaining("'eval' is a reserved word and cannot be used as a name"),
    ]);
  });

  test("予約語 'arguments' は変数名に使えない(strict modeでJS構文エラーになるため)", () => {
    expect(inMain(`arguments := 1`)).toEqual([
      expect.stringContaining("'arguments' is a reserved word and cannot be used as a name"),
    ]);
  });

  test("予約語 'eval' は関数名に使えない", () => {
    const errors = errorsOf(`fn eval(e: int) int { return e }\nfn main() {}`);
    expect(errors).toEqual([
      expect.stringContaining("'eval' is a reserved word and cannot be used as a name"),
    ]);
  });

  test("シャドーイングを検出: 内側スコープで外側の変数を隠す", () => {
    expect(inMain(`x := 1\nif x > 0 {\nx := 2\nprint(x)\n}`)).toEqual([
      expect.stringContaining("shadows an outer binding"),
    ]);
  });

  test("シャドーイングを検出: 内側スコープで関数名を隠す", () => {
    const errors = errorsOf(`fn helper() int { return 1 }\nfn main() { helper := 5\nprint(helper) }`);
    expect(errors).toEqual([expect.stringContaining("shadows an outer binding")]);
  });

  test("兄弟スコープでの同名再利用はシャドーイングではない", () => {
    expect(
      inMain(`for i := 0; i < 3; i++ {\nprint(i)\n}\nfor i := 0; i < 3; i++ {\nprint(i)\n}`),
    ).toEqual([]);
  });

  test("更新したい場合は mut + '=' を使えばシャドーイングにならない", () => {
    expect(inMain(`mut x := 1\nif x > 0 {\nx = 2\nprint(x)\n}`)).toEqual([]);
  });

  test("不変(デフォルト)の変数への再代入を検出", () => {
    expect(inMain(`x := 1\nx = 2`)).toEqual([
      expect.stringContaining("'x' is immutable — declare it with 'mut'"),
    ]);
  });

  test("mut なら再代入できる", () => {
    expect(inMain(`mut x := 1\nx = 2\nprint(x)`)).toEqual([]);
  });

  test("不変の変数への ++ を検出", () => {
    expect(inMain(`x := 1\nx++`)).toEqual([expect.stringContaining("immutable")]);
  });

  test("引数は常に不変", () => {
    const errors = errorsOf(`fn f(a: int) { a = 2 }\nfn main() { f(1) }`);
    expect(errors).toEqual([expect.stringContaining("'a' is immutable")]);
  });

  test("F-9b: 複合代入 += -= *= /= %= が使える", () => {
    expect(inMain(`mut x := 1\nx += 2\nprint(x)`)).toEqual([]);
    expect(inMain(`mut x := 10\nx -= 3\nprint(x)`)).toEqual([]);
    expect(inMain(`mut x := 2\nx *= 3\nprint(x)`)).toEqual([]);
    expect(inMain(`mut x := 10\nx /= 3\nprint(x)`)).toEqual([]);
    expect(inMain(`mut x := 10\nx %= 3\nprint(x)`)).toEqual([]);
    expect(inMain(`mut s := "a"\ns += "b"\nprint(s)`)).toEqual([]);
  });

  test("F-9b: 複合代入も不変の変数へは使えない", () => {
    expect(inMain(`x := 1\nx += 2`)).toEqual([expect.stringContaining("'x' is immutable")]);
  });

  test("F-9b: 複合代入は左右の型が合わないと検出する", () => {
    expect(inMain(`mut x := 1\nx += "a"`)).toEqual([
      expect.stringContaining(`invalid operation: int + "a"`),
    ]);
  });

  test("F-9b: 複合代入は配列の添字にも使える", () => {
    expect(inMain(`mut nums := [1, 2, 3]\nnums[0] += 10\nprint(nums[0])`)).toEqual([]);
  });

  test("F-9b: 複合代入はmapの要素には使えない(欠損キーで壊れるため)", () => {
    expect(inMain(`mut m := map<string, int>{"a": 1}\nm["a"] += 1`)).toEqual([
      expect.stringContaining("cannot use '+=' on a map entry"),
    ]);
  });

  test("F-9b: 複合代入のリテラルゼロ除算はコンパイル時に検出する", () => {
    expect(inMain(`mut x := 10\nx /= 0`)).toEqual([
      expect.stringContaining("integer division by zero"),
    ]);
  });

  test("F-9c: トップレベル定数が使える(:= と型注釈つき)", () => {
    expect(errorsOf(`maxRetries := 3\nfn main() { print(maxRetries) }`)).toEqual([]);
    expect(errorsOf(`greeting: string = "hi"\nfn main() { print(greeting) }`)).toEqual([]);
  });

  test("F-9c: トップレベル定数は宣言順に関係なく関数から参照できる(関数同士と同じ)", () => {
    expect(errorsOf(`fn main() { print(limit) }\nlimit := 10`)).toEqual([]);
  });

  test("F-9c: トップレベル定数は他の定数を参照できる(先に書かれていれば)", () => {
    expect(errorsOf(`base := 10\ndoubled := base * 2\nfn main() { print(doubled) }`)).toEqual([]);
    expect(errorsOf(`doubled := base * 2\nbase := 10\nfn main() { print(doubled) }`)).toEqual([
      expect.stringContaining("undefined: 'base'"),
    ]);
  });

  test("F-9c: トップレベル定数は不変(再代入・++ 不可)", () => {
    expect(errorsOf(`limit := 10\nfn main() { limit = 20\nprint(limit) }`)).toEqual([
      expect.stringContaining("'limit' is immutable"),
    ]);
  });

  test("F-9c: トップレベル定数は関数名などと名前がぶつかると検出する", () => {
    expect(errorsOf(`fn limit() int { return 1 }\nlimit := 10\nfn main() {}`)).toEqual([
      expect.stringContaining("'limit' is already declared"),
    ]);
  });

  test("F-9c: トップレベル定数の型注釈と値が合わないと検出する", () => {
    expect(errorsOf(`limit: string = 10\nfn main() { print(limit) }`)).toEqual([
      expect.stringContaining(`cannot use int as string`),
    ]);
  });

  test("F-9c: トップレベルの 'mut' は使えない", () => {
    expect(() => parse(`mut limit := 10\nfn main() {}`)).toThrow("top-level bindings are always immutable");
  });

  test("C風forのヘッダ変数は暗黙に可変(i++が通る)", () => {
    expect(inMain(`for i := 0; i < 3; i++ {\nprint(i)\n}`)).toEqual([]);
  });

  test("match: 網羅していないと不足メンバーを名指しでエラー", () => {
    const errors = errorsOf(`fn f() int | none | error { return 1 }
fn main() {
	x := f()
	msg := match x {
		int => "ok"
		error => "bad"
	}
	print(msg)
}`);
    expect(errors).toEqual([expect.stringContaining("match is not exhaustive — missing: none")]);
  });

  test("match: 全メンバーをカバーすれば通り、アーム内は絞り込まれる", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn main() {
	x := f()
	msg := match x {
		error => "failed"
		int => "value: \${x + 1}"
	}
	print(msg)
}`);
    expect(errors).toEqual([]);
  });

  test("match: _ でも網羅できる", () => {
    const errors = errorsOf(`fn f() int | none | error { return 1 }
fn main() {
	x := f()
	print(match x {
		int => "ok"
		_ => "not ok"
	})
}`);
    expect(errors).toEqual([]);
  });

  test("match: union のメンバーでないパターンを弾く", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn main() {
	x := f()
	print(match x {
		string => "?"
		_ => "!"
	})
}`);
    expect(errors).toEqual([expect.stringContaining("can never be string")]);
  });

  test("match: カバー済みパターンの重複を弾く", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn main() {
	x := f()
	print(match x {
		int => "a"
		int => "b"
		error => "c"
	})
}`);
    expect(errors).toEqual([expect.stringContaining("already covered")]);
  });

  test("match: _ の後のアームは到達不能", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn main() {
	x := f()
	print(match x {
		_ => "a"
		int => "b"
	})
}`);
    expect(errors).toEqual([expect.stringContaining("unreachable arm")]);
  });

  test("match: union でない対象を弾く", () => {
    expect(inMain(`x := 1\nprint(match x {\n_ => "a"\n})`)).toEqual([
      expect.stringContaining("match subject must be a union"),
    ]);
  });

  test("union型のタイポをコンパイル時に捕まえる(設計目標の実証)", () => {
    const errors = errorsOf(`type Status = "active" | "banned" | "pending"
fn ban(s: Status) {
	print(s)
}
fn main() {
	ban("actev")
}`);
    expect(errors).toEqual([
      expect.stringContaining(`cannot use "actev" as "active" | "banned" | "pending"`),
    ]);
  });

  test("type宣言: 正しい使用はエラーなし・不変ならリテラル型のまま渡せる", () => {
    const errors = errorsOf(`type Status = "active" | "banned"
fn ban(s: Status) {
	print(s)
}
fn main() {
	s := "active"
	ban(s)
	ban("banned")
}`);
    expect(errors).toEqual([]);
  });

  test("mut宣言はリテラル型を string に広げる(別の文字列を再代入できる)", () => {
    expect(inMain(`mut s := "a"\ns = "b"\nprint(s)`)).toEqual([]);
  });

  test("string はリテラル型union に入れられない(逆方向は不可)", () => {
    const errors = errorsOf(`type Status = "active" | "banned"
fn ban(s: Status) {
	print(s)
}
fn main() {
	mut s := "active"
	ban(s)
}`);
    expect(errors).toEqual([expect.stringContaining(`cannot use string as`)]);
  });

  test("type宣言: 重複と組み込み名の再宣言を弾く", () => {
    expect(errorsOf(`type A = int\ntype A = string\nfn main() {}`)).toEqual([
      expect.stringContaining("type 'A' is already declared"),
    ]);
    expect(errorsOf(`type string = int\nfn main() {}`)).toEqual([
      expect.stringContaining("'string' is a builtin type"),
    ]);
  });

  test("type宣言: 循環を検出する", () => {
    const errors = errorsOf(`type A = B | none\ntype B = A | error\nfn main() {}`);
    expect(errors).toEqual([expect.stringContaining("type alias cycle")]);
  });

  test("type宣言: unionが自分自身を裸で参照する場合も循環エラー(1件のみ)", () => {
    const errors = errorsOf(`type A = A | int\nfn main() {}`);
    expect(errors).toEqual([expect.stringContaining("type alias cycle involving 'A'")]);
  });

  test("判別可能union: 自己参照(木構造)はstructフィールド越しなら循環エラーにならない", () => {
    const errors = errorsOf(
      `type Tree = { kind: "leaf", value: int } | { kind: "node", left: Tree, right: Tree }\nfn main() {}`,
    );
    expect(errors).toEqual([]);
  });

  test("typeToString: 自己参照する判別可能unionを型エラーに出しても無限再帰しない(退行防止)", () => {
    // 循環部分は "..." で打ち切って表示する。以前はスタックオーバーフローしていた
    const errors = errorsOf(`type Tree = { kind: "leaf", value: int } | { kind: "node", left: Tree, right: Tree }
fn main() {
	t: Tree = "not a tree"
	print(t)
}`);
    expect(errors).toEqual([
      expect.stringContaining(
        `cannot use "not a tree" as { kind: "leaf", value: int } | { kind: "node", left: ..., right: ... }`,
      ),
    ]);
  });

  test("match: リテラルunion の網羅性検査(不足リテラルを名指し)", () => {
    const errors = errorsOf(`type Status = "active" | "banned" | "pending"
fn label(s: Status) string {
	return match s {
		"active" => "OK"
		"banned" => "NG"
	}
}
fn main() { print(label("active")) }`);
    expect(errors).toEqual([expect.stringContaining(`missing: "pending"`)]);
  });

  test("struct: 正しい生成・フィールドアクセスはエラーなし", () => {
    const errors = errorsOf(`struct User {
	name: string
	age: int
}
fn main() {
	u := User{name: "alice", age: 30}
	print(u.name, u.age + 1)
}`);
    expect(errors).toEqual([]);
  });

  test("struct: フィールド名のタイポを検出(生成時とアクセス時)", () => {
    const errors = errorsOf(`struct User {
	name: string
	age: int
}
fn main() {
	u := User{name: "alice", age: 30}
	print(u.nmae)
}`);
    expect(errors).toEqual([
      expect.stringContaining("User has no field 'nmae' (fields: name, age)"),
    ]);
  });

  test("struct: フィールド不足と未知フィールドを検出", () => {
    const errors = errorsOf(`struct User {
	name: string
	age: int
}
fn main() {
	u := User{name: "alice"}
	v := User{name: "bob", age: 1, admin: true}
	print(u, v)
}`);
    expect(errors).toEqual([
      expect.stringContaining("missing field(s) in User: age"),
      expect.stringContaining("User has no field 'admin'"),
    ]);
  });

  test("struct: フィールドの型不一致を検出", () => {
    const errors = errorsOf(`struct User {
	age: int
}
fn main() {
	u := User{age: "thirty"}
	print(u)
}`);
    expect(errors).toEqual([
      expect.stringContaining(`field 'age': cannot use "thirty" as int`),
    ]);
  });

  test("struct: union に入れて narrowing で取り出せる", () => {
    const errors = errorsOf(`struct User {
	name: string
}
fn find(id: int) User | none {
	if id == 1 {
		return User{name: "alice"}
	}
	return none
}
fn main() {
	u := find(1)
	if u is none {
		return
	}
	print(u.name)
}`);
    expect(errors).toEqual([]);
  });

  test("struct: narrowing 前のフィールドアクセスを弾く", () => {
    const errors = errorsOf(`struct User {
	name: string
}
fn find(id: int) User | none {
	return none
}
fn main() {
	u := find(1)
	print(u.name)
}`);
    expect(errors).toEqual([
      expect.stringContaining("cannot access field or method on User | none — narrow it first"),
    ]);
  });

  test("struct: 再帰struct(Node | none)が書ける", () => {
    const errors = errorsOf(`struct Node {
	value: int
	next: Node | none
}
fn sum(n: Node | none) int {
	if n is none {
		return 0
	}
	return n.value + sum(n.next)
}
fn main() {
	print(sum(Node{value: 1, next: Node{value: 2, next: none}}))
}`);
    expect(errors).toEqual([]);
  });

  test("判別可能union: 宣言・構築・matchでの絞り込み・フィールドアクセスが通る", () => {
    const errors = errorsOf(`struct User {
	name: string
}
type GetUserResponse = { kind: "ok", user: User } | { kind: "notFound" } | { kind: "unauthorized" }
fn getUser(ok: bool) GetUserResponse {
	if ok {
		return GetUserResponse{ kind: "ok", user: User{name: "alice"} }
	}
	return GetUserResponse{ kind: "notFound" }
}
fn describe(res: GetUserResponse) string {
	return match res {
		{ kind: "ok" } => "found: \${res.user.name}"
		{ kind: "notFound" } => "not found"
		{ kind: "unauthorized" } => "unauthorized"
	}
}
fn main() { print(describe(getUser(true))) }`);
    expect(errors).toEqual([]);
  });

  test("判別可能union: matchが網羅的でないと不足メンバーの形を名指しで報告する", () => {
    const errors = errorsOf(`type Resp = { kind: "ok" } | { kind: "notFound" }
fn describe(res: Resp) string {
	return match res {
		{ kind: "ok" } => "ok"
	}
}
fn main() { print(describe(Resp{kind: "ok"})) }`);
    expect(errors).toEqual([expect.stringContaining(`missing: { kind: "notFound" }`)]);
  });

  test("判別可能union: 絞り込んだ枝以外のフィールドにはアクセスできない", () => {
    const errors = errorsOf(`struct User { name: string }
type Resp = { kind: "ok", user: User } | { kind: "notFound" }
fn describe(res: Resp) string {
	return match res {
		{ kind: "notFound" } => res.user.name
		{ kind: "ok" } => "ok"
	}
}
fn main() { print(describe(Resp{kind: "notFound"})) }`);
    expect(errors).toEqual([
      expect.stringContaining(`{ kind: "notFound" } has no field 'user'`),
    ]);
  });

  test("判別可能union: タグで解決した後もフィールド不明は通常どおり検出する(F-7)", () => {
    const errors = errorsOf(`type Resp = { kind: "ok" } | { kind: "notFound" }
fn main() {
	r := Resp{kind: "ok", extra: 1}
	print(r)
}`);
    expect(errors).toEqual([
      expect.stringContaining("Resp has no field 'extra' (fields: kind)"),
    ]);
  });

  test("判別可能union: タグの値がどのメンバーにも無いと構築エラー(F-7)", () => {
    const errors = errorsOf(`type Resp = { kind: "ok" } | { kind: "notFound" }
fn main() {
	r := Resp{kind: "nope"}
	print(r)
}`);
    expect(errors).toEqual([
      expect.stringContaining(`no member of 'Resp' has kind: "nope"`),
    ]);
  });

  test("F-7: タグ(共通のリテラル型フィールド)が無いunionはstruct memberが2個以上だと宣言時エラー", () => {
    const errors = errorsOf(`type Resp = { x: int } | { x: float }
fn main() { print(1) }`);
    expect(errors).toEqual([
      expect.stringContaining("discriminated union 'Resp' needs a tag field"),
    ]);
  });

  test("F-7: タグを書き忘れる(または非リテラル値)と構築エラー", () => {
    const errors = errorsOf(`type Resp = { kind: "ok" } | { kind: "notFound" }
fn main() {
	r := Resp{}
	print(r)
}`);
    expect(errors[0]).toContain("needs its tag field 'kind' set to select a member");
  });

  test("F-7: タグ候補のフィールドがあっても値が重複していれば宣言時エラー", () => {
    const errors = errorsOf(
      `type Resp = { kind: "ok", a: int } | { kind: "ok", b: int }\nfn main() { print(1) }`,
    );
    expect(errors).toEqual([
      expect.stringContaining("discriminated union 'Resp' needs a tag field"),
    ]);
  });

  test("F-7: 名前付きstruct同士のunion(Circle | Square)はタグ不要 — 自分の名前で構築する", () => {
    const errors = errorsOf(`struct Circle { kind: string  r: float }
struct Square { kind: string  s: float }
type Shape = Circle | Square
fn main() {
	c: Shape = Circle{kind: "circle", r: 2.0}
	if c is Circle { print(c.r) }
}`);
    expect(errors).toEqual([]);
  });

  test("F-7: メンバー追加という『遠隔』変更が既存の正しくタグ付けされたリテラルを壊さない", () => {
    // 判別可能unionにメンバーを追加しても、離れた場所の既存リテラル(タグで解決)は無傷のまま
    const before = errorsOf(`type Resp = { kind: "ok", value: int } | { kind: "notFound" }
fn make() Resp { return Resp{kind: "ok", value: 1} }
fn main() { print(make()) }`);
    const after = errorsOf(`type Resp = { kind: "ok", value: int } | { kind: "notFound" } | { kind: "forbidden" }
fn make() Resp { return Resp{kind: "ok", value: 1} }
fn main() { print(make()) }`);
    expect(before).toEqual([]);
    expect(after).toEqual([]);
  });

  test("F-14: mesh/io — io.args() / io.readFile(path) が型検査を通る", () => {
    expect(errorsOf(`import "mesh/io"
fn main() {
	args := io.args()
	print(len(args))
	content := io.readFile("x.txt")
	if content is error {
		return
	}
	print(content)
}`)).toEqual([]);
  });

  test("F-14: io.readFile は引数の型を検査する", () => {
    expect(errorsOf(`import "mesh/io"\nfn main() { print(io.readFile(1)) }`)).toEqual([
      expect.stringContaining(`cannot use int as string`),
    ]);
  });

  test("F-14: mesh/json — json.parse/json.stringify とjson.Value型が使える", () => {
    const errors = errorsOf(`import "mesh/json"
fn main() {
	v := json.parse("{}")
	if v is error {
		return
	}
	print(json.stringify(v))
}`);
    expect(errors).toEqual([]);
  });

  test("F-14: json.Value は判別可能unionとしてmatch/narrowingできる(タグは'kind')", () => {
    const errors = errorsOf(`import "mesh/json"
fn describe(v: json.Value) string {
	return match v {
		{ kind: "str" } => v.s
		{ kind: "num" } => str(v.n)
		{ kind: "bool" } => str(v.b)
		{ kind: "null" } => "null"
		{ kind: "arr" } => str(len(v.items))
		{ kind: "obj" } => str(len(v.entries))
	}
}
fn main() { print(describe(json.Value{kind: "str", s: "hi"})) }`);
    expect(errors).toEqual([]);
  });

  test("F-14: json.Value{...} は他の判別可能unionと同じくタグ値で構築できる", () => {
    expect(errorsOf(`import "mesh/json"\nfn main() { print(json.Value{kind: "num", n: 1.5}) }`)).toEqual(
      [],
    );
    expect(errorsOf(`import "mesh/json"\nfn main() { print(json.Value{kind: "nope"}) }`)).toEqual([
      expect.stringContaining("no member of 'Value' has kind: \"nope\""),
    ]);
  });

  test("F-14/C-6: mesh/io, mesh/json, mesh/http 以外の mesh/* は未実装のまま(回帰確認)", () => {
    expect(errorsOf(`import "mesh/dom"\nfn main() { print(1) }`)).toEqual([
      expect.stringContaining("unknown package 'mesh/dom'"),
    ]);
  });

  test("F-15: '_test.mesh' 内の 'test'+接頭辞のfn(() none | error)がテストとして発見される", () => {
    const tests = testsOf(`fn testAddition() none | error {
	if 2 + 2 != 4 {
		return error("bad math")
	}
	return none
}
fn helper() int { return 1 }`);
    expect(tests.map((t) => t.name)).toEqual(["testAddition"]);
    expect(tests[0].jsName).toBe("testAddition");
    expect(errorsOfTestFile(`fn testAddition() none | error { return none }\nfn helper() int { return 1 }`)).toEqual(
      [],
    );
  });

  test("F-15: 'test'名の関数でも通常の.meshファイル(非_test.mesh)なら発見されない", () => {
    // _test.mesh 以外はテストとして扱わない(命名規約はテストファイル内でだけ効く)
    expect(check(parse(`fn testAddition() none | error { return none }\nfn main() {}`))).toEqual([]);
  });

  test("F-15: テスト関数のシグネチャが不正だと検出する(引数あり・戻り値が違う)", () => {
    expect(errorsOfTestFile(`fn testBad(x: int) int { return x }`)).toEqual([
      expect.stringContaining(
        "test function 'testBad' must take no parameters and return 'none | error', got (int) int",
      ),
    ]);
    expect(errorsOfTestFile(`fn testBad() error { return error("x") }`)).toEqual([
      expect.stringContaining("must take no parameters and return 'none | error'"),
    ]);
  });

  test("F-15: mesh testはmain()が無くても検査できる(TDD的に先にテストだけ書ける)", () => {
    expect(errorsOfTestFile(`fn testX() none | error { return none }`)).toEqual([]);
  });

  test("struct: 名前的型付け(F-3) — 形が同じでも名前が違えば別の型(単位型の事故を防ぐ)", () => {
    const errors = errorsOf(`struct Meters { value: float }
struct Dollars { value: float }
fn charge(amount: Dollars) { print(amount.value) }
fn main() {
	distance := Meters{ value: 100.0 }
	charge(distance)
}`);
    expect(errors).toEqual([expect.stringContaining("cannot use Meters as Dollars")]);
  });

  test("struct: 名前的でも、無名{...}メンバーの場所には同形の名前付きstructを渡せる", () => {
    const errors = errorsOf(`struct Ok { kind: "ok" }
type Resp = { kind: "ok" } | { kind: "ng" }
fn describe(r: Resp) string {
	return match r {
		{ kind: "ok" } => "OK"
		{ kind: "ng" } => "NG"
	}
}
fn main() {
	print(describe(Ok{ kind: "ok" }))
}`);
    expect(errors).toEqual([]);
  });

  test("map: 読みは V | none なので絞り込み前に使うとエラー", () => {
    expect(inMain(`ages := map<string, int>{"a": 1}\nprint(ages["a"] + 1)`)).toEqual([
      expect.stringContaining("invalid operation"),
    ]);
  });

  test("map: or / is none で取り出せる", () => {
    expect(
      inMain(`ages := map<string, int>{"a": 1}
x := ages["a"] or 0
print(x + 1)
v := ages["b"]
if v is none {
	print("missing")
	return
}
print(v * 2)`),
    ).toEqual([]);
  });

  test("map: キー・値の型検査", () => {
    expect(inMain(`ages := map<string, int>{"a": "b"}`)).toEqual([
      expect.stringContaining(`map value must be int`),
    ]);
    expect(inMain(`ages := map<string, int>{}\nprint(ages[1] or 0)`)).toEqual([
      expect.stringContaining("map key must be string"),
    ]);
    expect(inMain(`ages := map<string, int>{}\nages["a"] = none`)).toEqual([
      expect.stringContaining("cannot assign none to int"),
    ]);
  });

  test("range: 配列は完全形が必須(単変数はエラー)", () => {
    expect(inMain(`nums := [1, 2]\nfor v := range nums {\nprint(v)\n}`)).toEqual([
      expect.stringContaining("needs two names"),
    ]);
  });

  test("range: int は単変数のみ・配列とmapは2変数で型が付く", () => {
    expect(
      inMain(`nums := [10, 20]
for i, v := range nums {
	print(i + v)
}
ages := map<string, int>{"a": 1}
for k, v := range ages {
	print("\${k}: \${v * 2}")
}
for i := range 3 {
	print(i)
}`),
    ).toEqual([]);
  });

  test("型注釈つき宣言: mut best := none 相当が書ける(不在アキュムレータ)", () => {
    const errors = errorsOf(`fn main() {
	mut best: string | none = none
	best = "red"
	if best is none {
		return
	}
	print(best)
}`);
    expect(errors).toEqual([]);
  });

  test("型注釈つき宣言: 宣言型に入らない値を弾く", () => {
    expect(inMain(`x: int = "hello"`)).toEqual([
      expect.stringContaining(`cannot use "hello" as int`),
    ]);
  });

  test("型注釈つき宣言: 不変(mutなし)は再代入できない", () => {
    expect(inMain(`x: int = 1\nx = 2`)).toEqual([
      expect.stringContaining("'x' is immutable"),
    ]);
  });

  test("型付き配列リテラル: 空の Todo[]{} はF-9aで廃止 — xs: T[] = [] を使えという案内が出る", () => {
    expect(() =>
      errorsOf(`struct Todo {
	id: int
}
fn main() {
	todos := Todo[]{}
	print(len(todos))
}`),
    ).toThrow("empty typed array literal 'T[]{}' was removed");
  });

  test("型付き配列リテラル: 要素の型を検査する", () => {
    expect(inMain(`nums := int[]{1, "two"}\nprint(len(nums))`)).toEqual([
      expect.stringContaining(`array element must be int, got "two"`),
    ]);
  });

  test("素の空配列 [] は型注釈があれば型付き配列になる(any[]は互換)", () => {
    // 型注釈つき宣言 / 戻り値の期待型があれば [] を Todo[] として使える
    const errors = errorsOf(`struct Todo {
	id: int
}
fn make() Todo[] {
	return []
}
fn main() {
	todos: Todo[] = []
	push(todos, Todo{id: 1})
	print(len(todos) + len(make()))
}`);
    expect(errors).toEqual([]);
  });

  test("具体要素型の配列は別の要素型に代入できない(int[] を string[] に不可)", () => {
    expect(inMain(`xs: string[] = [1, 2]`)).toEqual([
      expect.stringContaining("cannot use int[] as string[]"),
    ]);
  });

  test("range: 範囲にできない型を弾く", () => {
    expect(inMain(`for i, v := range "abc" {\nprint(i, v)\n}`)).toEqual([
      expect.stringContaining("cannot range over"),
    ]);
  });

  test("リテラルのゼロ除算はコンパイル時に検出", () => {
    expect(inMain(`print(1 / 0)`)).toEqual([expect.stringContaining("integer division by zero")]);
    expect(inMain(`print(1 % 0)`)).toEqual([expect.stringContaining("integer modulo by zero")]);
  });

  test("mut の後に宣言以外は書けない", () => {
    expect(() => inMain(`x := 1\nmut x = 2`)).toThrow(CompileError);
  });

  test("union戻り値: メンバーの値は返せる・非メンバーは弾く", () => {
    expect(errorsOf(`fn f() int | error { return error("x") }\nfn main() { print(f()) }`)).toEqual([]);
    expect(errorsOf(`fn f() int | error { return "text" }\nfn main() { print(f()) }`)).toEqual([
      expect.stringContaining(`cannot return "text" as int | error`),
    ]);
  });

  test("narrowing: is error で絞り込めば残りは int として使える", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn main() {
	x := f()
	if x is error {
		return
	}
	print(x + 1)
}`);
    expect(errors).toEqual([]);
  });

  test("narrowing なしで union を演算に使うとエラー", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }\nfn main() { x := f()\nprint(x + 1) }`);
    expect(errors).toEqual([expect.stringContaining("invalid operation")]);
  });

  test("narrowing: else 側は成功型に絞り込まれる", () => {
    const errors = errorsOf(`fn f() int | none { return none }
fn main() {
	x := f()
	if x is none {
		print("empty")
	} else {
		print(x * 2)
	}
}`);
    expect(errors).toEqual([]);
  });

  test("narrowing(F-6): フィールドパス(n.next is none)で絞り込める", () => {
    const errors = errorsOf(`struct Node {
	value: int
	next: Node | none
}
fn sum(n: Node) int {
	if n.next is none {
		return n.value
	}
	return n.value + sum(n.next)
}
fn main() {
	print(sum(Node{value: 1, next: none}))
}`);
    expect(errors).toEqual([]);
  });

  test("narrowing(F-6): フィールドパスのelse側も絞り込まれる", () => {
    const errors = errorsOf(`struct Node {
	value: int
	next: Node | none
}
fn describe(n: Node) string {
	if n.next is none {
		return "leaf"
	} else {
		return str(n.next.value)
	}
}
fn main() { print(describe(Node{value: 1, next: none})) }`);
    expect(errors).toEqual([]);
  });

  test("narrowing(F-6): フィールドへの代入は古いnarrowing事実を無効化する", () => {
    const errors = errorsOf(`struct Node {
	value: int
	next: Node | none
}
fn main() {
	a := Node{value: 1, next: none}
	if a.next is none {
		a.next = Node{value: 2, next: none}
		print(a.next.value)
	}
}`);
    expect(errors).toEqual([
      expect.stringContaining("cannot access field or method on Node | none — narrow it first"),
    ]);
  });

  test("narrowing(F-6): フィールドパスを絞り込まずに使うとエラーのまま", () => {
    const errors = errorsOf(`struct Node {
	value: int
	next: Node | none
}
fn bad(n: Node) int { return n.next.value }
fn main() { print(bad(Node{value: 1, next: none})) }`);
    expect(errors).toEqual([
      expect.stringContaining("cannot access field or method on Node | none — narrow it first"),
    ]);
  });

  test("narrowing(F-6): && は左の is が右辺とthen節に効く", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn main() {
	x := f()
	if x is int && x > 0 {
		print(x + 1)
	}
}`);
    expect(errors).toEqual([]);
  });

  test("narrowing(F-6): || は両方falseの側(else)がド・モルガンで絞り込まれる", () => {
    const errors = errorsOf(`fn main() {
	l: int | none = 1
	r: int | none = 2
	if l is none || r is none {
		return
	}
	print(l + r)
}`);
    expect(errors).toEqual([]);
  });

  test("narrowing(F-6): ! はド・モルガンでthen節が絞り込まれる", () => {
    const errors = errorsOf(`fn main() {
	v: int | none = 5
	if !(v is none) {
		print(v + 1)
	}
}`);
    expect(errors).toEqual([]);
  });

  test("== none は 'is none' へ誘導する", () => {
    const errors = errorsOf(`fn f() int | none { return none }\nfn main() { x := f()\nif x == none {\nreturn\n}\nprint(x) }`);
    expect(errors).toEqual([expect.stringContaining("use 'is none'")]);
  });

  test("'?' は戻り値型に失敗メンバーが無いと使えない", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn g() int {
	return f()?
}
fn main() { print(g()) }`);
    expect(errors).toEqual([expect.stringContaining("'?' propagates error")]);
  });

  test("'?' は失敗メンバーを含む関数内なら使える", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn g() int | error {
	v := f()?
	return v * 2
}
fn main() { print(g()) }`);
    expect(errors).toEqual([]);
  });

  test("旧記法の後置 '!' は '?' への誘導エラー(パース時)", () => {
    expect(() => parse(`fn f() int | error { return 1 }\nfn main() { x := f()!\nprint(x) }`)).toThrow(
      "postfix '!' was renamed — use '?'",
    );
  });

  test("'or' のフォールバックは成功型に合わせる", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }\nfn main() { x := f() or _ => "zero"\nprint(x) }`);
    expect(errors).toEqual([expect.stringContaining("'or' fallback must be int")]);
  });

  test("'or' はerrorを含むと束縛形が必須(Go式の明示性)・束縛形なら通る", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }\nfn main() { x := f() or 0\nprint(x) }`);
    expect(errors).toEqual([
      expect.stringContaining("'or' would silently discard an error"),
    ]);
    expect(
      errorsOf(`fn f() int | error { return 1 }\nfn main() { x := f() or _ => 0\nprint(x) }`),
    ).toEqual([]);
    // e には失敗値(error)が束縛される
    expect(
      errorsOf(`fn f() int | error { return 1 }\nfn main() { x := f() or e => len("\${e}")\nprint(x) }`),
    ).toEqual([]);
  });

  test("'or' は失敗しない型には使えない", () => {
    expect(inMain(`x := 1 or 2\nprint(x)`)).toEqual([
      expect.stringContaining("left side of 'or' never fails"),
    ]);
  });

  describe("構造化エラー(F-2後半): error type/struct と'?'/'or'の和解", () => {
    test("'?' は error type とタグ付けされたメンバーも伝播できる", () => {
      const errors = errorsOf(`error type DbError = { kind: "notFound", table: string } | { kind: "timeout", ms: int }
fn find(id: int) int | DbError {
	return DbError{kind: "notFound", table: "users"}
}
fn useIt(id: int) int | DbError {
	v := find(id)?
	return v + 1
}
fn main() { print(useIt(1)) }`);
      expect(errors).toEqual([]);
    });

    test("'error struct X { ... }' 単体形でも伝播できる", () => {
      const errors = errorsOf(`error struct DbError { table: string }
fn find(id: int) int | DbError { return DbError{table: "users"} }
fn useIt(id: int) int | DbError {
	v := find(id)?
	return v
}
fn main() { print(useIt(1)) }`);
      expect(errors).toEqual([]);
    });

    test("'error' マーカーの無い普通のstructは今まで通り伝播できない", () => {
      const errors = errorsOf(`struct NotAnError { message: string }
fn find(id: int) int | NotAnError { return NotAnError{message: "x"} }
fn useIt(id: int) int | NotAnError {
	v := find(id)?
	return v
}
fn main() { print(useIt(1)) }`);
      expect(errors).toEqual([
        expect.stringContaining("'?' has nothing to propagate — int | NotAnError has no none/error/error type"),
      ]);
    });

    test("'or' はerror typeを含むと束縛形が必須で、束縛したら kind で分岐できる", () => {
      const src = (form: string) => `error type DbError = { kind: "notFound", table: string } | { kind: "timeout", ms: int }
fn find(id: int) int | DbError { return DbError{kind: "notFound", table: "users"} }
fn main() {
	${form}
	print(x)
}`;
      expect(errorsOf(src(`x := find(1) or -1`))).toEqual([
        expect.stringContaining("'or' would silently discard an error"),
      ]);
      expect(
        errorsOf(src(`x := find(1) or e => match e { { kind: "notFound" } => -1  { kind: "timeout" } => -2 }`)),
      ).toEqual([]);
    });

    test("'?' の文脈つき形( f() ? \"ctx\" )は構造化エラーを弾く(メッセージに変換できないため)", () => {
      const errors = errorsOf(`error struct DbError { table: string }
fn find(id: int) int | DbError { return DbError{table: "users"} }
fn useIt(id: int) int | DbError {
	v := find(id) ? "useIt failed"
	return v
}
fn main() { print(useIt(1)) }`);
      expect(errors).toEqual([
        expect.stringContaining("'?' with context can't convert DbError to a message"),
      ]);
    });

    test("error type の宣言時検証: メンバーはstruct形でないといけない", () => {
      const errors = errorsOf(`error type Bad = int\nfn main() { print(1) }`);
      expect(errors).toEqual([
        expect.stringContaining("error type 'Bad' members must be struct-shaped"),
      ]);
    });

    test("error type の宣言時検証: 既存の名前付き型をそのままタグ付けすることはできない", () => {
      const errors = errorsOf(`struct Existing { x: int }
error type Aliased = Existing
fn main() { print(1) }`);
      expect(errors).toEqual([
        expect.stringContaining("error type 'Aliased' can't tag the existing type 'Existing'"),
      ]);
    });

    test("既存の none/error の伝播(F-2前半の文脈つき?含む)は今まで通り動く", () => {
      const errors = errorsOf(`fn parse(s: string) int | error {
	return toInt(s)
}
fn useIt(s: string) int | error {
	v := parse(s) ? "bad config"
	return v + 1
}
fn main() { print(useIt("41")) }`);
      expect(errors).toEqual([]);
    });
  });

  test("if の条件は bool でなければならない", () => {
    expect(inMain(`if 1 {\n}`)).toEqual([expect.stringContaining("must be bool")]);
  });

  test("チャネルの要素型の不一致を検出", () => {
    expect(inMain(`ch := chan<int>(none)\nch <- "hi"`)).toEqual([
      expect.stringContaining(`cannot send "hi" to chan<int>`),
    ]);
  });

  test("main がなければエラー", () => {
    expect(errorsOf(`fn f() {}`)).toEqual([expect.stringContaining("missing 'fn main()'")]);
  });

  test("補間内の式も型検査される(未定義変数を検出)", () => {
    expect(inMain(`print("hello \${nobody}")`)).toEqual([
      expect.stringContaining("undefined: 'nobody'"),
    ]);
  });

  test("none は union に none が含まれる場合のみ返せる", () => {
    expect(errorsOf(`fn f() int | none { return none }\nfn main() { print(f()) }`)).toEqual([]);
    expect(errorsOf(`fn f() int { return none }\nfn main() { print(f()) }`)).toEqual([
      expect.stringContaining("cannot return none as int"),
    ]);
  });

  test("contains: 正しい使用はエラーなし・要素型の不一致を検出", () => {
    expect(inMain(`nums := [1, 2, 3]\nprint(contains(nums, 2))`)).toEqual([]);
    expect(inMain(`nums := [1, 2, 3]\nprint(contains(nums, "x"))`)).toEqual([
      expect.stringContaining(`contains() second argument must be int, got "x"`),
    ]);
  });

  test("indexOf: 戻り値は int | none なので絞り込みが必要", () => {
    expect(inMain(`nums := [1, 2, 3]\ni := indexOf(nums, 2)\nprint(i + 1)`)).toEqual([
      expect.stringContaining("invalid operation"),
    ]);
    expect(inMain(`nums := [1, 2, 3]\ni := indexOf(nums, 2)\nif i is none {\nreturn\n}\nprint(i + 1)`)).toEqual(
      [],
    );
  });

  test("F-9d: get(arr, i) は T | none なので絞り込みが必要", () => {
    expect(inMain(`nums := [1, 2, 3]\nv := get(nums, 0)\nprint(v + 1)`)).toEqual([
      expect.stringContaining("invalid operation"),
    ]);
    expect(inMain(`nums := [1, 2, 3]\nv := get(nums, 0)\nif v is none {\nreturn\n}\nprint(v + 1)`)).toEqual([]);
    expect(inMain(`nums := [1, 2, 3]\nprint(get(nums, 0) or 0)`)).toEqual([]);
  });

  test("F-9d: get() は添字がintでないと検出する", () => {
    expect(inMain(`nums := [1, 2, 3]\nprint(get(nums, "0"))`)).toEqual([
      expect.stringContaining(`index must be int, got "0"`),
    ]);
  });

  test("F-9d: get() は配列以外だと検出する", () => {
    expect(inMain(`print(get("not an array", 0))`)).toEqual([
      expect.stringContaining("get() requires an array"),
    ]);
  });

  test("keys/values: mapから配列の型を正しく推論", () => {
    expect(
      inMain(`ages := map<string, int>{"a": 1}\nnames := keys(ages)\nprint(names[0])\nnums := values(ages)\nprint(nums[0])`),
    ).toEqual([]);
  });

  test("sort: int[]/string[]は通り、structなど非順序型は弾く", () => {
    expect(inMain(`nums := [3, 1, 2]\nsorted := sort(nums)\nprint(sorted[0])`)).toEqual([]);
    expect(inMain(`words := ["b", "a"]\nprint(sort(words))`)).toEqual([]);
    const errors = errorsOf(`struct User { name: string }
fn main() {
	us := [User{name: "b"}, User{name: "a"}]
	print(sort(us))
}`);
    expect(errors).toEqual([expect.stringContaining("sort() requires int[], float[] or string[]")]);
  });

  test("split: 常に string[] を返す(unionではない)", () => {
    expect(inMain(`parts := split("a,b,c", ",")\nprint(len(parts), parts[0])`)).toEqual([]);
    expect(inMain(`parts := split(1, ",")`)).toEqual([
      expect.stringContaining("split() requires a string"),
    ]);
  });

  test("join: string[] 以外を弾く", () => {
    expect(inMain(`words := ["a", "b"]\nprint(join(words, "-"))`)).toEqual([]);
    expect(inMain(`nums := [1, 2]\nprint(join(nums, "-"))`)).toEqual([
      expect.stringContaining("join() requires string[]"),
    ]);
  });

  test("trim/upper/lower: string → string", () => {
    expect(inMain(`print(upper(trim(lower(" Hi "))))`)).toEqual([]);
    expect(inMain(`print(upper(1))`)).toEqual([expect.stringContaining("upper() requires a string")]);
  });

  test("toInt: 戻り値は int | error なので絞り込みが必要", () => {
    expect(inMain(`n := toInt("42")\nprint(n + 1)`)).toEqual([
      expect.stringContaining("invalid operation"),
    ]);
    expect(inMain(`n := toInt("42")\nif n is error {\nreturn\n}\nprint(n + 1)`)).toEqual([]);
    expect(inMain(`n := toInt("42") or _ => 0\nprint(n + 1)`)).toEqual([]);
  });

  test("filter: 正しい述語はエラーなし・パラメータ型/戻り値型の不一致を検出", () => {
    expect(inMain(`nums := [1, 2, 3]\nevens := filter(nums, fn(n: int) bool { return n % 2 == 0 })\nprint(evens)`)).toEqual(
      [],
    );
    expect(inMain(`nums := [1, 2, 3]\nprint(filter(nums, fn(s: string) bool { return true }))`)).toEqual([
      expect.stringContaining("filter() callback must take a single int parameter"),
    ]);
    expect(inMain(`nums := [1, 2, 3]\nprint(filter(nums, fn(n: int) int { return n }))`)).toEqual([
      expect.stringContaining("filter() callback must return bool, got int"),
    ]);
  });

  test("F-8: map(): 戻り値の型が変わってよい(int[] → string[])", () => {
    expect(
      inMain(`nums := [1, 2, 3]\nlabels := map(nums, fn(n: int) string { return str(n) })\nprint(labels[0])`),
    ).toEqual([]);
    expect(inMain(`nums := [1, 2, 3]\nprint(map(nums, fn(s: string) int { return 1 }))`)).toEqual([
      expect.stringContaining("map() callback must take a single int parameter"),
    ]);
  });

  test("reduce: 初期値・要素・戻り値の型の整合性を検査", () => {
    expect(inMain(`nums := [1, 2, 3]\ntotal := reduce(nums, fn(acc: int, n: int) int { return acc + n }, 0)\nprint(total + 1)`)).toEqual(
      [],
    );
    // 蓄積型(Acc)は要素型と別でよい(int[] を string に畳み込む)
    expect(
      inMain(`nums := [1, 2, 3]\ns := reduce(nums, fn(acc: string, n: int) string { return acc + str(n) }, "")\nprint(s + "!")`),
    ).toEqual([]);
    expect(inMain(`nums := [1, 2, 3]\nprint(reduce(nums, fn(n: int) int { return n }, 0))`)).toEqual([
      expect.stringContaining("reduce() callback must take (accumulator, element)"),
    ]);
    expect(
      inMain(`nums := [1, 2, 3]\nprint(reduce(nums, fn(acc: string, n: int) string { return acc }, 0))`),
    ).toEqual([expect.stringContaining("reduce() initial value must be string, got int")]);
  });

  test("F-8: 'map' は文脈依存キーワード — map<K,V>型/リテラルとmap(arr,f)呼び出しが共存する", () => {
    expect(
      inMain(`nums := [1, 2, 3]\ndoubled := map(nums, fn(n: int) int { return n * 2 })\nprint(doubled[0])`),
    ).toEqual([]);
    expect(inMain(`ages := map<string, int>{"a": 1}\nprint(ages["a"])`)).toEqual([]);
    expect(inMain(`ages: map<string, int> = map<string, int>{}\nprint(len(ages))`)).toEqual([]);
  });

  test("退行防止: 'map' を裸の値として使うと構文エラーではなくbuiltin-as-value診断になる", () => {
    // レビューで見つかった穴: '<' が来なければ即 expect("<", "after 'map'") が発火し、
    // "expected '<' after 'map'" という的外れなsyntax-errorになっていた(F-8のparser修正)
    expect(inMain(`x := map\nprint(x)`)).toEqual([
      expect.stringContaining("'map' is a builtin function — call it like map(...)"),
    ]);
    expect(inMain(`print(map)`)).toEqual([
      expect.stringContaining("'map' is a builtin function — call it like map(...)"),
    ]);
  });

  test("メソッド: 正しい宣言・呼び出しはエラーなし", () => {
    const errors = errorsOf(`struct Todo {
	title: string
	done: bool
}
fn (t: Todo) complete() Todo {
	return Todo{title: t.title, done: true}
}
fn (t: Todo) render() string {
	return t.title
}
fn main() {
	t := Todo{title: "a", done: false}
	t2 := t.complete()
	print(t2.render())
}`);
    expect(errors).toEqual([]);
  });

  test("メソッド: 名前空間はグローバル関数と分離している(render(t)は呼べない)", () => {
    const errors = errorsOf(`struct Todo { title: string }
fn (t: Todo) render() string { return t.title }
fn main() {
	t := Todo{title: "a"}
	print(render(t))
}`);
    expect(errors).toEqual([expect.stringContaining("undefined: 'render'")]);
  });

  test("メソッド: 別structなら同名メソッドが衝突しない", () => {
    const errors = errorsOf(`struct User { name: string }
struct Order { id: int }
fn (u: User) describe() string { return u.name }
fn (o: Order) describe() string { return str(o.id) }
fn main() {
	u := User{name: "a"}
	o := Order{id: 1}
	print(u.describe(), o.describe())
}`);
    expect(errors).toEqual([]);
  });

  test("メソッド: 引数の数・型の不一致を検出", () => {
    const errors1 = errorsOf(`struct Todo { title: string }
fn (t: Todo) rename(newTitle: string) Todo { return Todo{title: newTitle} }
fn main() {
	t := Todo{title: "a"}
	print(t.rename())
}`);
    expect(errors1).toEqual([expect.stringContaining("expected 1 argument(s), got 0")]);

    const errors2 = errorsOf(`struct Todo { title: string }
fn (t: Todo) rename(newTitle: string) Todo { return Todo{title: newTitle} }
fn main() {
	t := Todo{title: "a"}
	print(t.rename(1))
}`);
    expect(errors2).toEqual([expect.stringContaining("argument 1: cannot use int as string")]);
  });

  test("メソッド: レシーバはstruct限定(intなどは拒否)", () => {
    const errors = errorsOf(`fn (n: int) double() int { return n * 2 }\nfn main() {}`);
    expect(errors).toEqual([expect.stringContaining("method receiver must be a struct type, got int")]);
  });

  test("メソッド: フィールドと同名は宣言時に拒否", () => {
    const errors = errorsOf(`struct Todo { title: string }
fn (t: Todo) title() string { return t.title }
fn main() {}`);
    expect(errors).toEqual([expect.stringContaining("Todo already has a field named 'title'")]);
  });

  test("メソッド: 同じstructへの重複宣言を拒否", () => {
    const errors = errorsOf(`struct Todo { title: string }
fn (t: Todo) render() string { return t.title }
fn (t: Todo) render() string { return t.title }
fn main() {}`);
    expect(errors).toEqual([expect.stringContaining("Todo already has a method named 'render'")]);
  });

  test("メソッド: union未絞り込みでの呼び出しを検出", () => {
    const errors = errorsOf(`struct User { name: string }
fn find(id: int) User | none { return none }
fn (u: User) describe() string { return u.name }
fn main() {
	u := find(1)
	print(u.describe())
}`);
    expect(errors).toEqual([
      expect.stringContaining("cannot access field or method on User | none — narrow it first"),
    ]);
  });

  test("メソッド: ()を付けずに参照すると修正方法つきエラー", () => {
    const errors = errorsOf(`struct Todo { title: string }
fn (t: Todo) render() string { return t.title }
fn main() {
	t := Todo{title: "a"}
	print(t.render)
}`);
    expect(errors).toEqual([
      expect.stringContaining("'render' is a method — call it like render(...)"),
    ]);
  });

  test("メソッド: 連鎖呼び出し(chaining)が書ける", () => {
    const errors = errorsOf(`struct Todo {
	title: string
	done: bool
}
fn (t: Todo) complete() Todo { return Todo{title: t.title, done: true} }
fn (t: Todo) render() string {
	if t.done { return "[x] " + t.title }
	return "[ ] " + t.title
}
fn main() {
	todos := [Todo{title: "a", done: false}]
	print(todos[0].complete().render())
}`);
    expect(errors).toEqual([]);
  });

  test("受信は常に T | closed — 絞り込む前に演算に使うとエラー", () => {
    expect(inMain(`ch := chan<int>(none)\nv := <-ch\nprint(v + 1)`)).toEqual([
      expect.stringContaining("invalid operation"),
    ]);
    expect(
      inMain(`ch := chan<int>(none)\nv := <-ch\nif v is closed {\nreturn\n}\nprint(v + 1)`),
    ).toEqual([]);
  });

  test("is: 型名パターンで絞り込める(matchと同じパターン)", () => {
    expect(inMain(`ch := chan<int>(none)\nv := <-ch\nif v is int {\nprint(v + 1)\n}`)).toEqual([]);
    const errors = errorsOf(`struct User { name: string }
fn find(id: int) User | none | error {
	if id == 1 { return User{name: "a"} }
	return none
}
fn main() {
	u := find(1)
	if u is User {
		print(u.name)
	}
}`);
    expect(errors).toEqual([]);
  });

  test("is: 文字列リテラルパターンで絞り込め、else側は残りのメンバーになる", () => {
    const errors = errorsOf(`type Status = "active" | "banned" | "pending"
fn label(s: Status) string {
	if s is "active" {
		return "OK"
	}
	return match s {
		"banned" => "NG"
		"pending" => "WAIT"
	}
}
fn main() { print(label("active")) }`);
    expect(errors).toEqual([]);
  });

  test("is: 部分構造パターンでガード節が書ける(判別可能union)", () => {
    const errors = errorsOf(`struct User { name: string }
type Resp = { kind: "ok", user: User } | { kind: "notFound" }
fn describe(res: Resp) string {
	if res is { kind: "notFound" } {
		return "404"
	}
	return "found: \${res.user.name}"
}
fn main() { print(describe(Resp{kind: "notFound"})) }`);
    expect(errors).toEqual([]);
  });

  test("is: unionに無い型は can never be エラー", () => {
    expect(inMain(`ch := chan<int>(none)\nv := <-ch\nprint(v is string)`)).toEqual([
      expect.stringContaining("can never be string"),
    ]);
  });

  test("close(): チャネル以外を渡すとエラー", () => {
    expect(inMain(`close(1)`)).toEqual([expect.stringContaining("close() requires a channel, got int")]);
  });

  test("chan<T>(n): 容量は int でなければならない", () => {
    expect(inMain(`ch := chan<int>("x")`)).toEqual([
      expect.stringContaining("channel capacity must be int"),
    ]);
    expect(inMain(`ch := chan<int>(3)\nprint(ch)`)).toEqual([]);
  });

  test("select: 型は各アームの union、チャネル以外を渡すとエラー", () => {
    const errors = errorsOf(`fn main() {
	a := chan<int>(none)
	b := chan<string>(none)
	x := select {
		v := <-a => str(v)
		v := <-b => v
	}
	print(x)
}`);
    expect(errors).toEqual([]);

    expect(inMain(`x := select {\nv := <-1 => v\n}`)).toEqual([
      expect.stringContaining("select arm requires a channel, got int"),
    ]);
  });

  test("select: アーム内で束縛した変数は T | closed として絞り込める", () => {
    const errors = errorsOf(`fn main() {
	a := chan<int>(none)
	total := select {
		v := <-a => match v {
			closed => 0
			int => v + 1
		}
	}
	print(total)
}`);
    expect(errors).toEqual([]);
  });

  describe("ジェネリクス(F-1後半)", () => {
    test("引数の型からTを推論できる(配列+関数値の2箇所とも一致)", () => {
      const errors = errorsOf(`fn first<T>(arr: T[], pred: fn(T) bool) T | none {
	for _, v := range arr {
		if pred(v) { return v }
	}
	return none
}
fn main() {
	nums := [1, 2, 3]
	r := first(nums, fn(n: int) bool { return n > 1 })
	if r is none { return }
	print(r + 1)
}`);
      expect(errors).toEqual([]);
    });

    test("複数の型パラメータ(map<K, V>)を同時に推論できる", () => {
      const errors = errorsOf(`fn mapKeys<K, V>(m: map<K, V>) K[] {
	return keys(m)
}
fn main() {
	m := map<string, int>{"a": 1}
	print(mapKeys(m))
}`);
      expect(errors).toEqual([]);
    });

    test("struct型でもTを推論でき、絞り込み後にフィールドアクセスできる", () => {
      const errors = errorsOf(`struct User { name: string  age: int }
fn first<T>(arr: T[], pred: fn(T) bool) T | none {
	for _, v := range arr {
		if pred(v) { return v }
	}
	return none
}
fn main() {
	users := [User{name: "a", age: 1}, User{name: "b", age: 2}]
	u := first(users, fn(u: User) bool { return u.age > 1 })
	if u is none { return }
	print(u.name)
}`);
      expect(errors).toEqual([]);
    });

    test("退行防止: Tがunionの中(T | error)にしか現れなくても推論できる(critique文書のretry例)", () => {
      const errors = errorsOf(`fn retry<T>(f: fn() T | error, tries: int) T | error {
	mut i := 1
	for i <= tries {
		v := f()
		if v is error { i = i + 1; continue }
		return v
	}
	return error("gave up")
}
fn main() {
	n := retry(fn() int | error { return 42 }, 3)
	if n is error { return }
	print(n + 1)
}`);
      expect(errors).toEqual([]);
    });

    test("退行防止: Tがunion(T | none)の中にあり、素の値(非union)を渡しても推論できる", () => {
      const errors = errorsOf(`fn firstNonNone<T>(a: T | none) T | none { return a }
fn main() {
	x := firstNonNone(5)
	if x is none { return }
	print(x + 1)
}`);
      expect(errors).toEqual([]);
    });

    test("T | none に none だけを渡すと、他に手がかりが無いので推論失敗のまま(正しい挙動)", () => {
      const errors = errorsOf(`fn firstNonNone<T>(a: T | none) T | none { return a }
fn main() { print(firstNonNone(none)) }`);
      expect(errors).toEqual([expect.stringContaining("cannot infer type parameter(s) 'T'")]);
    });

    test("同じTが2引数に出てくる場合、食い違いを通常の代入不可エラーとして報告する", () => {
      const errors = errorsOf(`fn pair<T>(a: T, b: T) T { return a }
fn main() { print(pair(1, "x")) }`);
      expect(errors).toEqual([expect.stringContaining(`argument 2: cannot use "x" as int`)]);
    });

    test("型パラメータが戻り値型にしか現れないと宣言時にエラー(呼び出し側から推論できないため)", () => {
      const errors = errorsOf(`fn zero<T>() T { return 0 }\nfn main() { print(zero()) }`);
      expect(errors).toEqual([
        expect.stringContaining("type parameter 'T' must appear in a parameter type"),
        expect.stringContaining("cannot return int as T"),
        expect.stringContaining("cannot infer type parameter(s) 'T'"),
      ]);
    });

    test("型パラメータ名が組み込み型と衝突するとエラー", () => {
      const errors = errorsOf(`fn f<int>(x: int) int { return x }\nfn main() { print(f(1)) }`);
      expect(errors).toEqual(
        expect.arrayContaining([expect.stringContaining("shadows a builtin type name")]),
      );
    });

    test("同じ型パラメータ名を2回宣言するとエラー", () => {
      const errors = errorsOf(`fn f<T, T>(a: T, b: T) T { return a }\nfn main() { print(f(1, 2)) }`);
      expect(errors).toEqual(
        expect.arrayContaining([expect.stringContaining("'T' is declared more than once")]),
      );
    });

    test("型パラメータは抽象型として扱われ、+のような演算はできない(パラメトリシティ)", () => {
      const errors = errorsOf(`fn addOne<T>(x: T) T { return x + 1 }\nfn main() { print(addOne(1)) }`);
      expect(errors).toEqual([expect.stringContaining("invalid operation: T + int")]);
    });

    test("ジェネリック関数を変数へ代入してから呼ぶのは非対応(Tのまま残りエラーになる)", () => {
      const errors = errorsOf(`fn first<T>(arr: T[], pred: fn(T) bool) T | none {
	for _, v := range arr { if pred(v) { return v } }
	return none
}
fn main() {
	f := first
	nums := [1, 2, 3]
	r := f(nums, fn(n: int) bool { return n > 1 })
	print(r)
}`);
      expect(errors.length).toBeGreaterThan(0);
    });
  });

  describe("defer文", () => {
    test("関数呼び出しのdeferはエラーなし(素の関数・メソッド・組み込み)", () => {
      expect(inMain(`defer print("bye")`)).toEqual([]);
      const errors = errorsOf(`struct Resource { name: string }
fn (r: Resource) release() { print(r.name) }
fn main() {
	r := Resource{name: "a"}
	defer r.release()
	ch := chan<int>(1)
	defer close(ch)
}`);
      expect(errors).toEqual([]);
    });

    test("退行防止: 呼び出し以外をdeferすると'defer-requires-call'で拒否される", () => {
      expect(inMain(`defer 1 + 1`)).toEqual([
        expect.stringContaining("'defer' must be followed by a function or method call"),
      ]);
      expect(inMain(`x := 1\ndefer x`)).toEqual([
        expect.stringContaining("'defer' must be followed by a function or method call"),
      ]);
    });

    test("deferした呼び出し自体は通常の呼び出しと同じ型検査を受ける(引数の型不一致等)", () => {
      const errors = errorsOf(`fn needsInt(n: int) { print(n) }
fn main() {
	mut s := "not an int"
	defer needsInt(s)
}`);
      expect(errors).toEqual([expect.stringContaining("argument 1: cannot use string as int")]);
    });
  });

  describe("診断コード(F-13): code/fixの付与", () => {
    test("代表的な診断にそれぞれ意味のあるcodeが付く", () => {
      const cases: { src: string; code: DiagnosticCode }[] = [
        { src: `x := notDefined`, code: "undefined-name" },
        { src: `x := 1\nx := 2`, code: "already-declared" },
        { src: `x := 1\nx = 2`, code: "immutable-assignment" },
        { src: `x := "a" + 1`, code: "invalid-operation" },
        { src: `x := 1\nif x == none { }`, code: "use-is-none" },
      ];
      for (const { src, code } of cases) {
        const diags = diagnosticsOf(`fn main() {\n${src}\n}`);
        expect(diags.length).toBeGreaterThan(0);
        expect(diags[0].code).toBe(code);
      }

      // 複数関数にまたがる/型宣言が絡むケースは fn main() で包めないので個別プログラムで確認
      expect(
        diagnosticsOf(`fn f(a: int) int { return a }\nfn main() { print(f(1, 2)) }`)[0].code,
      ).toBe("argument-count");
      expect(
        diagnosticsOf(`struct User { name: string }\nfn main() { u := User{name: "a"}\nprint(u.age) }`)[0]
          .code,
      ).toBe("unknown-field");
    });

    test("'== none' のfixは '==' を 'is' に置き換える単一range置換になる", () => {
      const diags = diagnosticsOf(`fn main() {\n\tx: int | none = 1\n\tif x == none {\n\t}\n}`);
      expect(diags[0].code).toBe("use-is-none");
      // 3行目 "\tif x == none {" — タブも1桁として数えるので '==' は7〜8桁目
      expect(diags[0].fix).toEqual({
        description: "replace '==' with 'is'",
        range: { start: { line: 3, col: 7 }, end: { line: 3, col: 9 } },
        replacement: "is",
      });
    });

    test("'!= none' や 'none == x' はトークン置換で表現できないのでfix無し(codeは付く)", () => {
      const d1 = diagnosticsOf(`fn main() {\n\tx: int | none = 1\n\tif x != none {\n\t}\n}`);
      expect(d1[0].code).toBe("use-is-none");
      expect(d1[0].fix).toBeUndefined();

      const d2 = diagnosticsOf(`fn main() {\n\tx: int | none = 1\n\tif none == x {\n\t}\n}`);
      expect(d2[0].code).toBe("use-is-none");
      expect(d2[0].fix).toBeUndefined();
    });

    test("DIAGNOSTIC_EXPLANATIONS はcheckerが実際に出す全codeを説明できる(型で保証されるが実測でも確認)", () => {
      // 代表的な失敗パターンを一通り集めて発火させ、出てきたcodeが全部説明表にあることを確認する
      const programs = [
        `fn main() {\n\tx := notDefined\n\tprint(x)\n}`,
        `fn main() {\n\tx := 1\n\tx := 2\n\tprint(x)\n}`,
        `error type X = int\nfn main() { print(1) }`,
        `fn f<T>() T { return 1 }\nfn main() { print(f()) }`,
      ];
      for (const program of programs) {
        for (const d of diagnosticsOf(program)) {
          expect(Object.hasOwn(DIAGNOSTIC_EXPLANATIONS, d.code)).toBe(true);
        }
      }
    });
  });

  describe("H-1: any型の撤去(2026-07-21決定)", () => {
    test("退行防止: 'x: any'と書くとany-type-removedで拒否される(TSのas anyと同じ穴だった)", () => {
      expect(inMain(`x: any = 5\nprint(x)`)).toEqual([
        expect.stringContaining("'any' is not a type in Mesh"),
      ]);
    });

    test("退行防止: 撤去前は型不整合な演算がany経由で素通りしていた(このテストはエラーになることを確認する)", () => {
      // レビューで実測した穴そのもの: x: any = 5 の後、x + "文字列" が無検査で通っていた
      const errors = errorsOf(`fn main() {\n\tx: any = 5\n\ty := x + "no longer silent"\n\tprint(y)\n}`);
      expect(errors.some((e) => e.includes("'any' is not a type in Mesh"))).toBe(true);
    });

    test("退行防止: 文脈の無い空配列/mapリテラル(mut x := [])はcannot-infer-typeで拒否される", () => {
      // レビューで実測したもう1つの穴: mut arr := [] は any[] になり、push(arr, 1)と
      // push(arr, "混在")のどちらも検査を素通りしていた
      expect(inMain(`mut arr := []\nprint(arr)`)).toEqual([
        expect.stringContaining("cannot infer a complete type for 'arr'"),
      ]);
      expect(inMain(`x := []\nprint(x)`)).toEqual([
        expect.stringContaining("cannot infer a complete type for 'x'"),
      ]);
    });

    test("退行防止: トップレベル定数の文脈無し空配列リテラルも同様に拒否される(F-9c)", () => {
      const errors = errorsOf(`xs := []\nfn main() { print(xs) }`);
      expect(errors).toEqual([expect.stringContaining("cannot infer a complete type for 'xs'")]);
    });

    test("二重報告防止: 値の評価自体が既にエラーなら、cannot-infer-typeは重ねて出さない", () => {
      // undefined変数からの:=は、undefined-nameだけが出て、cannot-infer-typeは便乗しない
      expect(inMain(`x := notDefined\nprint(x)`)).toEqual([
        expect.stringContaining("undefined: 'notDefined'"),
      ]);
    });

    test("型注釈がある場合・関数引数・戻り値・structフィールドとしての空配列は今まで通り動く", () => {
      // any撤去後もassignable()の「配列要素がanyなら互換」ルールは残しているので、
      // 「既知の型と照合するだけ」の文脈では空配列リテラルはそのまま使える
      const errors = errorsOf(`struct Todo { title: string }
struct Container { items: Todo[] }
fn count(ts: Todo[]) int { return len(ts) }
fn makeEmpty() Todo[] { return [] }
xs: int[] = []
fn main() {
	ys: Todo[] = []
	print(len(xs), len(ys))
	print(count([]))
	print(len(makeEmpty()))
	c := Container{items: []}
	print(len(c.items))
}`);
      expect(errors).toEqual([]);
    });

    test("'any'という名前のtype/struct宣言は今まで通り予約名として拒否される", () => {
      expect(errorsOf(`type any = string\nfn main() {}`)).toEqual([
        expect.stringContaining("'any' is a builtin type and cannot be redeclared"),
      ]);
      expect(errorsOf(`struct any { x: int }\nfn main() {}`)).toEqual([
        expect.stringContaining("'any' is a builtin type and cannot be redeclared"),
      ]);
    });
  });

  describe("H-2: json struct(検証つきJSONデコードの自動生成)", () => {
    test("フラットなjson structは型検査を通り、decode<Name>が呼び出せる", () => {
      const errors = jsonErrorsOf(`import "mesh/json"
json struct User {
	name: string
	age: int
}
fn main() {
	v := json.parse("{}") or _ => json.Value{kind: "null"}
	u := decodeUser(v)
	if u is error { return }
	print(u.name, u.age)
}`);
      expect(errors).toEqual([]);
    });

    test("ネスト・配列・optionalの組み合わせも型検査を通る", () => {
      const errors = jsonErrorsOf(`import "mesh/json"
json struct Address { city: string }
json struct Person {
	name: string
	address: Address
	tags: string[]
	nickname: string | none
}
fn main() {
	v := json.parse("{}") or _ => json.Value{kind: "null"}
	p := decodePerson(v)
	if p is error { return }
	print(p.name, p.address.city, p.tags, p.nickname)
}`);
      expect(errors).toEqual([]);
    });

    test("退行防止: サポート外のフィールド型(素のstruct・map)は合成時にjson-struct-unsupported-fieldで拒否される", () => {
      expect(
        jsonErrorsOf(`import "mesh/json"
struct Address { city: string }
json struct Person {
	name: string
	address: Address
}
fn main() {}`),
      ).toEqual([expect.stringContaining("can't auto-decode field 'address'")]);

      expect(
        jsonErrorsOf(`import "mesh/json"
json struct Config {
	settings: map<string, string>
}
fn main() {}`),
      ).toEqual([expect.stringContaining("can't auto-decode field 'settings'")]);
    });

    test("退行防止: import \"mesh/json\"が無いとjson-struct-missing-importで拒否される", () => {
      expect(
        jsonErrorsOf(`json struct User {
	name: string
}
fn main() {}`),
      ).toEqual([expect.stringContaining("needs 'import \"mesh/json\"'")]);
    });

    test("退行防止: 'json type'はunion向けの自動デコードが複雑すぎるため明示的に拒否される", () => {
      expect(
        jsonErrorsOf(`json type Status = "active" | "banned"
fn main() {}`),
      ).toEqual([expect.stringContaining("'json type' isn't supported")]);
    });

    test("export json structは、生成されたdecode<Name>もexportされる(他パッケージから呼べる)", () => {
      const result = compileModules([
        {
          pkg: "main",
          file: "app.mesh",
          source: `import "mesh/json"\nimport "models"\nfn main() {\n\tv := json.parse("{}") or _ => json.Value{kind: "null"}\n\tu := models.decodeUser(v)\n\tif u is error { return }\n\tprint(u.name)\n}`,
        },
        {
          pkg: "models",
          file: "models/models.mesh",
          source: `import "mesh/json"\nexport json struct User {\n\tname: string\n}`,
        },
      ]);
      expect(result.diagnostics).toEqual([]);
    });
  });

  describe("C-6続き: mesh/http(検証つきサーバーAPI)", () => {
    test("正しい使い方は型検査を通る", () => {
      expect(
        errorsOf(`import "mesh/http"
fn handler(req: http.Request) http.Response {
	return http.Response{status: 200, body: req.path, headers: map<string, string>{}}
}
fn main() {
	r := http.listen(":8080", handler)
	if r is error { return }
}`),
      ).toEqual([]);
    });

    test("handlerの型が合わないとエラーになる(引数がRequestでない)", () => {
      const errors = errorsOf(`import "mesh/http"
fn badHandler(req: string) http.Response {
	return http.Response{status: 200, body: req, headers: map<string, string>{}}
}
fn main() {
	http.listen(":8080", badHandler)
}`);
      expect(errors.length).toBeGreaterThan(0);
    });

    test("http.Response{...}のフィールド不足はmissing-fieldsになる", () => {
      expect(
        errorsOf(`import "mesh/http"
fn main() {
	r := http.Response{status: 200, body: "hi"}
}`),
      ).toEqual([expect.stringContaining("missing field(s)")]);
    });

    test("importせずにhttp.Request/http.listenを使うとundefined-nameになる", () => {
      const errors = errorsOf(`fn handler(req: http.Request) http.Response {
	return http.Response{status: 200, body: "hi", headers: map<string, string>{}}
}
fn main() {
	http.listen(":8080", handler)
}`);
      expect(errors.length).toBeGreaterThan(0);
    });

    test("req.headers[key]はstring | noneなのでnarrowingせずに使うとエラーになる", () => {
      const errors = errorsOf(`import "mesh/http"
fn handler(req: http.Request) http.Response {
	v := req.headers["x-id"]
	return http.Response{status: 200, body: v, headers: map<string, string>{}}
}
fn main() {}`);
      expect(errors.length).toBeGreaterThan(0);
    });

    test("is noneで絞り込めばreq.headers[key]をそのままbodyに使える", () => {
      expect(
        errorsOf(`import "mesh/http"
fn handler(req: http.Request) http.Response {
	v := req.headers["x-id"]
	if v is none {
		return http.Response{status: 400, body: "missing", headers: map<string, string>{}}
	}
	return http.Response{status: 200, body: v, headers: map<string, string>{}}
}
fn main() {}`),
      ).toEqual([]);
    });
  });
});
