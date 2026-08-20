//! CLI(meshバイナリ)の統合テスト。
//! 読み込み層の責務として検証する振る舞い(ADR-0048)を置く。
//! 字句解析器に持ち込めない入力(不正UTF-8のバイト列)の検証はここで行う。
//! 書き方はAAAパターン+1テスト1assert(規約: .claude/skills/test-writing/SKILL.md)。

use std::path::PathBuf;
use std::process::Command;

/// テスト終了時(panicによる失敗時を含む)に必ず削除される一時ファイル。
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// 不正UTF-8のソースファイルを与えたとき、CLIは位置(行・列)つきでE0101を報告すること
/// (仕様1章L-1〔負例: invalid-utf8〕)。ADR-0048によりこの検査は読み込み層の責務で、
/// 字句解析器のテストではなくCLIの振る舞いとして検証する。
/// 3つのassertは「E0101が位置つきで報告される」という1検証項目の構成要素
/// (コード・行・列)。行・列とも1始まり(エディタの表示と一致する形)を期待する。
#[test]
fn invalid_utf8_source_reports_e0101_with_position() {
    // Arrange
    // 3行目 `let x = 0`(9バイト)の直後に不正バイト 0xFF を置く。
    // 0xFF は3行目の10バイト目=1始まりの列10。
    let source: &[u8] = b"let a = 1\nlet b = 2\nlet x = 0\xFF";
    let path = {
        let mut p = std::env::temp_dir();
        p.push("mesh-invalid-utf8-test.mesh");
        std::fs::write(&p, source).expect("一時ファイルに不正バイト列を書き込めること");
        p
    };
    let path_str = path
        .to_str()
        .expect("一時ファイルのパスはUTF-8であること")
        .to_string();
    let _temp = TempFile(path);

    // Act
    let output = Command::new(env!("CARGO_BIN_EXE_mesh"))
        .args(["build", &path_str])
        .output()
        .expect("meshバイナリを起動できること");

    // Assert
    // 一時ディレクトリのパスに数値が含まれうるため、E0101を含む行だけを取り出し
    // パス文字列を除去してから、行・列の数値を検証する(誤検知の防止)。
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr
        .lines()
        .filter(|line| line.contains("E0101"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(&path_str, "<file>");
    assert!(
        message.contains("E0101"),
        "E0101を含む行がstderrにあること: {stderr}"
    );
    assert!(
        message.contains('3'),
        "不正バイトの行番号(3)が含まれること: {message}"
    );
    assert!(
        message.contains("10"),
        "不正バイトの列番号(10)が含まれること: {message}"
    );
}
