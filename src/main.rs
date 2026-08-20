//! Mesh CLI。`mesh build <file.mesh>` で隣に .js を出力する。

use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
使い方: mesh build <ファイル.mesh>

コマンド:
  build   .mesh ファイルをコンパイルして、同じ場所に .js を出力します";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["build", file] => build(Path::new(file)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// 不正UTF-8検出位置の直前までのバイト列(UTF-8として妥当)を、行・列(ともに1始まり)に
/// 換算する(仕様1章L-1・ADR-0048)。行は `\n` の個数+1、列は最終行の文字数+1。
/// 文字数で数えるのはエディタの表示と一致させるため(バイト数だと日本語でずれる)。
fn line_column(valid_prefix: &str) -> (usize, usize) {
    let line = valid_prefix.matches('\n').count() + 1;
    let column = valid_prefix
        .rsplit('\n')
        .next()
        .map_or(0, |s| s.chars().count())
        + 1;
    (line, column)
}

fn build(path: &Path) -> ExitCode {
    if path.extension().and_then(|e| e.to_str()) != Some("mesh") {
        eprintln!(
            "エラー: 拡張子が .mesh のファイルを指定してください: {}",
            path.display()
        );
        return ExitCode::FAILURE;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("エラー: {} を読めません: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    // UTF-8検査は読み込み層の責務(仕様1章L-1・ADR-0048)。字句解析器は妥当なUTF-8を
    // 受け取る前提で組まれているため、不正バイトはここで位置つきのE0101として止める。
    let source = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(e) => {
            let valid = std::str::from_utf8(&bytes[..e.valid_up_to()])
                .expect("valid_up_toまでのバイト列はUTF-8として妥当であるため必ず成功する");
            let (line, column) = line_column(valid);
            eprintln!(
                "エラー: {}:{line}行{column}列: ソースをUTF-8として読めませんでした(E0101)。ファイルをUTF-8で保存し直してください",
                path.display()
            );
            return ExitCode::FAILURE;
        }
    };
    match mesh::compile(source) {
        Ok(js) => {
            let out = path.with_extension("js");
            if let Err(e) = std::fs::write(&out, js) {
                eprintln!("エラー: {} に書き込めません: {e}", out.display());
                return ExitCode::FAILURE;
            }
            println!("{} -> {}", path.display(), out.display());
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::line_column;

    /// 不正バイトがファイル先頭のとき(有効プレフィックスが空)は1行1列であること
    /// (仕様1章L-1。行・列とも1始まり)。列の+1を落とした実装を検知する。
    #[test]
    fn empty_prefix_is_line_1_column_1() {
        // Arrange
        let prefix = "";

        // Act
        let (line, column) = line_column(prefix);

        // Assert
        assert_eq!((line, column), (1, 1));
    }

    /// 末尾改行の直後(次の行頭)は2行1列であること(仕様1章L-1)。
    /// `lines()` は末尾の空行を返さないため、行を `lines().count()` で数える実装や
    /// 列を最終行の文字列から出す実装がここで破綻する。
    #[test]
    fn prefix_ending_with_newline_is_next_line_column_1() {
        // Arrange
        let prefix = "abc\n";

        // Act
        let (line, column) = line_column(prefix);

        // Assert
        assert_eq!((line, column), (2, 1));
    }

    /// 列はバイト数でなく文字数で数えること(仕様1章L-1。エディタの表示と一致させる)。
    /// `あい` は2文字6バイトのため、不正バイトの列は3(バイト数なら7)。
    #[test]
    fn column_counts_characters_not_bytes() {
        // Arrange
        let prefix = "あい";

        // Act
        let (line, column) = line_column(prefix);

        // Assert
        assert_eq!((line, column), (1, 3));
    }

    /// 列は最終行の長さから出すこと(1行目からでない)。`a`(1文字)と
    /// `bcdef`(5文字)の2行で、split()版(最初の行で数える)実装がここで破綻する。
    #[test]
    fn column_comes_from_last_line_not_first() {
        // Arrange
        let prefix = "a\nbcdef";

        // Act
        let (line, column) = line_column(prefix);

        // Assert
        assert_eq!((line, column), (2, 6));
    }
}
