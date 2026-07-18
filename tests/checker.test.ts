import { describe, expect, test } from "bun:test";
import { check } from "../src/checker";
import { parse } from "../src/parser";
import { CompileError } from "../src/token";

const errorsOf = (src: string) => check(parse(src)).map((d) => d.message);
const inMain = (body: string) => errorsOf(`fn main() {\n${body}\n}`);

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
      expect.stringContaining("cannot access field on User | none — narrow it first"),
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

  test("型付き配列リテラル: 空配列に型が付く", () => {
    // 空の [] は any[] だが、Todo[]{} は Todo[] になる
    const errors = errorsOf(`struct Todo {
	id: int
}
fn make() Todo[] {
	return Todo[]{}
}
fn main() {
	todos := Todo[]{}
	push(todos, Todo{id: 1})
	print(len(todos) + len(make()))
}`);
    expect(errors).toEqual([]);
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

  test("== none は 'is none' へ誘導する", () => {
    const errors = errorsOf(`fn f() int | none { return none }\nfn main() { x := f()\nif x == none {\nreturn\n}\nprint(x) }`);
    expect(errors).toEqual([expect.stringContaining("use 'is none'")]);
  });

  test("'!' は戻り値型に失敗メンバーが無いと使えない", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn g() int {
	return f()!
}
fn main() { print(g()) }`);
    expect(errors).toEqual([expect.stringContaining("'!' propagates error")]);
  });

  test("'!' は失敗メンバーを含む関数内なら使える", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }
fn g() int | error {
	v := f()!
	return v * 2
}
fn main() { print(g()) }`);
    expect(errors).toEqual([]);
  });

  test("'or' のフォールバックは成功型に合わせる", () => {
    const errors = errorsOf(`fn f() int | error { return 1 }\nfn main() { x := f() or "zero"\nprint(x) }`);
    expect(errors).toEqual([expect.stringContaining("'or' fallback must be int")]);
  });

  test("'or' は失敗しない型には使えない", () => {
    expect(inMain(`x := 1 or 2\nprint(x)`)).toEqual([
      expect.stringContaining("left side of 'or' never fails"),
    ]);
  });

  test("if の条件は bool でなければならない", () => {
    expect(inMain(`if 1 {\n}`)).toEqual([expect.stringContaining("must be bool")]);
  });

  test("チャネルの要素型の不一致を検出", () => {
    expect(inMain(`ch := chan<int>()\nch <- "hi"`)).toEqual([
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
    expect(inMain(`n := toInt("42") or 0\nprint(n + 1)`)).toEqual([]);
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

  test("transform: 戻り値の型が変わってよい(int[] → string[])", () => {
    expect(
      inMain(`nums := [1, 2, 3]\nlabels := transform(nums, fn(n: int) string { return str(n) })\nprint(labels[0])`),
    ).toEqual([]);
    expect(inMain(`nums := [1, 2, 3]\nprint(transform(nums, fn(s: string) int { return 1 }))`)).toEqual([
      expect.stringContaining("transform() callback must take a single int parameter"),
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

  test("map(...) は 'map' 型キーワードと衝突して構文エラーになる(transformを使う)", () => {
    expect(() => errorsOf(`nums := [1, 2, 3]\nprint(map(nums, fn(n: int) int { return n }))`)).toThrow(
      CompileError,
    );
  });
});
