// Mesh CLI(Rust版)。milestone 35でTS版`src/cli.ts`のサブコマンド構成へ寄せた:
//   mesh run   <file.mesh> [args...]  コンパイルして即実行
//   mesh build <file.mesh> [-o out]   JavaScriptを書き出す
//   mesh check <file.mesh> [--json]   型検査のみ
//   mesh ast   <file.mesh>            パース結果のASTを表示(移植用のデバッグ支援)
//
// **full_checkerのゲート統合**(milestone 35): `run`/`build`/`check`のいずれも、codegenの前に
// full_checker(診断を出すchecker)を通す。診断があればソース行+`^`つきで報告して終了する。
// full_checkerはまだ**エントリファイル1本だけ**を検査する(複数ファイル対応は今後の
// milestone)——importしたパッケージの中身は未検査だが、import aliasはANY扱いなので
// 誤検知は出ない(検出漏れ側に倒れる)。
//
// `run`は生成JSを一時ファイルへ書き、`bun`(無ければ`node`)で実行する。TS版が
// `process.execPath`で自分自身(bun)を使うのと同じ役割で、Rust版はJSランタイムを外部に
// 持つのでPATHから探す。
use mesh::codegen::{self, ModuleUnit};
use mesh::diagnostic_codes::Diagnostic;
use mesh::full_checker;
use mesh::json_decode::synthesize_json_decoders;
use mesh::modules::load_modules;
use mesh::parser::parse;
use mesh::token::CompileError;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "Mesh compiler (Rust port)

Usage:
  mesh run   <file.mesh> [args...]   compile and run
  mesh build <file.mesh> [-o out]    compile to JavaScript
  mesh check <file.mesh> [--json]    type-check only
  mesh ast   <file.mesh>             print the parsed AST (debug aid for the port)
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(command) = args.get(1).map(String::as_str) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let rest: Vec<String> = args.iter().skip(3).cloned().collect();

    match command {
        "run" | "build" | "check" | "ast" => match args.get(2) {
            Some(file) => dispatch(command, file, &rest),
            None => {
                eprintln!("{USAGE}");
                ExitCode::FAILURE
            }
        },
        // 旧来の`mesh <file.mesh> [--emit-js]`もしばらく受け付ける(移植中の手癖と、
        // docs/handoff.mdに載っている確認コマンドを壊さないため)
        file if file.ends_with(".mesh") => {
            let legacy_emit = args.get(2).map(|a| a == "--emit-js").unwrap_or(false);
            dispatch(if legacy_emit { "build-stdout" } else { "ast" }, file, &[])
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(command: &str, file: &str, rest: &[String]) -> ExitCode {
    match command {
        "ast" => run_ast(file),
        "check" => run_check(file, rest.iter().any(|a| a == "--json")),
        "build" | "build-stdout" | "run" => {
            let js = match compile_file(file) {
                Ok(js) => js,
                Err(code) => return code,
            };
            match command {
                "build-stdout" => {
                    print!("{js}");
                    ExitCode::SUCCESS
                }
                "build" => write_output(file, &js, rest),
                _ => run_js(file, &js, rest),
            }
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

// ---- パイプライン(load → parse → json struct合成 → full_checker → codegen) ----

struct ParsedModules {
    units: Vec<ModuleUnit>,
    // file → ソース。診断にソース行を添えるために保持する
    sources: HashMap<String, String>,
}

// エントリファイルと、そこからimportで辿れるパッケージを全部読んでパースする。
// 構文エラーはこの時点で報告して`Err`を返す(型検査まで進めない)
fn parse_all(file: &str) -> Result<ParsedModules, ExitCode> {
    let sources = match load_modules(Path::new(file)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return Err(ExitCode::FAILURE);
        }
    };
    let mut units = Vec::with_capacity(sources.len());
    let mut source_map = HashMap::new();
    for m in &sources {
        let name = m.file.display().to_string();
        source_map.insert(name.clone(), m.source.clone());
        match parse(&m.source) {
            Ok(mut program) => {
                if let Err(e) = synthesize_json_decoders(&mut program) {
                    eprintln!("{name}: {e}");
                    return Err(ExitCode::FAILURE);
                }
                units.push(ModuleUnit { pkg: m.pkg.clone(), file: name, program });
            }
            Err(errors) => {
                print!("{}", format_compile_errors(&name, &errors, &m.source));
                return Err(ExitCode::FAILURE);
            }
        }
    }
    Ok(ParsedModules { units, sources: source_map })
}

// full_checkerのゲート。診断があればソース行つきで報告し、`Err`を返す。
// **エントリファイル(units[0])だけ**を検査する——full_checkerは単一ファイル専用のため
fn gate(parsed: &ParsedModules) -> Result<(), ExitCode> {
    let Some(entry) = parsed.units.first() else { return Ok(()) };
    let diagnostics = full_checker::check_program(&entry.program);
    if diagnostics.is_empty() {
        return Ok(());
    }
    let source = parsed.sources.get(&entry.file).map(String::as_str);
    print!("{}", format_diagnostics(&entry.file, &diagnostics, source));
    Err(ExitCode::FAILURE)
}

fn compile_file(file: &str) -> Result<String, ExitCode> {
    let parsed = parse_all(file)?;
    gate(&parsed)?;
    codegen::generate_modules(&parsed.units).map_err(|e| {
        eprintln!("{e}");
        ExitCode::FAILURE
    })
}

// ---- 各サブコマンド ----

fn run_ast(file: &str) -> ExitCode {
    match parse_all(file) {
        Ok(parsed) => {
            for u in &parsed.units {
                println!("{:#?}", u.program);
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn run_check(file: &str, json: bool) -> ExitCode {
    let parsed = match parse_all(file) {
        Ok(p) => p,
        // --json でも構文エラーはparse_all側の表示に任せる(TS版も構文エラーは
        // compileEntry経由で同じ診断経路に乗るが、Rust版のパーサはまだ
        // DiagnosticCodeへ統合されていないため。統合は将来のmilestone)
        Err(code) => return code,
    };
    let Some(entry) = parsed.units.first() else {
        eprintln!("{file}: no modules");
        return ExitCode::FAILURE;
    };
    let diagnostics = full_checker::check_program(&entry.program);
    if json {
        // AIエージェント向けの構造化出力(TS版`diagnosticsToJson`と同じ形。
        // `fix`はRust版がまだ自動修正を持たないため出さない)
        println!("{}", diagnostics_to_json(&entry.file, &diagnostics));
        return if diagnostics.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    if diagnostics.is_empty() {
        println!("{file}: no errors");
        return ExitCode::SUCCESS;
    }
    let source = parsed.sources.get(&entry.file).map(String::as_str);
    print!("{}", format_diagnostics(&entry.file, &diagnostics, source));
    ExitCode::FAILURE
}

fn write_output(file: &str, js: &str, rest: &[String]) -> ExitCode {
    let out_path = match rest.iter().position(|a| a == "-o").and_then(|i| rest.get(i + 1)) {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(file.strip_suffix(".mesh").unwrap_or(file).to_string() + ".mjs"),
    };
    match std::fs::write(&out_path, js) {
        Ok(()) => {
            println!("wrote {}", out_path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: cannot write {}: {e}", out_path.display());
            ExitCode::FAILURE
        }
    }
}

// 生成JSを一時ファイルへ書いてJSランタイムで実行する。プログラム自身の引数
// (`io.args()`で読める)は`--`無しでそのまま後ろへ渡す(TS版と同じ)
fn run_js(file: &str, js: &str, rest: &[String]) -> ExitCode {
    let Some(runtime) = ["bun", "node"].into_iter().find(|r| which(r)) else {
        eprintln!("error: 生成したJavaScriptを実行するには 'bun' か 'node' が必要です（PATHに見つかりません）");
        return ExitCode::FAILURE;
    };
    let stem = Path::new(file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "main".to_string());
    // 一時ディレクトリはプロセスIDで一意にする（同時実行しても衝突しない）
    let dir = env::temp_dir().join(format!("mesh-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("error: cannot create temp dir {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    let out_path = dir.join(format!("{stem}.mjs"));
    if let Err(e) = std::fs::write(&out_path, js) {
        eprintln!("error: cannot write {}: {e}", out_path.display());
        return ExitCode::FAILURE;
    }
    let status = Command::new(runtime).arg(&out_path).args(rest).status();
    // 一時ファイルは実行後に片付ける（失敗しても実行結果の報告を優先する）
    let _ = std::fs::remove_dir_all(&dir);
    match status {
        Ok(s) => match s.code() {
            Some(c) => ExitCode::from(c.clamp(0, 255) as u8),
            // シグナルで終了した場合（panicのabort等）。0にすると成功に化けるので1にする
            None => ExitCode::FAILURE,
        },
        Err(e) => {
            eprintln!("error: cannot run {runtime}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn which(cmd: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|p| p.join(cmd).is_file()))
        .unwrap_or(false)
}

// ---- 診断の表示(TS版`compiler.ts`の`formatDiagnostics`/`diagnosticsToJson`の移植) ----

// 各診断を「見出し + ソース行 + `^`」の3行で出す。桁合わせの空白はタブをタブのまま残す
// (lexerがタブも1文字と数えるため、端末のタブ描画に委ねる——TS版と同じ)
fn format_diagnostics(file: &str, diagnostics: &[Diagnostic], source: Option<&str>) -> String {
    let mut out = String::new();
    for d in diagnostics {
        out.push_str(&format!("{file}:{}:{}: error[{}]: {}\n", d.pos.line, d.pos.col, d.code, d.message));
        if let Some(line) = source.and_then(|s| s.lines().nth(d.pos.line.saturating_sub(1))) {
            out.push_str(&format!("  {line}\n  {}^\n", caret_prefix(line, d.pos.col)));
        }
    }
    out
}

// パーサ/レクサのエラー(まだDiagnosticCodeへ統合されていない)も同じ見た目で出す
fn format_compile_errors(file: &str, errors: &[CompileError], source: &str) -> String {
    let mut out = String::new();
    for e in errors {
        out.push_str(&format!("{file}:{}:{}: error[{}]: {}\n", e.pos.line, e.pos.col, e.code, e.message));
        if let Some(line) = source.lines().nth(e.pos.line.saturating_sub(1)) {
            out.push_str(&format!("  {line}\n  {}^\n", caret_prefix(line, e.pos.col)));
        }
    }
    out
}

// `^`の位置合わせ用の前置き。タブ以外を空白に潰す(TS版の`replace(/[^\t]/g, " ")`)
fn caret_prefix(line: &str, col: usize) -> String {
    line.chars().take(col.saturating_sub(1)).map(|c| if c == '\t' { '\t' } else { ' ' }).collect()
}

// TS版`diagnosticsToJson`と同じ形・同じ整形(2スペースインデント。TS版は
// `JSON.stringify(..., null, 2)`)。手書きなのは、この1箇所のためにserdeを足すより
// 依存を増やさない方を選んだため(フィールドが増えたら見直す)。
// `fix`(機械適用可能な自動修正)はRust版がまだ持たないので出さない——TS版でも
// fixが無い診断ではキーごと省略されるため、形は互換
fn diagnostics_to_json(file: &str, diagnostics: &[Diagnostic]) -> String {
    let items: Vec<String> = diagnostics
        .iter()
        .map(|d| {
            format!(
                "    {{\n      \"file\": {},\n      \"line\": {},\n      \"col\": {},\n      \"severity\": \"error\",\n      \"code\": {},\n      \"message\": {}\n    }}",
                json_string(file),
                d.pos.line,
                d.pos.col,
                json_string(&d.code.to_string()),
                json_string(&d.message)
            )
        })
        .collect();
    let diags = if items.is_empty() { "[]".to_string() } else { format!("[\n{}\n  ]", items.join(",\n")) };
    format!(
        "{{\n  \"file\": {},\n  \"ok\": {},\n  \"diagnostics\": {}\n}}",
        json_string(file),
        diagnostics.is_empty(),
        diags
    )
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
