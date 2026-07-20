// E2E テスト: .mesh をコンパイル → 生成された JS を実行 → 標準出力を照合
import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { compile, compileModules, type ModuleSource } from "../src/compiler";

function runSource(source: string): string {
  const result = compile(source);
  if (result.code === null) {
    throw new Error("compile failed:\n" + result.diagnostics.map((d) => d.message).join("\n"));
  }
  const dir = mkdtempSync(join(tmpdir(), "mesh-test-"));
  const path = join(dir, "out.mjs");
  writeFileSync(path, result.code);
  const proc = spawnSync(process.execPath, [path], { encoding: "utf8", timeout: 10_000 });
  if (proc.status !== 0) {
    throw new Error(`program exited with ${proc.status}:\n${proc.stderr}`);
  }
  return proc.stdout;
}

const runExample = (name: string) =>
  runSource(readFileSync(join(import.meta.dir, "..", "examples", name), "utf8"));

// 複数モジュール(パッケージ)をコンパイルして実行する
function runModules(modules: ModuleSource[]): string {
  const result = compileModules(modules);
  if (result.code === null) {
    throw new Error("compile failed:\n" + result.diagnostics.map((d) => d.message).join("\n"));
  }
  const dir = mkdtempSync(join(tmpdir(), "mesh-test-"));
  const path = join(dir, "out.mjs");
  writeFileSync(path, result.code);
  const proc = spawnSync(process.execPath, [path], { encoding: "utf8", timeout: 10_000 });
  if (proc.status !== 0) {
    throw new Error(`program exited with ${proc.status}:\n${proc.stderr}`);
  }
  return proc.stdout;
}

// panic で異常終了することを期待するヘルパ。stderr を返す
function runSourceExpectPanic(source: string): string {
  const result = compile(source);
  if (result.code === null) {
    throw new Error("compile failed:\n" + result.diagnostics.map((d) => d.message).join("\n"));
  }
  const dir = mkdtempSync(join(tmpdir(), "mesh-test-"));
  const path = join(dir, "out.mjs");
  writeFileSync(path, result.code);
  const proc = spawnSync(process.execPath, [path], { encoding: "utf8", timeout: 10_000 });
  if (proc.status === 0) {
    throw new Error(`expected panic, but program exited 0. stdout:\n${proc.stdout}`);
  }
  return proc.stderr;
}

describe("mesh check --json", () => {
  const CLI = join(import.meta.dir, "..", "src", "cli.ts");

  function checkJson(source: string): { exit: number | null; parsed: any } {
    const dir = mkdtempSync(join(tmpdir(), "mesh-json-"));
    const path = join(dir, "prog.mesh");
    writeFileSync(path, source);
    const proc = spawnSync(process.execPath, [CLI, "check", path, "--json"], {
      encoding: "utf8",
      timeout: 10_000,
    });
    return { exit: proc.status, parsed: JSON.parse(proc.stdout) };
  }

  test("エラーなし: ok=true / exit 0", () => {
    const { exit, parsed } = checkJson(`fn main() { print("hi") }`);
    expect(exit).toBe(0);
    expect(parsed.ok).toBe(true);
    expect(parsed.diagnostics).toEqual([]);
  });

  test("エラーあり: 位置つき診断の配列 / exit 1", () => {
    const { exit, parsed } = checkJson(`fn main() {\n\tprint(nothing)\n\tx := "a" + 1\n}`);
    expect(exit).toBe(1);
    expect(parsed.ok).toBe(false);
    expect(parsed.diagnostics.length).toBe(2);
    expect(parsed.diagnostics[0]).toMatchObject({
      line: 2,
      severity: "error",
      code: "undefined-name",
      message: expect.stringContaining("undefined: 'nothing'"),
    });
    expect(parsed.diagnostics[0].col).toBeGreaterThan(0);
    expect(parsed.diagnostics[0].file).toContain("prog.mesh");
  });

  test("構文エラーもJSONで返る", () => {
    const { exit, parsed } = checkJson(`fn main( {`);
    expect(exit).toBe(1);
    expect(parsed.ok).toBe(false);
    expect(parsed.diagnostics.length).toBe(1);
    expect(parsed.diagnostics[0].code).toBe("syntax-error");
  });

  test("F-13: 機械適用可能な診断にはfixパッチが付く(code+fix)", () => {
    const { parsed } = checkJson(`fn main() {\n\tx: int | none = 1\n\tif x == none {\n\t}\n}`);
    expect(parsed.diagnostics[0].code).toBe("use-is-none");
    expect(parsed.diagnostics[0].fix).toEqual({
      description: "replace '==' with 'is'",
      range: { start: { line: 3, col: 7 }, end: { line: 3, col: 9 } },
      replacement: "is",
    });
  });

  test("F-13: fixを持たない診断は fix フィールド自体が無い(undefinedを送らない)", () => {
    const { parsed } = checkJson(`fn main() {\n\tprint(nothing)\n}`);
    expect("fix" in parsed.diagnostics[0]).toBe(false);
  });
});

describe("mesh explain <code>(F-13)", () => {
  const CLI = join(import.meta.dir, "..", "src", "cli.ts");

  test("既知のcodeは説明文を1行以上返す(exit 0)", () => {
    const proc = spawnSync(process.execPath, [CLI, "explain", "use-is-none"], { encoding: "utf8" });
    expect(proc.status).toBe(0);
    expect(proc.stdout).toContain("is none");
  });

  test("未知のcodeはエラーで案内する(exit 1)", () => {
    const proc = spawnSync(process.execPath, [CLI, "explain", "not-a-real-code"], { encoding: "utf8" });
    expect(proc.status).toBe(1);
    expect(proc.stderr).toContain("unknown diagnostic code");
  });

  test("引数無しは全コード一覧を返す(mesh check --jsonが返すcodeは必ず載っている)", () => {
    const proc = spawnSync(process.execPath, [CLI, "explain"], { encoding: "utf8" });
    expect(proc.status).toBe(0);
    expect(proc.stdout).toContain("use-is-none");
    expect(proc.stdout).toContain("undefined-name");
  });
});

describe("言語カード(mesh card)", () => {
  const CLI = join(import.meta.dir, "..", "src", "cli.ts");

  test("mesh card がカードを出力する", () => {
    const proc = spawnSync(process.execPath, [CLI, "card"], { encoding: "utf8", timeout: 10_000 });
    expect(proc.status).toBe(0);
    expect(proc.stdout).toContain("# Mesh Language Card");
    expect(proc.stdout).toContain("Does NOT exist in Mesh");
    expect(proc.stdout).toContain("mesh check file.mesh --json");
  });

  test("mesh card --for <file>(F-13後半): 使っている機能だけに絞った縮小版を返す", () => {
    const dir = mkdtempSync(join(tmpdir(), "mesh-card-for-"));
    const path = join(dir, "simple.mesh");
    writeFileSync(path, `fn main() {\n\tx := 1\n\tprint(x + 2)\n}`);

    const proc = spawnSync(process.execPath, [CLI, "card", "--for", path], {
      encoding: "utf8",
      timeout: 10_000,
    });
    const full = spawnSync(process.execPath, [CLI, "card"], { encoding: "utf8", timeout: 10_000 });
    expect(proc.status).toBe(0);
    expect(proc.stdout).toContain("PROJECT-SCOPED SUBSET");
    expect(proc.stdout).toContain("## Program structure");
    expect(proc.stdout).not.toContain("## Concurrency");
    expect(proc.stdout.length).toBeLessThan(full.stdout.length * 0.6);
  });

  test("mesh card --for に引数無しはエラー(exit 1)", () => {
    const proc = spawnSync(process.execPath, [CLI, "card", "--for"], { encoding: "utf8", timeout: 10_000 });
    expect(proc.status).toBe(1);
    expect(proc.stderr).toContain("usage: mesh card --for");
  });

  test("カードの肯定的な主張がすべてコンパイル・実行できる", () => {
    const out = runSource(`struct User {
	name: string
	age: int
}
type Status = "active" | "banned"

fn find(id: int) User | none {
	if id == 1 {
		return User{name: "a", age: 1}
	}
	return none
}

fn parse(s: string) int | error {
	if s == "1" {
		return 1
	}
	return error("bad input: \${s}")
}

fn doubled(s: string) int | error {
	v := parse(s)?
	return v * 2
}

fn label(s: Status) string {
	return match s {
		"active" => "OK"
		"banned" => "NG"
	}
}

fn work(n: int) int {
	sleep(5)
	return n
}

fn main() {
	v := find(1)
	if v is none {
		return
	}
	print(v.name)
	v.age = 31

	x := parse("z") or _ => 0
	print(x)

	d := doubled("1")
	if d is error {
		return
	}
	print(d)

	print(label("active"))

	m := map<string, int>{"a": 1}
	m["b"] = 2
	delete(m, "b")
	mv := m["a"]
	if mv is none {
		return
	}
	print(mv + len(m))

	nums := [1, 2, 3]
	push(nums, 4)
	mut total := 0
	for _, n := range nums {
		total = total + n
	}
	for i := range 2 {
		total = total + i
	}
	print(total)

	task := spawn work(5)
	print(<-task)
	wait {
		spawn work(1)
	}
	print("done \${str(true)}")
}`);
    expect(out).toBe("a\n0\n2\nOK\n2\n11\n5\ndone true\n");
  });

  test("カードの『存在しない』リストが実際にエラーになる", () => {
    const fails = (src: string) => compile(src).code === null;
    expect(fails(`fn main() { x := null }`)).toBe(true); // null は存在しない
    expect(fails(`fn main() { try { } }`)).toBe(true); // try/catch は存在しない
    expect(fails(`fn f() (int, error) { return 1 }\nfn main() {}`)).toBe(true); // 多値戻り
    expect(fails(`fn main() { m := map<string, int>{}\nv, ok := m["a"]\nprint(v, ok) }`)).toBe(true); // comma-ok
    expect(fails(`fn main() { while true { } }`)).toBe(true); // while は存在しない
  });

  test("カードの主張: map の反復は挿入順で決定的", () => {
    expect(
      runSource(`fn main() {\n\tm := map<string, int>{}\n\tm["z"] = 1\n\tm["a"] = 2\n\tm["m"] = 3\n\tfor k, v := range m {\n\t\tprint("\${k}=\${v}")\n\t}\n}`),
    ).toBe("z=1\na=2\nm=3\n");
  });

  test("カードの主張: 配列は参照渡し / 前置! / mut widening / print挙動", () => {
    // 配列を関数に渡して push すると呼び出し元に反映される(参照値)
    expect(
      runSource(`fn addOne(xs: int[]) {\n\tpush(xs, 99)\n}\nfn main() {\n\txs := [1]\n\taddOne(xs)\n\tprint(len(xs))\n}`),
    ).toBe("2\n");
    // 前置 ! (否定)
    expect(runSource(`fn main() {\n\tdone := false\n\tif !done {\n\t\tprint("not done")\n\t}\n}`)).toBe(
      "not done\n",
    );
    // mut s := リテラルは string に widening され再代入できる
    expect(runSource(`fn main() {\n\tmut s := "a"\n\ts = "b"\n\tprint(s)\n}`)).toBe("b\n");
    // print は複数引数をスペース区切り + 末尾改行
    expect(runSource(`fn main() {\n\tprint("a")\n\tprint("b", "c")\n}`)).toBe("a\nb c\n");
  });

  test("カードの新項目: 空配列 xs: T[] = [] / pushはnone / errメッセージ補間", () => {
    const out = runSource(`struct Item {
      name: string
    }
    fn parse(s: string) int | error {
      return error("bad: \${s}")
    }
    fn main() {
      items: Item[] = []          // 空の型付き配列
      push(items, Item{name: "x"})
      print(len(items))

      e := parse("z")
      if e is error {
        print("failed: \${e}")   // error のメッセージが補間される
      }
    }`);
    expect(out).toBe("1\nfailed: bad: z\n");
    // 「push を値として使う」はカードどおりエラーになる
    expect(compile(`fn main() { xs := [1]\ny := push(xs, 2)\nprint(y) }`).code).toBe(null);
  });

  test("カードの新項目: 標準ライブラリ第一弾(contains/indexOf/keys/values/sort)", () => {
    const out = runSource(`fn main() {
      nums := [3, 1, 2]
      print(contains(nums, 1))

      i := indexOf(nums, 9)
      if i is none {
        print("missing")
      }

      ages := map<string, int>{"b": 2, "a": 1}
      print(keys(ages))
      print(values(ages))

      print(sort(nums))
      print(nums)   // sort は非破壊 — 元の配列は変わらない
    }`);
    expect(out).toBe("true\nmissing\n[b a]\n[2 1]\n[1 2 3]\n[3 1 2]\n");
  });

  test("カードの新項目: 標準ライブラリ第二弾(split/join/trim/upper/lower/toInt)", () => {
    const out = runSource(`fn parseAge(s: string) int | error {
      return toInt(s)?   // toInt() DOES fail — ? で呼び出し元へ伝播できる
    }
    fn main() {
      csv := "  Alice, Bob ,Carol  "
      names := split(csv, ",")
      mut cleaned: string[] = []
      for _, n := range names {
        push(cleaned, upper(trim(n)))
      }
      print(join(cleaned, " | "))

      age := parseAge("30")
      if age is error {
        return
      }
      print(age)
    }`);
    expect(out).toBe("ALICE | BOB | CAROL\n30\n");
  });

  test("カードの新項目: 標準ライブラリ第三弾(filter/map/reduce)", () => {
    const out = runSource(`fn isEven(n: int) bool {
      return n % 2 == 0
    }
    fn main() {
      nums := [1, 2, 3, 4, 5, 6]
      evens := filter(nums, isEven)              // 名前付き関数を値として渡す
      doubled := map(evens, fn(n: int) int { return n * 2 })  // インラインクロージャ
      total := reduce(doubled, fn(acc: int, n: int) int { return acc + n }, 0)
      print(total)
    }`);
    expect(out).toBe("24\n");
  });

  test("カードの新項目: メソッド(Goスタイルのレシーバ構文)", () => {
    const out = runSource(`struct Todo {
      title: string
      done: bool
    }
    fn (t: Todo) complete() Todo {
      return Todo{title: t.title, done: true}
    }
    fn (t: Todo) render() string {
      if t.done { return "[x] " + t.title }
      return "[ ] " + t.title
    }
    fn main() {
      todos := [Todo{title: "a", done: false}]
      todos[0] = todos[0].complete()
      print(todos[0].render())
      print(Todo{title: "b", done: false}.complete().render())  // 連鎖(左から右へ読める)
    }`);
    expect(out).toBe("[x] a\n[x] b\n");
    // カードどおり、メソッド名は裸では呼べない(名前空間が分離している)
    expect(
      compile(`struct Todo { title: string }
fn (t: Todo) render() string { return t.title }
fn main() { t := Todo{title: "a"}\nprint(render(t)) }`).code,
    ).toBe(null);
  });

  test("カードの新項目: 2段スコープ(spawn=関数所有で暗黙wait / detach=プログラム所有)", () => {
    const out = runSource(`fn addTo(arr: int[], v: int) {
      sleep(30)
      push(arr, v)
    }
    fn structured(arr: int[]) {
      spawn addTo(arr, 1)   // 関数を抜けるとき暗黙に待たれる
    }
    fn background(arr: int[]) {
      detach addTo(arr, 2)  // プログラム所有 — この関数は待たずに戻る
    }
    fn main() {
      arr := [0]
      structured(arr)
      print(len(arr))       // 2: spawn は待たれた
      background(arr)
      print(len(arr))       // 2: detach はまだ完了していない
    }`);
    expect(out).toBe("2\n2\n");
  });

  test("カードの新項目: channel仕様の完成(close/T|closed/容量/select)", () => {
    const out = runSource(`fn produce(ch: chan<int>) {
      for i := 1; i <= 2; i++ {
        ch <- i
      }
      close(ch)
    }
    fn main() {
      ch := chan<int>(none)
      spawn produce(ch)
      mut total := 0
      for {
        v := <-ch
        if v is closed {
          break
        }
        total = total + v
      }
      print(total)

      a := chan<string>(none)
      spawn slowSend(a, "hi")
      r := select {
        v := <-a => v
        _ => "nothing"
      }
      print(r)
    }
    fn slowSend(ch: chan<string>, msg: string) {
      sleep(10)
      ch <- msg
    }`);
    expect(out).toBe("3\nnothing\n");
    // カードどおり、<-ch は絞り込まずに算術に使うとコンパイルエラーになる
    expect(
      compile(`fn main() { ch := chan<int>(none)\nv := <-ch\nprint(v + 1) }`).code,
    ).toBe(null);
  });

  test("カードの新項目: 判別可能union(タグ付きstruct形式)", () => {
    const out = runSource(`struct User { name: string }
    type GetUserResponse = { kind: "ok", user: User } | { kind: "notFound" } | { kind: "unauthorized" }
    fn getUser(id: string) GetUserResponse {
      if id == "1" { return GetUserResponse{ kind: "ok", user: User{name: "alice"} } }
      return GetUserResponse{ kind: "notFound" }
    }
    fn main() {
      print(match getUser("1") {
        { kind: "ok" } => "hi"
        _ => "?"
      })
      r := getUser("2")
      print(match r {
        { kind: "ok" } => "hi \${r.user.name}"
        { kind: "notFound" } => "not found"
        { kind: "unauthorized" } => "unauthorized"
      })
    }`);
    expect(out).toBe("hi\nnot found\n");
    // カードどおり、裸の { ... } は struct を使えと誘導される(union内でのみ有効)
    expect(compile(`type X = { a: int }\nfn main() {}`).diagnostics[0]?.message).toContain(
      "use 'struct X { ... }'",
    );
  });

  test("判別可能union: 自己参照(木構造)がstructフィールド越しなら再帰できる", () => {
    const out = runSource(`type Tree = { kind: "leaf", value: int } | { kind: "node", left: Tree, right: Tree }
    fn leaf(v: int) Tree { return Tree{kind: "leaf", value: v} }
    fn node(l: Tree, r: Tree) Tree { return Tree{kind: "node", left: l, right: r} }
    fn sumTree(t: Tree) int {
      return match t {
        { kind: "leaf" } => t.value
        { kind: "node" } => sumTree(t.left) + sumTree(t.right)
      }
    }
    fn main() {
      tree := node(node(leaf(1), leaf(2)), leaf(3))
      print(sumTree(tree))
    }`);
    expect(out).toBe("6\n");
  });

  test("判別可能union: 裸のunion同士の相互再帰は今まで通りcycleエラー(structに包まれない危険な形)", () => {
    // type A = B | none; type B = A | error — どちらもstruct等に包まれない裸の相互参照。
    // これを許すとflatten時に相手のplaceholderがまだ空で型情報が消えるため、引き続き弾く
    const result = compile(`type A = B | none\ntype B = A | error\nfn main() {}`);
    expect(result.diagnostics.length).toBe(1);
    expect(result.diagnostics[0]?.message).toContain("type alias cycle");
  });

  test("カードの新項目: is はmatchと同じパターンを受け付ける(型名・リテラル・部分構造)", () => {
    const out = runSource(`struct User { name: string }
    type Resp = { kind: "ok", user: User } | { kind: "notFound" }
    fn find(id: int) User | none | error {
      if id == 1 { return User{name: "alice"} }
      return none
    }
    fn describe(res: Resp) string {
      if res is { kind: "notFound" } {
        return "404"
      }
      return "found: \${res.user.name}"
    }
    type Status = "active" | "banned"
    fn label(s: Status) string {
      if s is "active" { return "OK" }
      return "NG"
    }
    fn main() {
      u := find(1)
      if u is User {
        print(u.name)
      }
      print(describe(Resp{kind: "notFound"}))
      print(describe(Resp{kind: "ok", user: User{name: "bob"}}))
      print(label("active"))
      print(label("banned"))
      print(find(2) is none)
    }`);
    expect(out).toBe("alice\n404\nfound: bob\nOK\nNG\ntrue\n");
  });

  test("カードの新項目: ? 伝播(旧!)・? \"文脈\"・or束縛形(Go式の明示性)", () => {
    const out = runSource(`fn parse(s: string) int | error {
      if s == "1" { return 1 }
      return error("bad input: \${s}")
    }
    fn find(id: int) int | none {
      if id == 1 { return 10 }
      return none
    }
    fn doubled(s: string) int | error {
      v := parse(s)?
      return v * 2
    }
    fn withCtx(s: string, line: int) int | error {
      return parse(s) ? "line \${line}: bad amount"
    }
    fn requireFound(id: int) int | error {
      return find(id) ? "user \${id} missing"    // none も文脈つき error に昇格
    }
    fn main() {
      d := doubled("1")
      if d is error { return }
      print(d)

      c := withCtx("x99", 4)
      if c is error {
        print("\${c}")
      }

      r := requireFound(9)
      if r is error {
        print("\${r}")
      }

      print(parse("z") or _ => 0)                  // 意図的に捨てる(痕跡が字面に残る)
      print(parse("z") or e => len("\${e}"))       // 失敗値を受け取って使う
      print(find(9) or 7)                          // none のみの union は素の or でOK
    }`);
    expect(out).toBe(
      "2\nline 4: bad amount: bad input: x99\nuser 9 missing\n0\n12\n7\n",
    );
    // 素の or で error を吸おうとすると誘導エラー
    expect(
      compile(`fn parse(s: string) int | error { return 1 }\nfn main() { x := parse("1") or 0\nprint(x) }`)
        .diagnostics[0]?.message,
    ).toContain("'or' would silently discard an error");
  });

  test("カードの新項目: 関数型注釈 fn(int) int — ユーザー定義の高階関数が書ける", () => {
    const out = runSource(`fn apply(f: fn(int) int, x: int) int {
      return f(x)
    }
    fn retryInt(f: fn() int | error, maxAttempts: int) int | error {
      for i := 1; i <= maxAttempts; i++ {
        v := f()
        if v is error { continue }
        return v
      }
      return error("gave up after \${maxAttempts}")
    }
    fn makeAdder(n: int) fn(int) int {
      return fn(x: int) int { return x + n }
    }
    struct Handler {
      onHit: fn(int) int
    }
    fn run(cb: (fn(int) int) | none, x: int) int {
      if cb is none { return x }
      return cb(x)
    }
    fn main() {
      double: fn(int) int = fn(x: int) int { return x * 2 }
      print(apply(double, 21))

      mut calls := 0
      flaky := fn() int | error {
        calls = calls + 1
        if calls < 3 { return error("boom") }
        return 99
      }
      print(retryInt(flaky, 5) or _ => 0)

      add10 := makeAdder(10)
      print(add10(5))

      h := Handler{onHit: double}
      print(h.onHit(50))

      print(run(none, 7))
      print(run(add10, 7))
    }`);
    expect(out).toBe("42\n99\n15\n100\n7\n17\n");
    // 引数の数が合わない関数値はコンパイルエラー
    expect(
      compile(
        `fn apply(f: fn(int) int, x: int) int { return f(x) }\nfn main() { print(apply(fn(a: int, b: int) int { return a + b }, 1)) }`,
      ).diagnostics[0]?.message,
    ).toContain("cannot use fn(int, int) int as fn(int) int");
  });

  test("カードの新項目: 自己参照する判別可能unionの回避策(名前付き再帰struct+T|noneの疑似optional)", () => {
    const out = runSource(`struct Expr {
        kind: string
        val: int | none
        left: Expr | none
        right: Expr | none
    }
    fn num(v: int) Expr { return Expr{kind: "num", val: v, left: none, right: none} }
    fn add(l: Expr, r: Expr) Expr { return Expr{kind: "add", val: none, left: l, right: r} }

    fn evalExpr(e: Expr) int {
        if e.kind == "num" {
            return e.val or 0
        }
        l := e.left
        if l is none { return 0 }
        r := e.right
        if r is none { return 0 }
        return evalExpr(l) + evalExpr(r)
    }

    fn main() {
        tree := add(num(2), add(num(3), num(4)))
        print(evalExpr(tree))
    }`);
    expect(out).toBe("9\n");
  });

  test("narrowing(F-6): is を || でつないだ複合条件もド・モルガンで絞り込まれる", () => {
    const out = runSource(`fn main() {
        l: int | none = 1
        r: int | none = 2
        if l is none || r is none {
            return
        }
        print(l + r)
    }`);
    expect(out).toBe("3\n");
  });

  test("narrowing(F-6): && は左のisが右辺とthen節に効く", () => {
    const out = runSource(`struct Circle { kind: string  r: float }
    struct Square { kind: string  s: float }
    type Shape = Circle | Square

    fn main() {
        c: Shape = Circle{kind: "circle", r: 2.0}
        if c is Circle && c.r > 1.0 {
            print("big circle")
        } else {
            print("small or not a circle")
        }
    }`);
    expect(out).toBe("big circle\n");
  });

  test("narrowing(F-6): ! はド・モルガンでnarrowingが効く", () => {
    const out = runSource(`fn main() {
        v: int | none = 5
        if !(v is none) {
            print(v + 1)
        }
    }`);
    expect(out).toBe("6\n");
  });

  test("narrowing(F-6): フィールドパス(n.next)を再代入せず直接narrowingできる", () => {
    const out = runSource(`struct Node { value: int  next: Node | none }
    fn sum(n: Node) int {
        total := n.value
        if n.next is none {
            return total
        }
        return total + sum(n.next)
    }
    fn main() {
        c := Node{value: 3, next: none}
        b := Node{value: 2, next: c}
        a := Node{value: 1, next: b}
        print(sum(a))
    }`);
    expect(out).toBe("6\n");
  });

  test("ジェネリクス(F-1後半): user-defined first/filter/contains run correctly at all instantiations", () => {
    const out = runSource(`struct User { name: string  age: int }

    fn first<T>(arr: T[], pred: fn(T) bool) T | none {
        for _, v := range arr {
            if pred(v) { return v }
        }
        return none
    }

    fn myFilter<T>(arr: T[], pred: fn(T) bool) T[] {
        out: T[] = []
        for _, v := range arr {
            if pred(v) { push(out, v) }
        }
        return out
    }

    fn myContains<T>(arr: T[], x: T) bool {
        for _, v := range arr {
            if v == x { return true }
        }
        return false
    }

    fn main() {
        nums := [1, 2, 3, 4, 5]
        r := first(nums, fn(n: int) bool { return n > 3 })
        if r is none { return }
        print(r)

        print(myFilter(nums, fn(n: int) bool { return n % 2 == 0 }))
        print(myContains(nums, 3))
        print(myContains(nums, 99))
        print(myContains(["alice", "bob"], "bob"))

        users := [User{name: "a", age: 1}, User{name: "b", age: 2}]
        adult := first(users, fn(u: User) bool { return u.age > 1 })
        if adult is none { return }
        print(adult.name)
    }`);
    expect(out).toBe("4\n[2 4]\ntrue\nfalse\ntrue\nb\n");
  });

  test("ジェネリクス(F-1後半): 別のジェネリック関数を呼ぶネストした推論も通る", () => {
    const out = runSource(`fn first<T>(arr: T[], pred: fn(T) bool) T | none {
        for _, v := range arr { if pred(v) { return v } }
        return none
    }
    fn firstPositive<T>(arr: T[], pred: fn(T) bool) T | none {
        return first(arr, pred)
    }
    fn main() {
        nums := [1, -2, 3]
        r := firstPositive(nums, fn(n: int) bool { return n > 0 })
        if r is none { return }
        print(r)
    }`);
    expect(out).toBe("1\n");
  });

  test("構造化エラー(F-2後半): error type の '?'/'or' 伝播が実際に正しい値を運ぶ", () => {
    const out = runSource(`error type DbError = { kind: "notFound", table: string } | { kind: "timeout", ms: int }

    fn find(id: int) int | DbError {
        if id == 1 { return 42 }
        if id == 2 { return DbError{kind: "timeout", ms: 100} }
        return DbError{kind: "notFound", table: "users"}
    }

    fn useIt(id: int) int | DbError {
        v := find(id)?
        return v + 1
    }

    fn main() {
        r1 := useIt(1)
        if r1 is int { print(r1) } else { print("unexpected error") }

        r2 := useIt(2)
        if r2 is { kind: "timeout" } {
            print("timeout after \${r2.ms}ms")
        } else {
            print(r2)
        }

        r3 := useIt(3)
        match r3 {
            { kind: "notFound" } => print("not found in \${r3.table}")
            { kind: "timeout" } => print("timeout")
            int => print(r3)
        }

        x := find(3) or e => -1
        print(x)
    }`);
    expect(out).toBe("43\ntimeout after 100ms\nnot found in users\n-1\n");
  });

  test("行頭 | の複数行union: ベンチ第1ラウンドの実提出コード形が末尾まで動く", () => {
    const out = runSource(`type Expr = { kind: "num", value: int }
    | { kind: "add", left: Expr, right: Expr }
    | { kind: "mul", left: Expr, right: Expr }
    | { kind: "neg", operand: Expr }

    fn evaluate(expr: Expr) int {
        return match expr {
            { kind: "num" } => expr.value
            { kind: "add" } => evaluate(expr.left) + evaluate(expr.right)
            { kind: "mul" } => evaluate(expr.left) * evaluate(expr.right)
            { kind: "neg" } => -evaluate(expr.operand)
        }
    }

    fn main() {
        e := Expr{kind: "neg", operand: Expr{kind: "add", left: Expr{kind: "num", value: 1}, right: Expr{kind: "num", value: 1}}}
        print(evaluate(e))
    }`);
    expect(out).toBe("-2\n");
  });
});

describe("モジュールシステム(import / export)", () => {
  const util: ModuleSource = {
    pkg: "util",
    file: "util/u.mesh",
    source: `export fn pub() int { return 10 }
fn priv() int { return 20 }
export struct Vec { x: int }
struct Hidden { x: int }
export fn make(x: int) Vec { return Vec{x: x} }`,
  };

  test("パッケージのexported関数・struct・メソッド・型注釈が使える", () => {
    const out = runModules([
      {
        pkg: "main",
        file: "app.mesh",
        source: `import "mathutil"
fn main() {
	print(mathutil.add(1, 2))
	p := mathutil.Point{x: 3, y: 4}
	print(p.magnitudeSq())
	q: mathutil.Point = mathutil.origin()
	print(q.x, q.y)
	f := mathutil.add
	print(f(10, 20))
}`,
      },
      {
        pkg: "mathutil",
        file: "mathutil/ops.mesh",
        source: `export fn add(a: int, b: int) int { return a + b }`,
      },
      {
        pkg: "mathutil",
        file: "mathutil/point.mesh",
        source: `export struct Point { x: int  y: int }
fn (p: Point) magnitudeSq() int { return p.x * p.x + p.y * p.y }
export fn origin() Point { return Point{x: 0, y: 0} }`,
      },
    ]);
    expect(out).toBe("3\n25\n0 0\n30\n");
  });

  test("同一パッケージ内の複数ファイルはimport不要で互いに見える(未export関数も)", () => {
    const out = runModules([
      {
        pkg: "main",
        file: "app.mesh",
        source: `import "lib"\nfn main() { print(lib.compose(5)) }`,
      },
      { pkg: "lib", file: "lib/a.mesh", source: `export fn compose(n: int) int { return helper(n) + 1 }` },
      { pkg: "lib", file: "lib/b.mesh", source: `fn helper(n: int) int { return n * 2 }` },
    ]);
    expect(out).toBe("11\n");
  });

  test("P6: exported型をパッケージ越しに共有できる(判別可能unionも)", () => {
    const out = runModules([
      {
        pkg: "main",
        file: "app.mesh",
        source: `import "api"
fn main() {
	res := api.getUser("1")
	print(match res {
		{ kind: "ok" } => "found: \${res.name}"
		{ kind: "notFound" } => "404"
	})
	print(match api.getUser("9") {
		{ kind: "ok" } => "?"
		{ kind: "notFound" } => "404"
	})
}`,
      },
      {
        pkg: "api",
        file: "api/api.mesh",
        source: `export type UserResult = { kind: "ok", name: string } | { kind: "notFound" }
export fn getUser(id: string) UserResult {
	if id == "1" { return UserResult{kind: "ok", name: "alice"} }
	return UserResult{kind: "notFound"}
}`,
      },
    ]);
    expect(out).toBe("found: alice\n404\n");
  });

  test("可視性: 未exportの関数・struct・型へのアクセスはエラー", () => {
    const err = (source: string) => {
      const r = compileModules([{ pkg: "main", file: "app.mesh", source }, util]);
      expect(r.code).toBe(null);
      return r.diagnostics.map((d) => d.message).join("\n");
    };
    expect(err(`import "util"\nfn main() { print(util.priv()) }`)).toContain(
      "'priv' is not exported by package 'util'",
    );
    expect(err(`import "util"\nfn main() { h := util.Hidden{x: 1}\nprint(h) }`)).toContain(
      "'Hidden' is not exported by package 'util'",
    );
    expect(err(`import "util"\nfn main() { print(util.nope()) }`)).toContain(
      "package 'util' has no exported function or constant 'nope'",
    );
  });

  test("誘導エラー: パッケージ名を値に使う・型を関数のように呼ぶ・alias衝突", () => {
    const err = (source: string) => {
      const r = compileModules([{ pkg: "main", file: "app.mesh", source }, util]);
      return r.diagnostics.map((d) => d.message).join("\n");
    };
    expect(err(`import "util"\nfn main() { print(util) }`)).toContain(
      "'util' is a package — use it as a qualifier",
    );
    expect(err(`import "util"\nfn main() { print(util.Vec()) }`)).toContain("'Vec' is a type");
    expect(err(`import "util"\nfn main() { util := 1\nprint(util) }`)).toContain(
      "conflicts with an imported package name",
    );
  });

  test("import循環と未知パッケージはコンパイルエラー", () => {
    const cyc = compileModules([
      { pkg: "main", file: "app.mesh", source: `import "a"\nfn main() { print(a.f()) }` },
      { pkg: "a", file: "a/a.mesh", source: `import "b"\nexport fn f() int { return b.g() }` },
      { pkg: "b", file: "b/b.mesh", source: `import "a"\nexport fn g() int { return 1 }` },
    ]);
    expect(cyc.diagnostics.map((d) => d.message).join("\n")).toContain("import cycle: a -> b -> a");

    const unknown = compileModules([
      { pkg: "main", file: "app.mesh", source: `import "nothere"\nfn main() {}` },
    ]);
    expect(unknown.diagnostics.map((d) => d.message).join("\n")).toContain("unknown package 'nothere'");
  });

  test("modules_demo.mesh — CLI経由でimportがディスクから解決される", () => {
    const CLI = join(import.meta.dir, "..", "src", "cli.ts");
    const example = join(import.meta.dir, "..", "examples", "modules_demo.mesh");
    const proc = spawnSync(process.execPath, [CLI, "run", example], { encoding: "utf8", timeout: 15_000 });
    expect(proc.status).toBe(0);
    expect(proc.stdout).toBe("3\n12\n25\n0 0\n");
  });

  test("F-9c: トップレベル定数を実行できる(単一パッケージ)", () => {
    const out = runSource(`maxRetries := 3\nlabel: string = "try #"\nfn main() {\n\tprint(label, maxRetries)\n}`);
    expect(out).toBe("try # 3\n");
  });

  test("F-9c: トップレベルの参照値(配列/map/struct)は再代入できないが中身は関数から変更できる", () => {
    // card.tsが以前「グローバルな可変状態は無い」と自己矛盾していた挙動をそのまま固定するテスト
    // (:= の不変性は束縛の不変性であってデータの不変性ではない、というのはローカル変数と同じ)
    const out = runSource(`events: string[] = []
fn logEvent(msg: string) { push(events, msg) }
fn main() {
	logEvent("a")
	logEvent("b")
	print(len(events))
	print(events)
}`);
    expect(out).toBe("2\n[a b]\n");
  });

  test("F-9c: exportしたトップレベル定数をパッケージ越しに参照できる", () => {
    // main が先、依存パッケージ config が後(cli.tsの発見順と同じ)。JSのconstはhoistされない
    // ので、codegen側が依存順(config → main)に並べ替えてから出さないとTDZエラーになる
    const out = runModules([
      {
        pkg: "main",
        file: "app.mesh",
        source: `import "config"\nfn main() { print(config.MaxRetries + 1) }`,
      },
      { pkg: "config", file: "config/c.mesh", source: `export MaxRetries := 3` },
    ]);
    expect(out).toBe("4\n");
  });
});

describe("F-14: mesh/io + mesh/json(組み込みパッケージ)", () => {
  test("json.parse → 判別可能unionとして絞り込み → json.stringifyで往復できる", () => {
    const out = runSource(`import "mesh/json"
fn main() {
	r := json.parse("{\\"a\\": 1, \\"b\\": [true, null, \\"x\\"]}")
	if r is error {
		print("parse failed")
		return
	}
	v := r
	if v is { kind: "obj" } {
		print(len(v.entries))
	}
	print(json.stringify(v))
}`);
    expect(out).toBe(`2\n{"a":1,"b":[true,null,"x"]}\n`);
  });

  test("json.parse: 壊れたJSONは 'error' になる(panicしない)", () => {
    const out = runSource(`import "mesh/json"
fn main() {
	r := json.parse("{not valid json")
	if r is error {
		print("got error")
		return
	}
	print("should not reach here")
}`);
    expect(out).toBe("got error\n");
  });

  test("json.Value{...} で構築してstringifyできる(利用者側からの構築)", () => {
    const out = runSource(`import "mesh/json"
fn main() {
	v := json.Value{kind: "arr", items: [
		json.Value{kind: "num", n: 1.0},
		json.Value{kind: "str", s: "two"},
		json.Value{kind: "bool", b: true},
		json.Value{kind: "null"},
	]}
	print(json.stringify(v))
}`);
    expect(out).toBe(`[1,"two",true,null]\n`);
  });

  test("io.readFile: 実在するファイルを読める / 無いファイルは 'error'(panicしない)", () => {
    const dir = mkdtempSync(join(tmpdir(), "mesh-io-test-"));
    const filePath = join(dir, "data.txt");
    writeFileSync(filePath, "hello from disk");
    const out = runSource(`import "mesh/io"
fn main() {
	ok := io.readFile("${filePath}")
	if ok is error {
		print("unexpected error")
		return
	}
	print(ok)

	missing := io.readFile("${filePath}.does-not-exist")
	if missing is error {
		print("missing handled")
	}
}`);
    expect(out).toBe("hello from disk\nmissing handled\n");
  });

  test("io.args(): CLI経由で渡した引数がプログラムから見える", () => {
    const dir = mkdtempSync(join(tmpdir(), "mesh-io-args-"));
    const meshPath = join(dir, "main.mesh");
    writeFileSync(
      meshPath,
      `import "mesh/io"\nfn main() {\n\tfor _, a := range io.args() {\n\t\tprint(a)\n\t}\n}`,
    );
    const CLI = join(import.meta.dir, "..", "src", "cli.ts");
    const proc = spawnSync(process.execPath, [CLI, "run", meshPath, "foo", "bar"], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(proc.status).toBe(0);
    expect(proc.stdout).toBe("foo\nbar\n");
  });

  test("mesh/io, mesh/json 以外の mesh/* は未実装のまま(回帰確認)", () => {
    const result = compile(`import "mesh/http"\nfn main() { print(1) }`);
    expect(result.code).toBeNull();
    expect(result.diagnostics.map((d) => d.message).join("\n")).toContain("unknown package 'mesh/http'");
  });

  test("退行防止: 組み込みパッケージと同名のユーザーパッケージ('io'/'json')は検査時にエラーになる", () => {
    // レビューで見つかった穴: 以前はregistryが黙って上書きされ、検査は通るのに生成JSが
    // 同名関数の二重宣言でロード時にクラッシュしていた(P4違反 — Meshのソースに一切結びつかない
    // 素のJSパースエラーになる)
    const result = compileModules([
      { pkg: "main", file: "app.mesh", source: `import "io"\nfn main() { print(io.args()) }` },
      { pkg: "io", file: "io/io.mesh", source: `export fn args() string[] { return [] }` },
    ]);
    expect(result.code).toBeNull();
    const messages = result.diagnostics.map((d) => d.message).join("\n");
    expect(messages).toContain("package name 'io' collides with the built-in package 'mesh/io'");
  });
});

describe("F-15: mesh test --json(テストランナー)", () => {
  const CLI = join(import.meta.dir, "..", "src", "cli.ts");
  const newProjectDir = () => mkdtempSync(join(tmpdir(), "mesh-test-cmd-"));

  test("fn testXxx() none | error の合否・panic隔離が正しく動く", () => {
    const dir = newProjectDir();
    writeFileSync(
      join(dir, "main.mesh"),
      `fn add(a: int, b: int) int { return a + b }\nfn main() { print(add(1, 2)) }`,
    );
    writeFileSync(
      join(dir, "main_test.mesh"),
      `fn testAdditionPasses() none | error {
	if add(2, 3) != 5 { return error("bad") }
	return none
}
fn testAdditionFails() none | error {
	if add(2, 2) != 5 { return error("expected 5, got \${add(2, 2)}") }
	return none
}
fn testPanicIsIsolated() none | error {
	nums := [1, 2, 3]
	print(nums[10])
	return none
}`,
    );
    const proc = spawnSync(process.execPath, [CLI, "test", join(dir, "main.mesh")], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(proc.status).toBe(1); // 一部失敗しているので非0
    expect(proc.stdout).toContain("ok   testAdditionPasses");
    expect(proc.stdout).toContain("FAIL testAdditionFails");
    expect(proc.stdout).toContain("expected 5, got 4");
    // panicも1件の失敗として報告され、テストラン自体はクラッシュせず最後まで終わる
    expect(proc.stdout).toContain("FAIL testPanicIsIsolated");
    expect(proc.stdout).toContain("index 10 out of range");
    expect(proc.stdout).toContain("1/3 passed");
  });

  test("退行防止: detach/spawnした背景タスクのpanicは『合格』に紛れず可視化される", () => {
    // レビューで見つかった穴: __runTestsはawaitされたテスト本体しかtry/catchしておらず、
    // detach/spawnの失敗は別の非同期経路(__panic)を通るので、以前は捕まらずにok:trueへ
    // 紛れ込んでいた(detach)か、プロセスがレースで落ちて"internal error"になっていた(spawn)
    const dir = newProjectDir();
    writeFileSync(join(dir, "main.mesh"), `fn main() { print(1) }`);
    writeFileSync(
      join(dir, "main_test.mesh"),
      `fn boom() none | error {
	nums := [1, 2, 3]
	print(nums[10])
	return none
}
fn testDetachPanics() none | error {
	detach boom()
	return none
}
fn testOk() none | error {
	return none
}`,
    );
    const proc = spawnSync(process.execPath, [CLI, "test", join(dir, "main.mesh"), "--json"], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(proc.status).toBe(1); // 背景タスクのpanicのせいで全体はfailのはず
    const report = JSON.parse(proc.stdout);
    expect(report.ok).toBe(false);
    expect(report.tests.find((t: { name: string }) => t.name === "testDetachPanics").pass).toBe(true);
    expect(report.tests.find((t: { name: string }) => t.name === "testOk").pass).toBe(true);
    const bg = report.tests.find((t: { name: string }) => t.name === "(background task)");
    expect(bg.pass).toBe(false);
    expect(bg.message).toContain("index 10 out of range");
  });

  test("退行防止: spawnした背景タスクのpanicも同様に可視化される(以前はプロセスがレースで落ちていた)", () => {
    const dir = newProjectDir();
    writeFileSync(join(dir, "main.mesh"), `fn main() { print(1) }`);
    writeFileSync(
      join(dir, "main_test.mesh"),
      `fn boom() none | error {
	nums := [1, 2, 3]
	print(nums[10])
	return none
}
fn testSpawnPanics() none | error {
	spawn boom()
	wait { }
	return none
}`,
    );
    const proc = spawnSync(process.execPath, [CLI, "test", join(dir, "main.mesh"), "--json"], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(proc.status).toBe(1);
    const report = JSON.parse(proc.stdout);
    expect(report.ok).toBe(false);
    expect(report.tests.some((t: { name: string; message?: string }) => t.message?.includes("index 10 out of range")))
      .toBe(true);
  });

  test("--json は構造化結果を返す", () => {
    const dir = newProjectDir();
    writeFileSync(join(dir, "main.mesh"), `fn main() { print(1) }`);
    writeFileSync(join(dir, "main_test.mesh"), `fn testOk() none | error { return none }`);
    const proc = spawnSync(process.execPath, [CLI, "test", join(dir, "main.mesh"), "--json"], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(proc.status).toBe(0);
    const report = JSON.parse(proc.stdout);
    expect(report).toEqual({ ok: true, tests: [{ name: "testOk", file: join(dir, "main_test.mesh"), pass: true }] });
  });

  test("テストが1つも無ければその旨を表示し、exit 0", () => {
    const dir = newProjectDir();
    writeFileSync(join(dir, "main.mesh"), `fn main() { print(1) }`);
    const proc = spawnSync(process.execPath, [CLI, "test", join(dir, "main.mesh")], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(proc.status).toBe(0);
    expect(proc.stdout).toContain("no tests found");
  });

  test("シグネチャが不正なテスト関数はコンパイルエラーになる", () => {
    const dir = newProjectDir();
    writeFileSync(join(dir, "main.mesh"), `fn main() { print(1) }`);
    writeFileSync(join(dir, "main_test.mesh"), `fn testBad(x: int) int { return x }`);
    const proc = spawnSync(process.execPath, [CLI, "test", join(dir, "main.mesh")], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(proc.status).toBe(1);
    expect(proc.stderr).toContain("invalid-test-signature");
  });

  test("ディレクトリを渡すとそのパッケージ自身をテストする(依存元のテストは実行しない)", () => {
    const dir = newProjectDir();
    mkdirSync(join(dir, "mathutil"));
    writeFileSync(join(dir, "mathutil", "ops.mesh"), `export fn square(n: int) int { return n * n }`);
    writeFileSync(
      join(dir, "mathutil", "ops_test.mesh"),
      `fn testSquare() none | error {
	if square(4) != 16 { return error("bad") }
	return none
}`,
    );
    writeFileSync(join(dir, "app.mesh"), `import "mathutil"\nfn main() { print(mathutil.square(3)) }`);
    writeFileSync(
      join(dir, "app_test.mesh"),
      `fn testAppUsesSquare() none | error {
	if mathutil.square(5) != 25 { return error("bad") }
	return none
}`,
    );

    // mathutil単体のテストだけが走る
    const pkgProc = spawnSync(process.execPath, [CLI, "test", join(dir, "mathutil"), "--json"], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(pkgProc.status).toBe(0);
    expect(JSON.parse(pkgProc.stdout).tests.map((t: { name: string }) => t.name)).toEqual(["testSquare"]);

    // mainのテストだけが走る(mathutilのテストは含まれない)
    const appProc = spawnSync(process.execPath, [CLI, "test", join(dir, "app.mesh"), "--json"], {
      encoding: "utf8",
      timeout: 15_000,
    });
    expect(appProc.status).toBe(0);
    expect(JSON.parse(appProc.stdout).tests.map((t: { name: string }) => t.name)).toEqual(["testAppUsesSquare"]);
  });

  test("mesh testはmain()が無いパッケージも検査・実行できる(TDD向け)", () => {
    const dir = newProjectDir();
    mkdirSync(join(dir, "greet"));
    writeFileSync(join(dir, "greet", "greet.mesh"), `export fn hello(name: string) string { return "hi \${name}" }`);
    writeFileSync(
      join(dir, "greet", "greet_test.mesh"),
      `fn testHello() none | error {
	if hello("bob") != "hi bob" { return error("bad") }
	return none
}`,
    );
    const proc = spawnSync(process.execPath, [CLI, "test", join(dir, "greet")], { encoding: "utf8", timeout: 15_000 });
    expect(proc.status).toBe(0);
    expect(proc.stdout).toContain("1/1 passed");
  });
});

describe("退行防止: toFloat/round/floor/ceil(float→intの片道しか無かった穴)", () => {
  // レビュー起点: int→floatは自動昇格するが逆方向が無く、json.Value.n(float)を
  // 配列添字やループ境界のintへ戻す手段が無かった。round/floor/ceilで丸め方向を選ばせ、
  // 結果がsafe integer範囲を超えたらpanicする(F-10と同じ無音の精度崩れを許さない方針)
  test("json.Value.n(float)をroundでintに戻して配列添字に使える", () => {
    const out = runSource(`import "mesh/json"
fn main() {
  items := ["a", "b", "c"]
  v := json.parse("1") or _ => json.Value{kind: "null"}
  if v is { kind: "num" } {
    print(items[round(v.n)])
  }
}`);
    expect(out).toBe("b\n");
  });

  test("toFloat: intをfloatにしてfloat除算を強制できる", () => {
    expect(runSource(`fn main() { print(toFloat(7) / toFloat(2)) }`)).toBe("3.5\n");
  });

  test("floor/ceil: 切り捨て・切り上げの向きが正しい", () => {
    expect(runSource(`fn main() { print(floor(3.7)) }`)).toBe("3\n");
    expect(runSource(`fn main() { print(ceil(3.2)) }`)).toBe("4\n");
    expect(runSource(`fn main() { print(round(3.5)) }`)).toBe("4\n");
  });

  test("panic: round/floor/ceilの結果がsafe integer範囲を超えたら停止する", () => {
    // Meshの浮動小数点リテラルは指数表記非対応なので、乗算で範囲外を作る
    const huge = "huge := 100000000000000.0\n\thuge = huge * huge";
    expect(runSourceExpectPanic(`fn main() {\n\tmut ${huge}\n\tprint(round(huge))\n}`)).toContain(
      "exceeds the safe integer range",
    );
    expect(runSourceExpectPanic(`fn main() {\n\tmut ${huge}\n\tprint(floor(huge))\n}`)).toContain(
      "exceeds the safe integer range",
    );
    expect(runSourceExpectPanic(`fn main() {\n\tmut ${huge}\n\tprint(ceil(huge))\n}`)).toContain(
      "exceeds the safe integer range",
    );
  });

  test("型検査: toFloatはint以外、round/floor/ceilはfloat以外を弾く", () => {
    const badToFloat = compile(`fn main() { print(toFloat(1.5)) }`);
    expect(badToFloat.code).toBeNull();
    expect(badToFloat.diagnostics.map((d) => d.message).join("\n")).toContain(
      "toFloat() requires an int",
    );
    const badRound = compile(`fn main() { print(round(5)) }`);
    expect(badRound.code).toBeNull();
    expect(badRound.diagnostics.map((d) => d.message).join("\n")).toContain("round() requires a float");
  });
});

describe("退行防止: __proto__をstructフィールド名に使うと拒否される", () => {
  // レビュー起点: codegenはstruct literalを素のJSオブジェクトリテラル({ name: value })へ
  // 直訳するため、フィールド名が'__proto__'だと代入ではなくprototypeの差し替えになり、
  // 値が検査エラーも実行時エラーも無く黙って消えていた
  test("通常のstruct宣言で__proto__をフィールド名にするとコンパイルエラー", () => {
    const result = compile(`struct Sneaky {\n\t__proto__: string\n}\nfn main() {\n\ts := Sneaky{__proto__: "x"}\n\tprint(s.__proto__)\n}`);
    expect(result.code).toBeNull();
    expect(result.diagnostics.map((d) => d.message).join("\n")).toContain(
      "'__proto__' can't be used as a field name",
    );
  });

  test("判別可能unionの無名struct(F-7)でも__proto__は拒否される", () => {
    const result = compile(
      `type Shape = { kind: "circle", __proto__: float } | { kind: "square", side: float }\nfn main() { print(1) }`,
    );
    expect(result.code).toBeNull();
    expect(result.diagnostics.map((d) => d.message).join("\n")).toContain(
      "'__proto__' can't be used as a field name",
    );
  });
});

describe("e2e", () => {
  test("hello.mesh", () => {
    expect(runExample("hello.mesh")).toBe("Hello, Mesh!\n");
  });

  test("fizzbuzz.mesh", () => {
    expect(runExample("fizzbuzz.mesh")).toBe(
      "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n",
    );
  });

  test("status.mesh — type宣言とリテラルunion", () => {
    expect(runExample("status.mesh")).toBe("ようこそ\nアクセス不可\n");
  });

  test("maps.mesh — mapとfor range", () => {
    expect(runExample("maps.mesh")).toBe(
      "alice is 30\ndave is unknown\n2 people\nalice: 30\ncarol: 28\ntotal: 60\ntick 0\ntick 1\ntick 2\n",
    );
  });

  test("channel_spec.mesh — close/T|closed/容量/select", () => {
    expect(runExample("channel_spec.mesh")).toBe("total: 6\ngot: from a\nnothing ready\n");
  });

  test("users.mesh — struct+union+match", () => {
    expect(runExample("users.mesh")).toBe(
      "hello alice (30)\n404 not found\n500: invalid id: -1\n",
    );
  });

  test("discriminated_union.mesh — タグ付きstruct形式のunion", () => {
    expect(runExample("discriminated_union.mesh")).toBe(
      "found: alice\nnot found\nunauthorized\n",
    );
  });

  test("tree.mesh — 自己参照する判別可能union(木構造)", () => {
    expect(runExample("tree.mesh")).toBe("6\n3\n");
  });

  test("errors.mesh — union型エラーハンドリング(is / or / match)", () => {
    expect(runExample("errors.mesh")).toBe(
      "10 / 3 = 3\nfallback: 0\nlogged: 16\ncaught: division by zero\nmatch says: 3\n",
    );
  });

  test("channels.mesh — spawnとchannel", () => {
    expect(runExample("channels.mesh")).toBe(
      "worker 1 done\nworker 2 done\nworker 3 done\n",
    );
  });

  test("spawn式: 受取口を返し、後から待てる", () => {
    const out = runSource(`fn double(n: int) int {
      sleep(30)
      return n * 2
    }
    fn main() {
      task := spawn double(21)
      print("waiting")
      print(<-task)
    }`);
    expect(out).toBe("waiting\n42\n");
  });

  test("spawn式: 2つ起動して並行に走る(合計時間で確認)", () => {
    const out = runSource(`fn slow(n: int) int {
      sleep(80)
      return n
    }
    fn main() {
      a := spawn slow(1)
      b := spawn slow(2)
      // <-a の型は int | closed(2026-07-18のclose対応で常にこうなる)。絞り込んでから使う
      va := <-a
      if va is closed {
        return
      }
      vb := <-b
      if vb is closed {
        return
      }
      print(va + vb)
    }`);
    expect(out).toBe("3\n");
  });

  test("配列型: chan<T>[] で受取口をまとめてfan-out/fan-inできる", () => {
    const out = runSource(`fn compute(n: int) int {
      return n * n
    }
    fn main() {
      nums := [1, 2, 3, 4, 5]
      mut tasks: chan<int>[] = []
      for _, n := range nums {
        push(tasks, spawn compute(n))
      }
      mut sum := 0
      for _, t := range tasks {
        v := <-t
        if v is closed {
        } else {
          sum = sum + v
        }
      }
      print(sum)
    }`);
    expect(out).toBe("55\n");
  });

  test("waitブロック: 中で起動したタスクを全部待つ", () => {
    const out = runSource(`fn addTo(arr: int[], v: int) {
      sleep(40)
      push(arr, v)
    }
    fn main() {
      arr := [0]
      wait {
        spawn addTo(arr, 1)
        spawn addTo(arr, 2)
      }
      print(len(arr))
    }`);
    expect(out).toBe("3\n");
  });

  test("spawnの直後の行では完了していない(暗黙waitは関数の出口でだけ効く)", () => {
    const out = runSource(`fn addTo(arr: int[], v: int) {
      sleep(40)
      push(arr, v)
    }
    fn main() {
      arr := [0]
      spawn addTo(arr, 1)
      print(len(arr))
    }`);
    expect(out).toBe("1\n");
  });

  test("2段スコープ: spawnは関数を抜けるとき暗黙に待たれる(リーク不可能)", () => {
    const out = runSource(`fn addTo(arr: int[], v: int) {
      sleep(40)
      push(arr, v)
    }
    fn work(arr: int[]) {
      spawn addTo(arr, 1)
      // 明示的な wait は書いていない — それでも work は addTo 完了後に戻る
    }
    fn main() {
      arr := [0]
      work(arr)
      print(len(arr))
    }`);
    expect(out).toBe("2\n");
  });

  test("2段スコープ: 早期returnでもspawnは待たれる", () => {
    const out = runSource(`fn addTo(arr: int[], v: int) {
      sleep(40)
      push(arr, v)
    }
    fn work(arr: int[]) int {
      spawn addTo(arr, 1)
      return 99
    }
    fn main() {
      arr := [0]
      r := work(arr)
      print(r, len(arr))
    }`);
    expect(out).toBe("99 2\n");
  });

  test("2段スコープ: detachは関数の外まで生き延びる(呼び出し元は待たない)", () => {
    const out = runSource(`fn addTo(arr: int[], v: int) {
      sleep(40)
      push(arr, v)
    }
    fn work(arr: int[]) {
      detach addTo(arr, 1)
      // detach はプログラム所有 — work は待たずに即戻る
    }
    fn main() {
      arr := [0]
      work(arr)
      print(len(arr))
    }`);
    expect(out).toBe("1\n");
  });

  test("2段スコープ: detachも受取口を返す(spawn と対称)", () => {
    const out = runSource(`fn slow(n: int) int {
      sleep(30)
      return n * 2
    }
    fn main() {
      task := detach slow(21)
      print(<-task)
    }`);
    expect(out).toBe("42\n");
  });

  test("int同士の除算は切り捨て、floatが混ざれば小数", () => {
    expect(runSource(`fn main() { print(7 / 2) }`)).toBe("3\n");
    expect(runSource(`fn main() { print(7.0 / 2) }`)).toBe("3.5\n");
  });

  test("型付き配列リテラル: 空から push で育てる", () => {
    const out = runSource(`struct Item {
      name: string
    }
    fn main() {
      items: Item[] = []
      push(items, Item{name: "a"})
      push(items, Item{name: "b"})
      for _, it := range items {
        print(it.name)
      }
      nums := int[]{1, 2, 3}
      print(len(items), len(nums))
    }`);
    expect(out).toBe("a\nb\n2 3\n");
  });

  test("配列と len / push", () => {
    const out = runSource(`fn main() {
      nums := [10, 20]
      push(nums, 30)
      print(len(nums), nums[2])
    }`);
    expect(out).toBe("3 30\n");
  });

  test("無名関数とクロージャ", () => {
    const out = runSource(`fn main() {
      double := fn(x: int) int { return x * 2 }
      print(double(21))
    }`);
    expect(out).toBe("42\n");
  });

  test("再帰関数 (フィボナッチ)", () => {
    const out = runSource(`fn fib(n: int) int {
      if n < 2 {
        return n
      }
      return fib(n - 1) + fib(n - 2)
    }
    fn main() { print(fib(10)) }`);
    expect(out).toBe("55\n");
  });

  test("none は 'none' と表示される", () => {
    expect(runSource(`fn main() { print(none) }`)).toBe("none\n");
  });

  test("T | none: is none で絞り込んで使う", () => {
    const out = runSource(`fn find(id: int) string | none {
      if id == 1 {
        return "alice"
      }
      return none
    }
    fn main() {
      name := find(1)
      if name is none {
        print("not found")
        return
      }
      print("found: \${name}")
      missing := find(99)
      if missing is none {
        print("99 is missing")
      }
    }`);
    expect(out).toBe("found: alice\n99 is missing\n");
  });

  test("'!' は失敗を呼び出し元へ伝播する", () => {
    const out = runSource(`fn parseEven(n: int) int | error {
      if n % 2 != 0 {
        return error("odd: \${n}")
      }
      return n
    }
    fn doubled(n: int) int | error {
      v := parseEven(n)?
      return v * 2
    }
    fn main() {
      a := doubled(4)
      if a is error {
        return
      }
      print(a)
      b := doubled(3)
      if b is error {
        print("caught: \${b}")
      }
    }`);
    expect(out).toBe("8\ncaught: odd: 3\n");
  });

  test("match式: 値を返し、アーム内で絞り込まれる", () => {
    const out = runSource(`fn divide(a: int, b: int) int | error {
      if b == 0 {
        return error("div by zero")
      }
      return a / b
    }
    fn label(a: int, b: int) string {
      r := divide(a, b)
      return match r {
        error => "failed: \${r}"
        int => "ok: \${r}"
      }
    }
    fn main() {
      print(label(10, 2))
      print(label(1, 0))
    }`);
    expect(out).toBe("ok: 5\nfailed: div by zero\n");
  });

  test("match式: 3メンバー union と _ ワイルドカード", () => {
    const out = runSource(`fn pick(n: int) int | none | error {
      if n == 0 {
        return none
      }
      if n < 0 {
        return error("negative")
      }
      return n
    }
    fn main() {
      a := pick(5)
      print(match a {
        int => "got \${a}"
        _ => "nothing"
      })
      b := pick(0)
      print(match b {
        int => "got \${b}"
        _ => "nothing"
      })
    }`);
    expect(out).toBe("got 5\nnothing\n");
  });

  test("type宣言+リテラルunion+matchの実行", () => {
    const out = runSource(`type Status = "active" | "banned" | "pending"

    fn label(s: Status) string {
      return match s {
        "active" => "OK"
        "banned", "pending" => "NG"
      }
    }
    fn main() {
      print(label("active"))
      print(label("pending"))
      print(label("banned"))
    }`);
    expect(out).toBe("OK\nNG\nNG\n");
  });

  test("type宣言: エイリアスは union の別名としても使える", () => {
    const out = runSource(`type Result = int | error

    fn half(n: int) Result {
      if n % 2 != 0 {
        return error("odd")
      }
      return n / 2
    }
    fn main() {
      r := half(10)
      if r is error {
        return
      }
      print(r)
    }`);
    expect(out).toBe("5\n");
  });

  test("配列リテラル: 複数行(末尾カンマ無し)でstructの要素を書ける", () => {
    const out = runSource(`struct Order {
      id: int
      amount: float
      status: string
    }
    fn (o: Order) summary() string {
      return "Order \${o.id}: \${o.amount} (\${o.status})"
    }
    fn main() {
      orders := [
        Order{id: 1, amount: 100.0, status: "paid"},
        Order{id: 2, amount: 50.0, status: "pending"},
        Order{id: 3, amount: 200.0, status: "paid"}
      ]
      isPaid := fn(o: Order) bool { return o.status == "paid" }
      paid := filter(orders, isPaid)
      discounted := map(paid, fn(o: Order) float { return o.amount * 0.9 })
      total := reduce(discounted, fn(acc: float, x: float) float { return acc + x }, 0.0)
      for _, o := range paid {
        print(o.summary())
      }
      print(total)
    }`);
    expect(out).toBe("Order 1: 100 (paid)\nOrder 3: 200 (paid)\n270\n");
  });

  test("struct: 生成・アクセス・フィールド更新・print", () => {
    const out = runSource(`struct User {
      name: string
      age: int
    }
    fn main() {
      u := User{name: "alice", age: 30}
      print("\${u.name} (\${u.age})")
      u.age = 31
      print(u.age)
      print(u)
    }`);
    expect(out).toBe("alice (30)\n31\n{name: alice, age: 31}\n");
  });

  test("struct: 再帰struct(リンクリスト)の走査", () => {
    const out = runSource(`struct Node {
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
      list := Node{value: 1, next: Node{value: 2, next: Node{value: 3, next: none}}}
      print(sum(list))
    }`);
    expect(out).toBe("6\n");
  });

  test("判別可能union: 宣言・構築・matchでの絞り込み・実行結果", () => {
    const out = runSource(`struct User {
      name: string
    }
    type GetUserResponse = { kind: "ok", user: User } | { kind: "notFound" } | { kind: "unauthorized" }
    fn getUser(id: string) GetUserResponse {
      if id == "1" {
        return GetUserResponse{ kind: "ok", user: User{name: "alice"} }
      }
      if id == "2" {
        return GetUserResponse{ kind: "unauthorized" }
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
    fn main() {
      print(describe(getUser("1")))
      print(describe(getUser("2")))
      print(describe(getUser("3")))
    }`);
    expect(out).toBe("found: alice\nunauthorized\nnot found\n");
  });

  test("struct: 名前的型付け(F-3) — 同形でも別名は弾かれ、無名メンバーには渡せる", () => {
    // 名前付き同士: 形が同じでも別の型(コンパイルエラー)
    expect(
      compile(`struct Meters { value: float }
struct Dollars { value: float }
fn charge(amount: Dollars) { print(amount.value) }
fn main() { charge(Meters{value: 100.0}) }`).diagnostics[0]?.message,
    ).toContain("cannot use Meters as Dollars");

    // 無名{...}メンバー(判別可能union)の場所には、同形の名前付きstructを渡せる(実行まで確認)
    const out = runSource(`struct Ok { kind: "ok" }
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
    expect(out).toBe("OK\n");
  });

  test("struct: match の型パターンで分解できる", () => {
    const out = runSource(`struct User {
      name: string
    }
    fn find(id: int) User | none | error {
      if id == 1 {
        return User{name: "alice"}
      }
      if id < 0 {
        return error("bad id")
      }
      return none
    }
    fn label(id: int) string {
      u := find(id)
      return match u {
        User => "hello \${u.name}"
        none => "404"
        error => "500"
      }
    }
    fn main() {
      print(label(1))
      print(label(2))
      print(label(0 - 1))
    }`);
    expect(out).toBe("hello alice\n404\n500\n");
  });

  test("map: 生成・読み(V|none)・書き・delete・len・print", () => {
    const out = runSource(`fn main() {
      ages := map<string, int>{"alice": 30, "bob": 25}
      ages["carol"] = 28

      print(ages["alice"] or 0)
      missing := ages["dave"]
      if missing is none {
        print("dave? none")
      }

      delete(ages, "bob")
      print(len(ages))
      print(ages)
    }`);
    expect(out).toBe("30\ndave? none\n2\nmap{alice: 30, carol: 28}\n");
  });

  test("range: 配列(i,v / _,v)・map(k,v)・int", () => {
    const out = runSource(`fn main() {
      nums := [10, 20, 30]
      mut total := 0
      for i, v := range nums {
        print("\${i}: \${v}")
      }
      for _, v := range nums {
        total = total + v
      }
      print(total)

      ages := map<string, int>{"a": 1, "b": 2}
      for k, v := range ages {
        print("\${k}=\${v}")
      }

      for i := range 3 {
        print(i)
      }
    }`);
    expect(out).toBe("0: 10\n1: 20\n2: 30\n60\na=1\nb=2\n0\n1\n2\n");
  });

  test("'or' は失敗時だけ右辺を評価する", () => {
    const out = runSource(`fn f(ok: bool) int | error {
      if ok {
        return 10
      }
      return error("boom")
    }
    fn main() {
      print(f(true) or _ => 0)
      print(f(false) or _ => 0)
    }`);
    expect(out).toBe("10\n0\n");
  });

  test("文字列補間: 式が埋め込まれて評価される", () => {
    expect(runSource(`fn main() { print("worker \${1 + 2} done") }`)).toBe("worker 3 done\n");
  });

  test("文字列補間: 変数・文字列式・先頭の式", () => {
    const out = runSource(`fn main() {
      name := "mesh"
      print("hello \${name}!")
      print("\${len(name)} chars")
      print("x\${"y"}z")
    }`);
    expect(out).toBe("hello mesh!\n4 chars\nxyz\n");
  });

  test("文字列補間: \\$ エスケープでリテラルの $ を書ける", () => {
    expect(runSource(`fn main() { print("price \\$100") }`)).toBe("price $100\n");
  });

  test("panic: 配列の範囲外アクセスは位置つきで即停止", () => {
    const stderr = runSourceExpectPanic(`fn main() {
      nums := [1, 2, 3]
      print(nums[10])
    }`);
    expect(stderr).toContain("panic: main.mesh:3:13: index 10 out of range (length 3)");
  });

  test("panic: 実行時のゼロ除算・ゼロ剰余", () => {
    expect(runSourceExpectPanic(`fn main() {\n\tzero := 0\n\tprint(10 / zero)\n}`)).toContain(
      "integer division by zero",
    );
    expect(runSourceExpectPanic(`fn main() {\n\tzero := 0\n\tprint(10 % zero)\n}`)).toContain(
      "integer modulo by zero",
    );
  });

  test("panic: F-10 int演算がsafe integerの範囲を超えたら即停止", () => {
    expect(
      runSourceExpectPanic(`fn main() {\n\tbig := 9007199254740991\n\tprint(big + 1)\n}`),
    ).toContain("integer overflow");
    expect(
      runSourceExpectPanic(`fn main() {\n\tbig := -9007199254740991\n\tprint(big - 1)\n}`),
    ).toContain("integer overflow");
    expect(
      runSourceExpectPanic(`fn main() {\n\tbig := 9007199254740991\n\tprint(big * 2)\n}`),
    ).toContain("integer overflow");
  });

  test("退行防止: 整数リテラル自体がsafe-integer範囲を超えていればコンパイル時に検出する", () => {
    // レビューで見つかった穴: F-10の__iarithは演算結果しか検査しておらず、
    // リテラルそのものが既に範囲外(9007199254740992以上)だと無検査で通っていた
    const result = compile(`fn main() {\n\tx := 9007199254740993\n\tprint(x)\n}`);
    expect(result.code).toBeNull();
    expect(result.diagnostics.map((d) => d.message).join("\n")).toContain(
      "integer literal 9007199254740993 exceeds the safe integer range",
    );
    // 境界値(MAX_SAFE_INTEGERちょうど)はエラーにならない
    expect(compile(`fn main() {\n\tx := 9007199254740991\n\tprint(x)\n}`).code).not.toBeNull();
  });

  test("F-9b: 複合代入 += -= *= /= %= が実行時にも正しく動く", () => {
    expect(runSource(`fn main() {\n\tmut x := 10\n\tx += 5\n\tprint(x)\n}`)).toBe("15\n");
    expect(runSource(`fn main() {\n\tmut x := 10\n\tx -= 3\n\tprint(x)\n}`)).toBe("7\n");
    expect(runSource(`fn main() {\n\tmut x := 4\n\tx *= 3\n\tprint(x)\n}`)).toBe("12\n");
    expect(runSource(`fn main() {\n\tmut x := 10\n\tx /= 3\n\tprint(x)\n}`)).toBe("3\n");
    expect(runSource(`fn main() {\n\tmut x := 10\n\tx %= 3\n\tprint(x)\n}`)).toBe("1\n");
    expect(runSource(`fn main() {\n\tmut s := "a"\n\ts += "b"\n\tprint(s)\n}`)).toBe("ab\n");
  });

  test("F-9b: 複合代入は配列の添字にも実行時に効く", () => {
    expect(
      runSource(`fn main() {\n\tmut nums := [1, 2, 3]\n\tnums[1] += 10\n\tprint(nums[1])\n}`),
    ).toBe("12\n");
  });

  test("panic: 複合代入もsafe-integer検査・ゼロ除算検査を通る", () => {
    expect(
      runSourceExpectPanic(`fn main() {\n\tmut big := 9007199254740991\n\tbig += 1\n\tprint(big)\n}`),
    ).toContain("integer overflow");
    expect(
      runSourceExpectPanic(`fn main() {\n\tmut x := 10\n\tmut zero := 0\n\tx /= zero\n\tprint(x)\n}`),
    ).toContain("integer division by zero");
  });

  test("panic: 範囲外への書き込みも配列を黙って伸ばさない", () => {
    const stderr = runSourceExpectPanic(`fn main() {
      nums := [1]
      nums[5] = 9
    }`);
    expect(stderr).toContain("index 5 out of range (length 1)");
  });

  test("範囲内のアクセス・書き込み・除算は通常どおり動く", () => {
    const out = runSource(`fn main() {
      nums := [10, 20, 30]
      nums[1] = 21
      print(nums[1], 7 / 2, 7 % 2, len("abc"))
    }`);
    expect(out).toBe("21 3 1 3\n");
  });

  test("float のゼロ除算は panic せず Infinity(Goと同じ割り切り)", () => {
    expect(runSource(`fn main() { print(1.0 / 0.0) }`)).toBe("Infinity\n");
  });

  test("標準ライブラリ第一弾: contains / indexOf", () => {
    const out = runSource(`fn main() {
      nums := [10, 20, 30]
      print(contains(nums, 20))
      print(contains(nums, 99))

      i := indexOf(nums, 20)
      if i is none {
        return
      }
      print(i)

      j := indexOf(nums, 99)
      if j is none {
        print("not found")
      }
    }`);
    expect(out).toBe("true\nfalse\n1\nnot found\n");
  });

  test("F-9d: get(arr, i) は範囲外でもpanicせず none を返す", () => {
    const out = runSource(`fn main() {
      nums := [10, 20, 30]
      v := get(nums, 1)
      if v is none {
        print("none")
      } else {
        print(v)
      }
      w := get(nums, 99)
      if w is none {
        print("none")
      } else {
        print(w)
      }
      print(get(nums, -1) or -999)
    }`);
    expect(out).toBe("20\nnone\n-999\n");
  });

  test("標準ライブラリ第一弾: keys / values(mapの挿入順)", () => {
    const out = runSource(`fn main() {
      ages := map<string, int>{}
      ages["b"] = 2
      ages["a"] = 1
      print(keys(ages))
      print(values(ages))
    }`);
    expect(out).toBe("[b a]\n[2 1]\n");
  });

  test("標準ライブラリ第一弾: sortは非破壊で元の配列を変えない", () => {
    const out = runSource(`fn main() {
      nums := [3, 1, 2]
      sorted := sort(nums)
      print(nums)
      print(sorted)

      words := ["banana", "apple", "cherry"]
      print(sort(words))
    }`);
    expect(out).toBe("[3 1 2]\n[1 2 3]\n[apple banana cherry]\n");
  });

  test("標準ライブラリ第二弾: split / join / trim / upper / lower", () => {
    const out = runSource(`fn main() {
      parts := split("a,b,c", ",")
      print(parts)
      print(join(parts, "-"))
      print(trim("  hi  "))
      print(upper("hi"))
      print(lower("HI"))

      lone := split("no-sep", ",")
      print(lone)
    }`);
    expect(out).toBe("[a b c]\na-b-c\nhi\nHI\nhi\n[no-sep]\n");
  });

  test("標準ライブラリ第二弾: toIntは成功/失敗の両方をunionで処理", () => {
    const out = runSource(`fn main() {
      n := toInt("42")
      if n is error {
        return
      }
      print(n + 1)

      m := toInt("abc")
      if m is error {
        print("failed: \${m}")
      }

      k := toInt("nope") or _ => -1
      print(k)
    }`);
    expect(out).toBe("43\nfailed: \"abc\" is not a valid int\n-1\n");
  });

  test("標準ライブラリ第二弾: toIntは符号・境界値も正しく扱う", () => {
    const out = runSource(`fn main() {
      a := toInt("-5") or _ => 0
      print(a)
      b := toInt("3.14") or _ => -1
      print(b)
      c := toInt("") or _ => -1
      print(c)
      d := toInt(" 5") or _ => -1
      print(d)
    }`);
    expect(out).toBe("-5\n-1\n-1\n-1\n");
  });

  test("標準ライブラリ第三弾: filterは名前付き関数を値として渡せる", () => {
    const out = runSource(`fn isEven(n: int) bool {
      return n % 2 == 0
    }
    fn main() {
      nums := [1, 2, 3, 4, 5, 6]
      evens := filter(nums, isEven)
      print(evens)
      print(nums)   // filter は非破壊
    }`);
    expect(out).toBe("[2 4 6]\n[1 2 3 4 5 6]\n");
  });

  test("標準ライブラリ第三弾: filterはインラインクロージャで外側のmut変数も捕捉できる", () => {
    const out = runSource(`fn main() {
      nums := [1, 2, 3, 4, 5]
      mut threshold := 3
      big := filter(nums, fn(n: int) bool { return n > threshold })
      print(big)
      threshold = 1
      print(filter(nums, fn(n: int) bool { return n > threshold }))
    }`);
    expect(out).toBe("[4 5]\n[2 3 4 5]\n");
  });

  test("標準ライブラリ第三弾: mapは要素の型を変えられる(int[] → string[])(F-8)", () => {
    const out = runSource(`fn main() {
      nums := [1, 2, 3]
      labels := map(nums, fn(n: int) string { return "n\${n}" })
      print(labels)
    }`);
    expect(out).toBe("[n1 n2 n3]\n");
  });

  test("標準ライブラリ第三弾: reduceは合計・文字列への畳み込みの両方ができる", () => {
    const out = runSource(`fn main() {
      nums := [1, 2, 3, 4]
      total := reduce(nums, fn(acc: int, n: int) int { return acc + n }, 0)
      print(total)

      joined := reduce(nums, fn(acc: string, n: int) string { return acc + str(n) }, "")
      print(joined)
    }`);
    expect(out).toBe("10\n1234\n");
  });

  test("標準ライブラリ第三弾: filter→map→reduceのパイプライン(F-8)", () => {
    const out = runSource(`fn isEven(n: int) bool {
      return n % 2 == 0
    }
    fn double(n: int) int {
      return n * 2
    }
    fn sum(acc: int, n: int) int {
      return acc + n
    }
    fn main() {
      nums := [1, 2, 3, 4, 5, 6]
      result := reduce(map(filter(nums, isEven), double), sum, 0)
      print(result)   // (2+4+6)*2 = 24
    }`);
    expect(out).toBe("24\n");
  });

  test("メソッド: 基本の宣言・呼び出し・連鎖(chaining)", () => {
    const out = runSource(`struct Todo {
      title: string
      done: bool
    }
    fn (t: Todo) complete() Todo {
      return Todo{title: t.title, done: true}
    }
    fn (t: Todo) render() string {
      if t.done {
        return "[x] " + t.title
      }
      return "[ ] " + t.title
    }
    fn main() {
      todos := [Todo{title: "a", done: false}, Todo{title: "b", done: false}]
      todos[0] = todos[0].complete()
      for _, t := range todos {
        print(t.render())
      }
      // 連鎖: 左から右へ読める
      print(Todo{title: "c", done: false}.complete().render())
    }`);
    expect(out).toBe("[x] a\n[ ] b\n[x] c\n");
  });

  test("メソッド: 同名メソッドがstructごとに別物として動く", () => {
    const out = runSource(`struct User { name: string }
    struct Order { id: int }
    fn (u: User) describe() string { return "user " + u.name }
    fn (o: Order) describe() string { return "order #" + str(o.id) }
    fn main() {
      u := User{name: "alice"}
      o := Order{id: 42}
      print(u.describe())
      print(o.describe())
    }`);
    expect(out).toBe("user alice\norder #42\n");
  });

  test("メソッド: レシーバはstructの参照値そのもの(フィールド書き込みが反映される)", () => {
    const out = runSource(`struct Counter { n: int }
    fn (c: Counter) inc() {
      c.n = c.n + 1
    }
    fn main() {
      list := [Counter{n: 0}]
      list[0].inc()
      list[0].inc()
      print(list[0].n)
    }`);
    // struct のフィールド書き込みは束縛の可変性と無関係(既存仕様。structは参照値)
    expect(out).toBe("2\n");
  });

  test("メソッド: 引数を取り、他のメソッド・関数を呼べる", () => {
    const out = runSource(`struct Todo {
      title: string
      done: bool
    }
    fn (t: Todo) withTitle(newTitle: string) Todo {
      return Todo{title: newTitle, done: t.done}
    }
    fn (t: Todo) summary() string {
      return t.withTitle(upper(t.title)).render()
    }
    fn (t: Todo) render() string {
      return "<" + t.title + ">"
    }
    fn main() {
      t := Todo{title: "abc", done: false}
      print(t.summary())
    }`);
    expect(out).toBe("<ABC>\n");
  });

  test("channel仕様: close + T|closed で終端を検出できる", () => {
    const out = runSource(`fn produce(ch: chan<int>) {
      for i := 1; i <= 3; i++ {
        ch <- i
      }
      close(ch)
    }
    fn main() {
      ch := chan<int>(none)
      spawn produce(ch)
      mut total := 0
      for {
        v := <-ch
        if v is closed {
          break
        }
        total = total + v
      }
      print(total)
    }`);
    expect(out).toBe("6\n");
  });

  test("channel仕様: close済みへの送信・二重closeはpanic", () => {
    expect(
      runSourceExpectPanic(`fn main() {\n\tch := chan<int>(none)\n\tclose(ch)\n\tclose(ch)\n}`),
    ).toContain("close of closed channel");
    expect(
      runSourceExpectPanic(`fn main() {\n\tch := chan<int>(none)\n\tclose(ch)\n\tch <- 1\n}`),
    ).toContain("send on closed channel");
  });

  test("channel仕様: chan<T>(n) は本物のブロッキング送信(バッファが空くまで待つ)", () => {
    const out = runSource(`fn producer(ch: chan<int>, log: string[]) {
      ch <- 1
      push(log, "sent 1")
      ch <- 2
      push(log, "sent 2")   // 容量1なので、1が受信されるまでここはブロックする
    }
    fn main() {
      ch := chan<int>(1)
      log: string[] = []
      spawn producer(ch, log)
      sleep(30)
      print(log)          // まだ "sent 1" だけのはず(2個目はブロック中)
      v1 := <-ch
      if v1 is closed { return }
      sleep(10)
      print(log)          // 受信したので "sent 1" "sent 2" になっているはず
      v2 := <-ch
      if v2 is closed { return }
      print(v1, v2)
    }`);
    expect(out).toBe('[sent 1]\n[sent 1 sent 2]\n1 2\n');
  });

  test("channel仕様: select は準備できたアームを選ぶ", () => {
    const out = runSource(`fn slowSend(ch: chan<string>, msg: string, ms: int) {
      sleep(ms)
      ch <- msg
    }
    fn main() {
      a := chan<string>(none)
      b := chan<string>(none)
      spawn slowSend(a, "from a", 15)
      spawn slowSend(b, "from b", 60)
      msg := select {
        v := <-a => "got: \${v}"
        v := <-b => "got: \${v}"
      }
      print(msg)
    }`);
    expect(out).toBe("got: from a\n");
  });

  test("channel仕様: select の _ (default) は非ブロッキングにする", () => {
    const out = runSource(`fn main() {
      empty := chan<int>(none)
      r := select {
        v := <-empty => "unexpected"
        _ => "nothing ready"
      }
      print(r)
    }`);
    expect(out).toBe("nothing ready\n");
  });

  test("channel仕様: selectでもclosedをmatchで扱える", () => {
    const out = runSource(`fn main() {
      ch := chan<int>(none)
      close(ch)
      r := select {
        v := <-ch => match v {
          closed => "closed"
          int => "got " + str(v)
        }
      }
      print(r)
    }`);
    expect(out).toBe("closed\n");
  });
});
