// v1マイルストーン: lexer+parser+codegen(milestone 1のスカラーサブセット)の疎通確認CLI。
// checker/codegenがさらに育ったら`mesh run`/`build`/`check`相当に育てていく
// (`--emit-js`が無ければ、今まで通りパース結果のASTを整形表示するだけ)。
use mesh::codegen;
use mesh::parser::parse;
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: mesh <file.mesh> [--emit-js]");
        return ExitCode::FAILURE;
    };
    let emit_js = args.get(2).map(|a| a == "--emit-js").unwrap_or(false);
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match parse(&source) {
        Ok(program) => {
            if emit_js {
                match codegen::generate(&program, path) {
                    Ok(js) => {
                        print!("{js}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        ExitCode::FAILURE
                    }
                }
            } else {
                println!("{program:#?}");
                ExitCode::SUCCESS
            }
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("{}:{}:{}: {} [{}]", path, e.pos.line, e.pos.col, e.message, e.code);
            }
            ExitCode::FAILURE
        }
    }
}
