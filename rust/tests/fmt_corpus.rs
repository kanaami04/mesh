// `mesh fmt` が **べき等**で**意味を変えない**ことを、examples全体に対して確かめる
// (TS撤去 段階3。TS版 `tests/formatter.test.ts` 冒頭のパラメータ化テストの移植)。
//
// Rust側にもフォーマッタの単体テストは13件あるが、**コーパス全体を通す性質テストは無かった**
// ——個別の整形規則が正しくても、実際のプログラムを通したときに壊れないかは別の話。
//
// 意味を変えないことの確認は**整形前後を実行して標準出力が一致するか**で見る。
// AST比較ではなく実行結果で判定するのが一番信頼できる(TS版のコメントの方針をそのまま踏襲)。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn mesh_bin() -> PathBuf {
    let mut p = std::env::current_exe().expect("テストバイナリのパス");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("mesh")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("rust/ の親").to_path_buf()
}

fn js_runtime_available() -> bool {
    ["bun", "node"]
        .iter()
        .any(|c| Command::new(c).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success()))
}

fn fmt(path: &Path) -> String {
    let out = Command::new(mesh_bin()).args(["fmt", path.to_str().unwrap()]).output().expect("mesh fmt を起動できること");
    assert!(out.status.success(), "fmt失敗 {}: {}", path.display(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("整形結果がUTF-8であること")
}

fn run(path: &Path) -> (String, String) {
    let out = Command::new(mesh_bin()).args(["run", path.to_str().unwrap()]).output().expect("mesh run を起動できること");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn examples全体で整形はべき等で意味を変えない() {
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため整形の意味保存テストをスキップ");
        return;
    }
    let root = repo_root();
    let examples = root.join("examples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&examples)
        .expect("examples/ を読めること")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mesh"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "examples/ が空(パスの解決が壊れている)");

    for f in &files {
        let formatted = fmt(f);
        // (1) べき等性: 一度整形した結果を再度整形しても変わらない
        let tmp_dir = std::env::temp_dir().join("mesh-fmt-corpus");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).expect("一時ディレクトリを作れること");
        let once = tmp_dir.join(f.file_name().expect("ファイル名"));
        std::fs::write(&once, &formatted).expect("整形結果を書けること");
        let twice = fmt(&once);
        assert_eq!(twice, formatted, "{} の整形がべき等でない", f.display());

        // (2) 意味保存: 整形前後で実行結果(stdout)が一致する。
        // **依存パッケージを使うexampleがあるので、整形結果は元と同じディレクトリへ別名で置く**
        // (一時ディレクトリへ移すと`import "mathutil"`が解決できない)
        let side = f.with_file_name(format!("__fmt_check_{}", f.file_name().unwrap().to_string_lossy()));
        std::fs::write(&side, &formatted).expect("整形結果を書けること");
        let (orig_out, _) = run(f);
        let (fmt_out, _) = run(&side);
        let _ = std::fs::remove_file(&side);
        let _ = std::fs::remove_dir_all(&tmp_dir);
        assert_eq!(fmt_out, orig_out, "{} は整形で実行結果が変わった", f.display());
    }
}
