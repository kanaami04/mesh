// CLIの統合テスト(milestone 35)。ビルド済みバイナリを実際に起動して、
// サブコマンドの入出力・終了コード・full_checkerゲートの挙動を確認する。
// **JSランタイム(bun/node)に依存する`run`のテストは、見つからない環境では
// スキップする**——CIには入っているが、無い環境でテスト全体を落とす理由は無い。
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR は <repo>/rust
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("rust/ の親がリポジトリルート").to_path_buf()
}

fn mesh_bin() -> PathBuf {
    // cargo が統合テスト用に用意する環境変数。プロファイル(debug/release)や
    // target ディレクトリの場所を自前で組み立てるより確実
    PathBuf::from(env!("CARGO_BIN_EXE_mesh"))
}

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn mesh(args: &[&str]) -> Output {
    let out = Command::new(mesh_bin()).args(args).current_dir(repo_root()).output().expect("mesh バイナリを起動できること（先に cargo build が必要）");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn js_runtime_available() -> bool {
    ["bun", "node"].iter().any(|r| Command::new(r).arg("--version").output().is_ok())
}

// 一時ファイルを作る（テスト間で衝突しないようテスト名を混ぜる）
fn temp_mesh(name: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mesh-cli-test-{name}.mesh"));
    std::fs::write(&path, source).expect("一時ファイルを書けること");
    path
}

#[test]
fn check_は無診断のファイルでno_errorsを出す() {
    let out = mesh(&["check", "examples/hello.mesh"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("no errors"), "stdout: {}", out.stdout);
}

#[test]
fn check_は診断をソース行とキャレット付きでstderrへ出す() {
    let path = temp_mesh("diag", "fn main() {\n    xs := [1, \"a\"]\n    print(xs)\n}\n");
    let p = path.display().to_string();
    let out = mesh(&["check", &p]);
    assert_eq!(out.code, 1);
    // **診断はstderr**（TS版`cli.ts`が`console.error(formatDiagnostics(...))`を使うのと同じ。
    // stdoutは"no errors"と`--json`の出力専用）。code reviewで発覚した逸脱の回帰テスト
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert!(out.stderr.contains("error[type-mismatch]"), "stderr: {}", out.stderr);
    // 見出しの次の行にソース行、その次にキャレット（TS版formatDiagnosticsと同じ3行構成）
    let lines: Vec<&str> = out.stderr.lines().collect();
    assert!(lines[1].contains("xs := [1, \"a\"]"), "lines: {lines:?}");
    assert!(lines[2].trim_start().starts_with('^'), "lines: {lines:?}");
}

#[test]
fn check_jsonは機械可読な構造化出力を出す() {
    let path = temp_mesh("json", "fn main() {\n    xs := []\n    print(xs)\n}\n");
    let p = path.display().to_string();
    let out = mesh(&["check", &p, "--json"]);
    assert_eq!(out.code, 1);
    assert!(out.stdout.contains("\"ok\": false"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("\"code\": \"cannot-infer-type\""), "stdout: {}", out.stdout);
    // 無診断なら ok: true で終了コード0
    let ok = mesh(&["check", "examples/hello.mesh", "--json"]);
    assert_eq!(ok.code, 0);
    assert!(ok.stdout.contains("\"ok\": true"), "stdout: {}", ok.stdout);
}

#[test]
fn buildはjsを書き出す() {
    let out_path = std::env::temp_dir().join("mesh-cli-test-build.mjs");
    let o = out_path.display().to_string();
    let out = mesh(&["build", "examples/hello.mesh", "-o", &o]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let js = std::fs::read_to_string(&out_path).expect("生成JSが書かれていること");
    assert!(js.contains("Hello, Mesh!"), "js: {}", &js[..js.len().min(200)]);
    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn runは生成jsを実行して標準出力を返す() {
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため run のテストをスキップ");
        return;
    }
    let out = mesh(&["run", "examples/hello.mesh"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "Hello, Mesh!\n");
}

#[test]
fn runはpanicの終了コードを伝える() {
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため run のテストをスキップ");
        return;
    }
    // defer_panic.mesh は範囲外アクセスでpanicする（deferは実行される）
    let out = mesh(&["run", "examples/defer_panic.mesh"]);
    assert_eq!(out.code, 1, "stdout: {} stderr: {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("cleanup ran"), "stdout: {}", out.stdout);
}

#[test]
fn run_buildはfull_checkerのゲートで止まる() {
    // 型エラーのあるプログラムは codegen まで進まず、診断を出して終了コード1
    let path = temp_mesh("gate", "fn main() {\n    n := 5\n    print(n[0])\n}\n");
    let p = path.display().to_string();
    for cmd in ["run", "build"] {
        let out = mesh(&[cmd, &p]);
        assert_eq!(out.code, 1, "{cmd}: stdout: {} stderr: {}", out.stdout, out.stderr);
        assert!(out.stderr.contains("error[not-indexable]"), "{cmd}: stderr: {}", out.stderr);
    }
}

#[test]
fn 複数ファイル_importありでも動く() {
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため run のテストをスキップ");
        return;
    }
    let out = mesh(&["run", "examples/modules_demo.mesh"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!out.stdout.is_empty());
}

#[test]
fn 構文エラーはソース行付きで報告され型検査へ進まない() {
    let path = temp_mesh("syntax", "fn main() {\n    x := \n}\n");
    let p = path.display().to_string();
    let out = mesh(&["check", &p]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("error[syntax-error]"), "stderr: {}", out.stderr);
}

#[test]
fn importしたパッケージの中身も検査される() {
    // 回帰(code reviewで発覚): ゲートがエントリファイルしか検査しておらず、
    // importしたパッケージ内の型エラーを素通りさせていた(TS版は検出する)。
    // 全モジュールを1本ずつ検査する形にして解消——`fn main`の要求はmainパッケージだけ
    let dir = std::env::temp_dir().join("mesh-cli-test-multi");
    let pkg = dir.join("badpkg");
    std::fs::create_dir_all(&pkg).expect("一時パッケージを作れること");
    std::fs::write(dir.join("main.mesh"), "import \"badpkg\"\n\nfn main() {\n    print(badpkg.broken())\n}\n").unwrap();
    std::fs::write(pkg.join("ops.mesh"), "export fn broken() int {\n    n := 5\n    return n[0]\n}\n").unwrap();
    let entry = dir.join("main.mesh").display().to_string();
    let out = mesh(&["check", &entry]);
    assert_eq!(out.code, 1, "stdout: {} stderr: {}", out.stdout, out.stderr);
    assert!(out.stderr.contains("error[not-indexable]"), "stderr: {}", out.stderr);
    assert!(out.stderr.contains("ops.mesh"), "診断は該当ファイル名で出るべき: {}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json出力は構文エラーでもjsonのまま() {
    // 回帰(code reviewで発覚): 構文エラーのときだけ素のテキストを出しており、
    // `--json`をパースするエージェントがそこでJSONパースに失敗していた
    // (TS版`compileModules`はパースエラーを型検査の診断と同じ配列へ畳み込む)。
    // README/requirements.mdが自己修正ループの前提として明記している契約なので実害がある
    let path = temp_mesh("json-syntax", "fn main() {\n    x := \n}\n");
    let p = path.display().to_string();
    let out = mesh(&["check", &p, "--json"]);
    assert_eq!(out.code, 1);
    assert!(out.stdout.starts_with('{'), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("\"code\": \"syntax-error\""), "stdout: {}", out.stdout);
    assert!(out.stdout.trim_end().ends_with('}'), "stdout: {}", out.stdout);
}

#[test]
fn fmtは正規形を標準出力へ出す() {
    // インデントをタブへ、空行は正規化(TS版と同じ規則)
    let path = temp_mesh("fmt", "fn main() {\n      print(1)\n\n      print(2)\n}\n");
    let p = path.display().to_string();
    let out = mesh(&["fmt", &p]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "fn main() {\n\tprint(1)\n\tprint(2)\n}\n");
    // 元ファイルは書き換わらない
    assert!(std::fs::read_to_string(&path).unwrap().contains("      print(1)"));
}

#[test]
fn fmt_wは元ファイルへ書き戻す() {
    let path = temp_mesh("fmt-w", "fn main() {\n      print(1)\n}\n");
    let p = path.display().to_string();
    let out = mesh(&["fmt", &p, "-w"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "fn main() {\n\tprint(1)\n}\n");
}

#[test]
fn fmtは構文エラーをstderrへ出して書き戻さない() {
    let path = temp_mesh("fmt-syntax", "fn main() {\n    x := \n}\n");
    let p = path.display().to_string();
    let before = std::fs::read_to_string(&path).unwrap();
    let out = mesh(&["fmt", &p, "-w"]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("error[syntax-error]"), "stderr: {}", out.stderr);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before, "壊れたソースを書き潰してはいけない");
}

// `mesh test`用: 対象ディレクトリを作ってファイルを書く
fn temp_test_dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mesh-cli-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("一時ディレクトリを作れること");
    for (f, src) in files {
        std::fs::write(dir.join(f), src).expect("一時ファイルを書けること");
    }
    dir
}

#[test]
fn testは合否を並べて終了コードで返す() {
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため test のテストをスキップ");
        return;
    }
    let dir = temp_test_dir(
        "test-run",
        &[
            ("main.mesh", "fn add(a: int, b: int) int {\n\treturn a + b\n}\n\nfn main() {\n\tprint(add(1, 2))\n}\n"),
            ("main_test.mesh", "fn testOk() none | error {\n\tif add(1, 2) != 3 {\n\t\treturn error(\"bad\")\n\t}\n\treturn none\n}\n\nfn testNg() none | error {\n\treturn error(\"boom\")\n}\n"),
        ],
    );
    let entry = dir.join("main.mesh").display().to_string();
    let out = mesh(&["test", &entry]);
    assert_eq!(out.code, 1, "失敗テストがあるので1: {} {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("ok   testOk"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("FAIL testNg"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("boom"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("1/2 passed"), "stdout: {}", out.stdout);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn testはテストが無ければその旨を出して成功で終わる() {
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため test のテストをスキップ");
        return;
    }
    let dir = temp_test_dir("test-none", &[("main.mesh", "fn main() {\n\tprint(1)\n}\n")]);
    let entry = dir.join("main.mesh").display().to_string();
    let out = mesh(&["test", &entry]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("no tests found"), "stdout: {}", out.stdout);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn testはシグネチャ違反を報告して実行しない() {
    // `fn test...`は`() none | error`固定(F-15)
    let dir = temp_test_dir(
        "test-sig",
        &[("main.mesh", "fn main() {\n\tprint(1)\n}\n"), ("main_test.mesh", "fn testBad(n: int) none | error {\n\treturn none\n}\n")],
    );
    let entry = dir.join("main.mesh").display().to_string();
    let out = mesh(&["test", &entry]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("error[invalid-test-signature]"), "stderr: {}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn 同じパッケージの複数ファイルは相互参照できる() {
    // 回帰(milestone 35のゲート統合以降の誤検知): パッケージ内の別ファイルの関数を
    // 呼ぶだけで`undefined-name`になり、`check`/`run`/`build`が正当なプログラムを弾いていた
    let dir = temp_test_dir("pkg-crossref", &[]);
    let pkg = dir.join("util");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(dir.join("main.mesh"), "import \"util\"\n\nfn main() {\n\tprint(util.twice(3))\n}\n").unwrap();
    std::fs::write(pkg.join("a.mesh"), "export fn twice(n: int) int {\n\treturn double(n)\n}\n").unwrap();
    std::fs::write(pkg.join("b.mesh"), "fn double(n: int) int {\n\treturn n * 2\n}\n").unwrap();
    let entry = dir.join("main.mesh").display().to_string();
    let out = mesh(&["check", &entry]);
    assert_eq!(out.code, 0, "stdout: {} stderr: {}", out.stdout, out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn 引数不足やサブコマンド無しはusageを出す() {
    let no_args = mesh(&[]);
    assert_eq!(no_args.code, 1);
    assert!(no_args.stderr.contains("Usage:"), "stderr: {}", no_args.stderr);
    let no_file = mesh(&["check"]);
    assert_eq!(no_file.code, 1);
    assert!(no_file.stderr.contains("Usage:"), "stderr: {}", no_file.stderr);
}

// --- milestone 38: card / explain -------------------------------------------------

#[test]
fn cardは言語カードを出す() {
    let out = mesh(&["card"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.starts_with("# Mesh Language Card"), "先頭: {:?}", out.stdout.lines().next());
    // 完全版なので「COMPLETE reference」の主張が残っている
    assert!(out.stdout.contains("COMPLETE reference"), "完全版の注記が無い");
}

#[test]
fn card_forは使っている機能のセクションだけに絞る() {
    let path = temp_mesh("card-for", "fn main() {\n    print(\"hi\")\n}\n");
    let file = path.display().to_string();
    let full = mesh(&["card"]);
    let subset = mesh(&["card", "--for", &file]);
    assert_eq!(subset.code, 0, "stderr: {}", subset.stderr);
    assert!(subset.stdout.len() < full.stdout.len(), "絞り込まれていない");
    // 使っていない機能のセクションは落ち、完全版の主張はサブセットの注記に置き換わる
    assert!(!subset.stdout.contains("## Concurrency"), "並行処理のセクションが残っている");
    assert!(subset.stdout.contains("PROJECT-SCOPED SUBSET"), "サブセットの注記が無い");
    assert!(!subset.stdout.contains("COMPLETE reference"), "完全版の主張が残っている");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn card_forは引数不足と読めないファイルをエラーにする() {
    let no_files = mesh(&["card", "--for"]);
    assert_eq!(no_files.code, 1);
    assert_eq!(no_files.stderr, "usage: mesh card --for <file.mesh> [<file2.mesh> ...]\n");
    let missing = mesh(&["card", "--for", "/nonexistent/nope.mesh"]);
    assert_eq!(missing.code, 1);
    assert_eq!(missing.stderr, "error: cannot read file '/nonexistent/nope.mesh'\n");
}

#[test]
fn explainは診断コードを説明する() {
    let out = mesh(&["explain", "division-by-zero"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.starts_with("Integer division or modulo by the literal 0"), "stdout: {}", out.stdout);
}

#[test]
fn explainは引数無しで一覧を出す() {
    let out = mesh(&["explain"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let expected_count = mesh::diagnostic_codes::DiagnosticCode::ALL.len();
    assert!(
        out.stdout.starts_with(&format!("{expected_count} diagnostic codes. Run 'mesh explain <code>' for details.\n\n")),
        "先頭: {:?}",
        out.stdout.lines().next()
    );
    // 一覧は辞書順（TS版の Object.keys(...).sort() と同じ）
    let codes: Vec<&str> = out.stdout.lines().skip(2).filter(|l| !l.is_empty()).collect();
    assert_eq!(codes.len(), expected_count);
    let mut sorted = codes.clone();
    sorted.sort_unstable();
    assert_eq!(codes, sorted);
}

#[test]
fn explainは知らないコードをエラーにする() {
    let out = mesh(&["explain", "no-such-code"]);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "error: unknown diagnostic code 'no-such-code' (run 'mesh explain' with no code to list them all)\n"
    );
    // TS版には説明があるが Rust版がまだ出さない診断も同じ扱い（実装済みの範囲だけ説明する）
    assert_eq!(mesh(&["explain", "narrow-required"]).code, 1);
}
