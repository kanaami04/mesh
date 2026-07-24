// v1マイルストーン: lexer+parser+codegen(milestone 6・複数ファイル/パッケージ修飾まで)の
// 疎通確認CLI。checker/codegenがさらに育ったら`mesh run`/`build`相当に育てていく
// (`--emit-js`が無ければ、今まで通り各ファイルのパース結果のASTを整形表示するだけ)。
// 複数ファイル発見(エントリファイル+importされたパッケージのソース一式)はmodules::load_modules
// (TS版cli.tsのloadModules/loadDependencies相当)に委ねる。
//
// `mesh check <file.mesh>`(milestone 22)はfull_checker::check_programの疎通確認用の
// 最小実装——full_checkerの現状のスコープ(スカラーのMesh、import/パッケージ対象外)に
// 合わせて、load_modulesを介さず単一ファイルだけをそのままparseして検査する
use mesh::codegen::{self, ModuleUnit};
use mesh::full_checker;
use mesh::json_decode::synthesize_json_decoders;
use mesh::modules::load_modules;
use mesh::parser::parse;
use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.get(1).map(String::as_str) == Some("check") {
        return run_check(args.get(2));
    }

    let Some(path) = args.get(1) else {
        eprintln!("usage: mesh <file.mesh> [--emit-js]\n       mesh check <file.mesh>");
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
            Ok(mut program) => {
                if let Err(e) = synthesize_json_decoders(&mut program) {
                    eprintln!("{file}: {e}");
                    return ExitCode::FAILURE;
                }
                units.push(ModuleUnit { pkg: m.pkg.clone(), file, program });
            }
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

fn run_check(path_arg: Option<&String>) -> ExitCode {
    let Some(path) = path_arg else {
        eprintln!("usage: mesh check <file.mesh>");
        return ExitCode::FAILURE;
    };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut program = match parse(&source) {
        Ok(program) => program,
        Err(errors) => {
            for e in &errors {
                println!("{path}:{}:{}: error[{}]: {}", e.pos.line, e.pos.col, e.code, e.message);
            }
            return ExitCode::FAILURE;
        }
    };
    // codegen経路(上記main()内のsynthesize呼び出し)と同じく、check前にjson structの
    // デコーダを合成する。
    // これをやらないと`json struct`が生成する`decode*`関数をfull_checkerが知らず、
    // それを呼ぶ正当なコードが誤ってundefined-nameになる(examples/json_decode.mesh)
    if let Err(e) = synthesize_json_decoders(&mut program) {
        eprintln!("{path}: {e}");
        return ExitCode::FAILURE;
    }
    let diagnostics = full_checker::check_program(&program);
    if diagnostics.is_empty() {
        return ExitCode::SUCCESS;
    }
    for d in &diagnostics {
        println!("{path}:{}:{}: error[{}]: {}", d.pos.line, d.pos.col, d.code, d.message);
    }
    ExitCode::FAILURE
}
