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

// このファイルの2つのテストは検査対象の**隣に**整形結果を一時ファイルとして置く
// (依存パッケージのimportを解決させるため)。cargoは同じファイル内のテストを並行実行するので、
// **相手の一時ファイルを`read_dir`が拾わないよう除外する**——拾うと、相手が消した直後の
// パスを検査して偽の失敗になる(実際に踏んだ)。異常終了で残った物を拾わない効果もある
fn is_temp_artifact(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("__"))
}

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
    try_fmt(path).unwrap_or_else(|e| panic!("fmt失敗 {}: {e}", path.display()))
}

// パースできない入力もコーパスに含まれる(`tests/parity/58-syntax-error/`等)ので、
// 失敗を`Err`で返す版。**呼び出し側は「なぜ失敗したか」を必ず確かめること**
// ——確かめずにスキップすると、本物のfmtの不具合を「対象外」として静かに握り潰す
fn try_fmt(path: &Path) -> Result<String, String> {
    let out = Command::new(mesh_bin()).args(["fmt", path.to_str().unwrap()]).output().expect("mesh fmt を起動できること");
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(String::from_utf8(out.stdout).expect("整形結果がUTF-8であること"))
}

// `mesh check` が出した診断コードを**出力順のまま**集める。
// 位置(行・桁)は整形で当然変わるので比べない——変わってはいけないのは「何を報告したか」
fn check_codes(path: &Path) -> Vec<String> {
    let out = Command::new(mesh_bin()).args(["check", path.to_str().unwrap()]).output().expect("mesh check を起動できること");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    text.split("error[")
        .skip(1)
        .filter_map(|s| s.split(']').next())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .map(|s| s.to_string())
        .collect()
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
        .filter(|p| p.extension().is_some_and(|x| x == "mesh") && !is_temp_artifact(p))
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

// 上のテストは意味の保存を**実行して標準出力を比べる**ことで見る。強い判定だが、
// **走らないプログラムには原理的に効かない**——診断を出させるためのプログラム
// (`tests/parity/`の132ケース)は丸ごと対象外だった。実際にそこへ
// 「`!(x is none)`が`!x is none`になる」というfmtの不具合が住み着いていた
// (整形の**べき等性は保たれたまま意味だけ壊れる**ので、べき等性の検査も素通りしていた)。
//
// そこで判定を「実行結果の一致」から「**診断の一致**」へ広げる。実行を要求しないので
// JSランタイムが無くても走り、対象が examples 24本 → 156本になる。
#[test]
fn コーパス全体で整形は診断を変えない() {
    let root = repo_root();
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("examples"))
        .expect("examples/ を読めること")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mesh") && !is_temp_artifact(p))
        .collect();
    files.extend(
        std::fs::read_dir(root.join("tests/parity"))
            .expect("tests/parity/ を読めること")
            .flatten()
            .map(|e| e.path().join("main.mesh"))
            .filter(|p| p.is_file()),
    );
    files.sort();
    assert!(files.len() > 100, "コーパスの収集が壊れている(集まったのは{}件)", files.len());

    let mut checked = 0;
    for f in &files {
        let formatted = match try_fmt(f) {
            Ok(s) => s,
            Err(stderr) => {
                // パースできない入力だけがスキップを許される。fmtが別の理由で落ちたなら不具合
                let codes = check_codes(f);
                assert!(
                    codes.iter().any(|c| c == "syntax-error") || !codes.is_empty(),
                    "{} は整形に失敗したのに診断も出ない(fmtの不具合): {stderr}",
                    f.display()
                );
                continue;
            }
        };
        // 整形結果は**元と同じディレクトリへ別名で置く**(エントリは1ファイルなので
        // 隣に置いてもパッケージには混ざらず、`import`は元どおり解決できる)
        // **上のテストと別の接頭辞にする**——cargoは同じファイル内のテストを並行実行するので、
        // 同名にすると互いの一時ファイルを消し合って偽の失敗になる(実際に踏んだ)
        let side = f.with_file_name(format!("__diagfmt_check_{}", f.file_name().expect("ファイル名").to_string_lossy()));
        std::fs::write(&side, &formatted).expect("整形結果を書けること");
        let before = check_codes(f);
        let after = check_codes(&side);
        let _ = std::fs::remove_file(&side);
        assert_eq!(after, before, "{} は整形で診断が変わった", f.display());
        checked += 1;
    }
    assert!(checked > 100, "実際に比較できたのが{checked}件しかない(スキップが多すぎる)");
}
