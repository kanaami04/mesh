// v1マイルストーン: lexer+parser(実用サブセット)の疎通確認CLI。
// checker/codegenを移植したら`mesh run`/`build`/`check`相当に育てていく
// (今はパース結果のASTを整形表示するだけ)。
use mesh::parser::parse;
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: mesh <file.mesh>");
        return ExitCode::FAILURE;
    };
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match parse(&source) {
        Ok(program) => {
            println!("{program:#?}");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("{}:{}:{}: {} [{}]", path, e.pos.line, e.pos.col, e.message, e.code);
            }
            ExitCode::FAILURE
        }
    }
}
