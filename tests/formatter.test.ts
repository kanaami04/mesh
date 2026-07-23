// mesh fmt: 印字が「べき等」で「意味を変えない」ことを主に検証する。
// 意味を変えないことの確認は、フォーマット前後のソースをそれぞれコンパイル・実行して
// 標準出力が一致するかで見る(AST比較ではなく実行結果で判定するのが一番信頼できる)

import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { format } from "../src/formatter";

function runViaCli(path: string): { stdout: string; stderr: string; status: number | null } {
  const proc = spawnSync(process.execPath, [join(import.meta.dir, "..", "src", "cli.ts"), "run", path], {
    encoding: "utf8",
    timeout: 15_000,
  });
  return { stdout: proc.stdout, stderr: proc.stderr, status: proc.status };
}

function formatFile(source: string): string {
  return format(source);
}

describe("mesh fmt: べき等性・意味保存(examples全体)", () => {
  const examplesDir = join(import.meta.dir, "..", "examples");
  const files = readdirSync(examplesDir).filter((f) => f.endsWith(".mesh"));

  for (const f of files) {
    test(`${f}: 整形後も同じ出力・整形は収束する`, () => {
      const src = readFileSync(join(examplesDir, f), "utf8");
      const formatted = formatFile(src);
      const formatted2 = formatFile(formatted);
      expect(formatted2).toBe(formatted); // べき等性

      const dir = mkdtempSync(join(tmpdir(), "mesh-fmt-"));
      // 依存パッケージ(mathutil等)を使うexampleのために、examples/配下のディレクトリを
      // そのまま使う(コピー不要 — 元ファイルは書き換えず、整形結果だけ別名で置く)
      const origResult = runViaCli(join(examplesDir, f));
      const fmtPath = join(dir, f);
      writeFileSync(fmtPath, formatted);
      // mathutilのような依存パッケージ解決はプロジェクトルート(エントリファイルのディレクトリ)
      // 基準なので、依存パッケージが要る場合は同ディレクトリにも複製する
      // (modules_demo.mesh は examples/mathutil を、json_models_demo.mesh は
      // examples/jsonmodels を使う)
      if (f === "modules_demo.mesh") {
        cpSync(join(examplesDir, "mathutil"), join(dir, "mathutil"), { recursive: true });
      }
      if (f === "json_models_demo.mesh") {
        cpSync(join(examplesDir, "jsonmodels"), join(dir, "jsonmodels"), { recursive: true });
      }
      const fmtResult = runViaCli(fmtPath);
      expect(fmtResult.stdout).toBe(origResult.stdout);
      expect(fmtResult.status).toBe(origResult.status);
    });
  }
});

describe("mesh fmt: コメントの保持", () => {
  test("leadingコメントはノードの直前にそのまま残る", () => {
    const src = `// this is a comment\nfn main() {\n\tprint(1)\n}\n`;
    expect(format(src)).toBe(`// this is a comment\nfn main() {\n\tprint(1)\n}\n`);
  });

  test("trailingコメントは同じ行の末尾に残る", () => {
    const src = `fn main() {\n\tx := 1 // trailing\n\tprint(x)\n}\n`;
    const out = format(src);
    expect(out).toContain("x := 1  // trailing");
  });

  test("複数のleadingコメント(連続する行)は順序を保って残る", () => {
    const src = `// line 1\n// line 2\nfn main() {\n\tprint(1)\n}\n`;
    const out = format(src);
    expect(out.startsWith("// line 1\n// line 2\n")).toBe(true);
  });

  test("文の直前のleadingコメントも保持される", () => {
    const src = `fn main() {\n\t// explain\n\tprint(1)\n}\n`;
    const out = format(src);
    expect(out).toContain("\t// explain\n\tprint(1)");
  });

  test("ファイル末尾の孤立コメントも消えない(安全弁)", () => {
    const src = `fn main() {\n\tprint(1)\n}\n// trailing file comment\n`;
    const out = format(src);
    expect(out).toContain("// trailing file comment");
  });
});

describe("mesh fmt: 複数行/単一行の選択を尊重する(gofmt方式・幅による自動折り返しはしない)", () => {
  test("単一行のstruct literalはそのまま単一行", () => {
    const src = `struct P { x: int\n\ty: int }\nfn main() { p := P{x: 1, y: 2}\nprint(p) }\n`;
    expect(format(src)).toContain("p := P{x: 1, y: 2}");
  });

  test("複数行で書かれたstruct literalは複数行のまま", () => {
    const src = `struct P {\n\tx: int\n\ty: int\n}\nfn main() {\n\tp := P{\n\t\tx: 1,\n\t\ty: 2,\n\t}\n\tprint(p)\n}\n`;
    const out = format(src);
    expect(out).toContain("p := P{\n\t\tx: 1\n\t\ty: 2\n\t}");
  });

  test("複数行で書かれたunion宣言は行頭'|'継続のまま", () => {
    const src = `type Status = "a"\n\t| "b"\n\t| "c"\nfn main() { print(1) }\n`;
    const out = format(src);
    expect(out).toContain('type Status = "a"\n\t| "b"\n\t| "c"');
  });

  test("単一行のインラインクロージャはそのまま単一行(実際の慣習に合わせる)", () => {
    const src = `fn first(arr: int[], pred: fn(int) bool) int | none {\n\tfor _, v := range arr {\n\t\tif pred(v) { return v }\n\t}\n\treturn none\n}\nfn main() {\n\tprint(first([1, 2, 3], fn(n: int) bool { return n > 1 }))\n}\n`;
    const out = format(src);
    expect(out).toContain("fn(n: int) bool { return n > 1 }");
  });
});

describe("退行防止: mesh fmtが意味を変えてしまっていたバグ(見つかって直したもの)", () => {
  test("リテラルの \\${...} を再フォーマットしても評価されない(補間ではなく文字として残す)", () => {
    // レビューで見つかった穴: 素朴な文字列再構築は \${1+1} を再パース時に本当の補間だと
    // 誤認し、"literal ${1+1}" が実際に評価された "2" へ化けてしまっていた
    const src = `fn main() {\n\ta := "should stay literal: \\\${1+1}"\n\tprint(a)\n}\n`;
    const out = format(src);
    expect(out).toContain('"should stay literal: \\${1+1}"');

    const dir = mkdtempSync(join(tmpdir(), "mesh-fmt-"));
    const path = join(dir, "prog.mesh");
    writeFileSync(path, out);
    const result = runViaCli(path);
    expect(result.stdout).toBe("should stay literal: ${1+1}\n");
  });

  test("複数行の関数呼び出し引数(呼び出しの閉じ括弧が独立行)がASI衝突で壊れない", () => {
    // レビューで見つかった穴: print(match r {...}) や dist(Point{...}) のような複数行呼び出しを
    // 素朴に改行しただけだと、最後の引数の直後にASIで挿入される';'がカンマ必須の引数リスト
    // 文法と衝突して構文エラーになっていた。呼び出し引数は末尾にも","を付けて解消した
    const src = `struct P { x: int\n\ty: int }\nfn dist(p: P) int { return p.x + p.y }\nfn main() {\n\ttotal := dist(\n\t\tP{x: 3, y: 4},\n\t)\n\tprint(total)\n}\n`;
    const out = format(src);
    const dir = mkdtempSync(join(tmpdir(), "mesh-fmt-"));
    const path = join(dir, "prog.mesh");
    writeFileSync(path, out);
    const result = runViaCli(path);
    expect(result.status).toBe(0);
    expect(result.stdout).toBe("7\n");
  });

  test("'json struct'の'json'キーワードは再整形しても消えない(milestone 9のexample追加で発覚)", () => {
    // printTypeDeclがisError(→"error ")だけ見ていてisJsonを見ておらず、json struct宣言を
    // 再整形すると普通のstructになってしまっていた——decodeUserが合成されなくなり
    // 「そのdecode関数が存在しない」という壊れた挙動になる(examples/json_decode.meshの
    // フォーマッタ往復テストで発覚)
    const src = `import "mesh/json"\njson struct User {\n\tname: string\n}\nfn main() {\n\tv := json.parse("{\\"name\\": \\"a\\"}") or _ => json.Value{kind: "null"}\n\tu := decodeUser(v)\n\tif u is error { print("failed"); return }\n\tprint(u.name)\n}\n`;
    const out = format(src);
    expect(out).toContain("json struct User {");

    const dir = mkdtempSync(join(tmpdir(), "mesh-fmt-"));
    const path = join(dir, "prog.mesh");
    writeFileSync(path, out);
    const result = runViaCli(path);
    expect(result.stdout).toBe("a\n");
  });
});

describe("mesh fmt: 基本的な整形(インデント・空白の正規化)", () => {
  test("インデントはタブに統一される", () => {
    const src = `fn main() {\n    print(1)\n}\n`; // スペース4つのインデントを混ぜる
    const out = format(src);
    expect(out).toContain("\tprint(1)");
  });

  test("べき等性: 一度整形した結果を再度整形しても変わらない(全体)", () => {
    const src = `fn main(){print("hi")\nx:=1\nif x>0{print(x)}}`;
    const out1 = format(src);
    const out2 = format(out1);
    expect(out2).toBe(out1);
  });
});
