// `tests/parity/` のケースを Rust版だけで走らせ、記録済みの期待出力と突き合わせる。
//
// **TS版(オラクル)との突き合わせはここではやらない**——それは `scripts/parity.sh` の仕事で、
// 出荷前にローカルで回す。CIでbunとcargoの両方を入れるのは重いのと、こちらの形なら
// **診断の変化がPRのdiff(expected.txt)に出る**という利点がある。挙動を変えたときに
// 「どの診断がどう変わったか」がレビューで見えるのが狙い。
//
// 期待出力の更新は `scripts/parity.sh --update`(TS版と突き合わせたうえで書き換わる)。
// **手でexpected.txtを書き換えないこと**——オラクルとの照合を飛ばすことになる。
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    // rust/tests/ から2つ上
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("rust/ の親").to_path_buf()
}

fn mesh_bin() -> PathBuf {
    // 統合テストのバイナリは target/debug/deps/ に置かれるので、その2つ上から mesh を引く
    let mut p = std::env::current_exe().expect("test binary path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(if cfg!(windows) { "mesh.exe" } else { "mesh" })
}

// パスをファイル名だけに正規化する(scripts/parity.sh の normalize と同じ意図——
// 絶対パスやチェックアウト先に依存させない)
fn normalize(out: &str) -> String {
    out.lines()
        .map(|line| match line.find(".mesh") {
            Some(end) => {
                let head = &line[..end + 5];
                let start = head.rfind('/').map(|i| i + 1).unwrap_or(0);
                format!("{}{}", &line[start..end + 5], &line[end + 5..])
            }
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn parityコーパスの診断が記録どおり出る() {
    let root = repo_root();
    let dir = root.join("tests/parity");
    let bin = mesh_bin();
    assert!(bin.exists(), "{} が無い(先に cargo build)", bin.display());

    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} を読めない: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("main.mesh").is_file())
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "parityコーパスが空");

    let mut failures = Vec::new();
    for case in &cases {
        let expected_path = case.join("expected.txt");
        let Ok(expected) = std::fs::read_to_string(&expected_path) else {
            failures.push(format!("{} が無い(scripts/parity.sh --update で作る)", expected_path.display()));
            continue;
        };
        let out = Command::new(&bin)
            .arg("check")
            .arg(case.join("main.mesh"))
            .output()
            .unwrap_or_else(|e| panic!("mesh check の起動に失敗: {e}"));
        let mut combined = String::from_utf8_lossy(&out.stdout).to_string();
        combined.push_str(&String::from_utf8_lossy(&out.stderr));
        let actual = normalize(combined.trim_end());
        if actual != expected.trim_end() {
            failures.push(format!(
                "--- {} の診断が記録と違う\n期待:\n{}\n実際:\n{}",
                case.file_name().unwrap_or_default().to_string_lossy(),
                expected.trim_end(),
                actual
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} 件のケースで診断が変わった。**意図した変更なら `scripts/parity.sh --update` で\
         TS版と突き合わせたうえで記録を更新すること**(手でexpected.txtを書き換えない):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
