// v1マイルストーン: lexerだけの疎通確認CLI。parser/checker/codegenを移植したら
// `mesh run`/`build`/`check`相当に育てていく(今はトークン列を表示するだけ)。
use mesh::lexer::lex;
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
    match lex(&source, None) {
        Ok(out) => {
            for t in &out.tokens {
                println!("{:?} {:?} @{}:{}", t.kind, t.value, t.pos.line, t.pos.col);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}:{}:{}: {} [{}]", path, e.pos.line, e.pos.col, e.message, e.code);
            ExitCode::FAILURE
        }
    }
}
