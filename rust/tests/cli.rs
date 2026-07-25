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
fn check_は診断をソース行とキャレット付きで出す() {
    let path = temp_mesh("diag", "fn main() {\n    xs := [1, \"a\"]\n    print(xs)\n}\n");
    let p = path.display().to_string();
    let out = mesh(&["check", &p]);
    assert_eq!(out.code, 1);
    assert!(out.stdout.contains("error[type-mismatch]"), "stdout: {}", out.stdout);
    // 見出しの次の行にソース行、その次にキャレット（TS版formatDiagnosticsと同じ3行構成）
    let lines: Vec<&str> = out.stdout.lines().collect();
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
        assert!(out.stdout.contains("error[not-indexable]"), "{cmd}: stdout: {}", out.stdout);
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
    assert!(out.stdout.contains("error[syntax-error]"), "stdout: {}", out.stdout);
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
