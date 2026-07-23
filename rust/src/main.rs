// v1マイルストーン: lexer+parser+codegen(milestone 6・複数ファイル/パッケージ修飾まで)の
// 疎通確認CLI。checker/codegenがさらに育ったら`mesh run`/`build`/`check`相当に育てていく
// (`--emit-js`が無ければ、今まで通り各ファイルのパース結果のASTを整形表示するだけ)。
// 複数ファイル発見(エントリファイル+importされたパッケージのソース一式)はmodules::load_modules
// (TS版cli.tsのloadModules/loadDependencies相当)に委ねる
use mesh::codegen::{self, ModuleUnit};
use mesh::modules::load_modules;
use mesh::parser::parse;
use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: mesh <file.mesh> [--emit-js]");
        return ExitCode::FAILURE;
    };
    let emit_js = args.get(2).map(|a| a == "--emit-js").unwrap_or(false);

    let sources = match load_modules(Path::new(path)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let mut units = Vec::with_capacity(sources.len());
    for m in &sources {
        let file = m.file.display().to_string();
        match parse(&m.source) {
            Ok(program) => units.push(ModuleUnit { pkg: m.pkg.clone(), file, program }),
            Err(errors) => {
                for e in &errors {
                    eprintln!("{file}:{}:{}: {} [{}]", e.pos.line, e.pos.col, e.message, e.code);
                }
                return ExitCode::FAILURE;
            }
        }
    }

    if emit_js {
        match codegen::generate_modules(&units) {
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
        for u in &units {
            println!("{:#?}", u.program);
        }
        ExitCode::SUCCESS
    }
}
