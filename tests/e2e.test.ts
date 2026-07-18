// E2E テスト: .mesh をコンパイル → 生成された JS を実行 → 標準出力を照合
import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { compile } from "../src/compiler";

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

  test("users.mesh — struct+union+match", () => {
    expect(runExample("users.mesh")).toBe(
      "hello alice (30)\n404 not found\n500: invalid id: -1\n",
    );
  });

  test("errors.mesh — union型エラーハンドリング(is / or / match)", () => {
    expect(runExample("errors.mesh")).toBe(
      "10 / 3 = 3\nfallback: 0\ncaught: division by zero\nmatch says: 3\n",
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
      print(<-a + <-b)
    }`);
    expect(out).toBe("3\n");
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

  test("waitなしなら起動直後は完了していない(対照実験)", () => {
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

  test("int同士の除算は切り捨て、floatが混ざれば小数", () => {
    expect(runSource(`fn main() { print(7 / 2) }`)).toBe("3\n");
    expect(runSource(`fn main() { print(7.0 / 2) }`)).toBe("3.5\n");
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
      v := parseEven(n)!
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

  test("'or' は失敗時だけ右辺を評価する", () => {
    const out = runSource(`fn f(ok: bool) int | error {
      if ok {
        return 10
      }
      return error("boom")
    }
    fn main() {
      print(f(true) or 0)
      print(f(false) or 0)
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
});
