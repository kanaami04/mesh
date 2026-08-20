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

/// 不正UTF-8のソースファイルを一時ファイルに書き、そのパス文字列を返す。
/// 戻り値のTempFileを呼び出し側が保持している間だけファイルが存在する。
/// ファイル名にプロセスIDと**呼び出し元のテスト名**を混ぜるのは、パスの奪い合いを防ぐため。
/// プロセスIDだけでは足りない——cargoは同一プロセス内でテストを並行実行するので、
/// 2つのテストが同じPIDで同じパスを掴み、片方の後始末がもう片方の読み取り前にファイルを消す
/// (impl-review 2026-08-20の指摘。worktreeの並行セッション間では実測で65ペア中2回失敗し、
/// 同一プロセス内では実際に `No such file or directory` で落ちた)。
/// **新しいテストを足すときは必ず別の `test_name` を渡すこと。**同じ文字列を渡すと
/// 同一プロセス内の並行実行で衝突が黙って復活する(impl-review 2026-08-20のメモ)。
fn write_invalid_utf8_source(test_name: &str) -> (String, TempFile) {
    // 3行目 `let x = 0`(9バイト)の直後に不正バイト 0xFF を置く。
    // 0xFF は3行目の10バイト目=1始まりの列10。
    let source: &[u8] = b"let a = 1\nlet b = 2\nlet x = 0\xFF";
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mesh-invalid-utf8-{}-{}.mesh",
        std::process::id(),
        test_name
    ));
    std::fs::write(&path, source).expect("一時ファイルに不正バイト列を書き込めること");
    let path_str = path
        .to_str()
        .expect("一時ファイルのパスはUTF-8であること")
        .to_string();
    (path_str, TempFile(path))
}

/// 不正UTF-8のソースファイルを与えたとき、CLIは位置(行・列)つきでE0101を報告すること
/// (仕様1章L-1〔負例: invalid-utf8〕)。ADR-0048によりこの検査は読み込み層の責務で、
/// 字句解析器のテストではなくCLIの振る舞いとして検証する。
/// 報告行は**全体を完全一致**で検証する。`contains("10")` は "E0101" 自体の部分文字列に
/// マッチしてしまうため恒真になる(impl-review 2026-08-20の指摘)。行・列とも1始まり
/// (エディタの表示と一致する形)で、列は文字数換え(バイト数でない)であることも
/// 文言ごとの一致で固定する。
#[test]
fn invalid_utf8_source_reports_e0101_with_position() {
    // Arrange
    let (path_str, _temp) = write_invalid_utf8_source("reports-position");

    // Act
    let output = Command::new(env!("CARGO_BIN_EXE_mesh"))
        .args(["build", &path_str])
        .output()
        .expect("meshバイナリを起動できること");

    // Assert
    // パスは実行環境のテンポラリディレクトリで変わるため、既知の値 `<file>` に
    // 正規化してから報告行全体を比較する。文言はstyle-guide原則4
    // (位置+原因+修正候補・責めないトーン)に従う。
    let stderr = String::from_utf8_lossy(&output.stderr).replace(&path_str, "<file>");
    assert_eq!(
        stderr.trim_end(),
        "エラー: <file>:3行10列: ソースをUTF-8として読めませんでした(E0101)。ファイルをUTF-8で保存し直してください"
    );
}

/// 不正UTF-8を報告したとき、CLIは失敗終了(終了コード1)すること(仕様1章L-1)。
/// エラー報告しながら成功終了する実装を検知する。
#[test]
fn invalid_utf8_source_exits_with_failure() {
    // Arrange
    let (path_str, _temp) = write_invalid_utf8_source("exits-with-failure");

    // Act
    let output = Command::new(env!("CARGO_BIN_EXE_mesh"))
        .args(["build", &path_str])
        .output()
        .expect("meshバイナリを起動できること");

    // Assert
    // ExitCode::FAILURE は終了コード1に対応する。
    assert_eq!(output.status.code(), Some(1));
}
