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

fn build(path: &Path) -> ExitCode {
    if path.extension().and_then(|e| e.to_str()) != Some("mesh") {
        eprintln!(
            "エラー: 拡張子が .mesh のファイルを指定してください: {}",
            path.display()
        );
        return ExitCode::FAILURE;
    }
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: {} を読めません: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match mesh::compile(&source) {
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
