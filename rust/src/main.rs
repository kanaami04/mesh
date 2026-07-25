// Mesh CLI(Rust版)。milestone 35でTS版`src/cli.ts`のサブコマンド構成へ寄せた:
//   mesh run   <file.mesh> [args...]  コンパイルして即実行
//   mesh build <file.mesh> [-o out]   JavaScriptを書き出す
//   mesh check <file.mesh> [--json]   型検査のみ
//   mesh fmt   <file.mesh> [-w]        正規形へ整形して標準出力へ(-w で書き戻す)
//   mesh ast   <file.mesh>            パース結果のASTを表示(移植用のデバッグ支援)
//
// **full_checkerのゲート統合**(milestone 35): `run`/`build`/`check`のいずれも、codegenの前に
// full_checker(診断を出すchecker)を通す。診断があればソース行+`^`つきで**stderrへ**報告して
// 終了する(TS版`cli.ts`が`console.error(formatDiagnostics(...))`を使うのと同じ。stdoutは
// `check`の"no errors"と`--json`の出力専用)。
// full_checkerは1ファイルずつしか検査できないが、**モジュールを1本ずつ順に検査する**ことで
// importしたパッケージの中身も検査対象にしている(TS版と同じく依存パッケージ→エントリの順)。
// パッケージ間の参照は互いにANYへ潰れるので誤検知は出ない(検出漏れ側に倒れる)——
// full_checker自身を複数ファイル対応にするのは今後のmilestone。
//
// `run`は生成JSを一時ファイルへ書き、`bun`(無ければ`node`)で実行する。TS版が
// `process.execPath`で自分自身(bun)を使うのと同じ役割で、Rust版はJSランタイムを外部に
// 持つのでPATHから探す。
use mesh::codegen::{self, ModuleUnit};
use mesh::diagnostic_codes::Diagnostic;
use mesh::formatter;
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
  mesh fmt   <file.mesh> [-w]        format to the canonical form (-w rewrites the file)
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
        "run" | "build" | "check" | "fmt" | "ast" => match args.get(2) {
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
        "fmt" => run_fmt(file, rest.iter().any(|a| a == "-w")),
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

// パースまで到達できなかった理由。**表示は呼び出し元に任せる**——`check --json`は
// 構文エラーもJSONへ載せる必要があるため(TS版`compileModules`はパースエラーを型検査の
// 診断と同じ配列へ畳み込む。一方モジュール発見の失敗はTS版も素のエラー文なので同じにする)
enum LoadFailure {
    Discover(String),
    Parse { file: String, source: String, errors: Vec<CompileError> },
}

impl LoadFailure {
    // 非JSONの経路での表示(全てstderr)
    fn report(&self) {
        match self {
            LoadFailure::Discover(msg) => eprintln!("{msg}"),
            LoadFailure::Parse { file, source, errors } => eprint!("{}", format_compile_errors(file, errors, source)),
        }
    }
}

// エントリファイルと、そこからimportで辿れるパッケージを全部読んでパースする。
// 構文エラーはこの時点で報告して`Err`を返す(型検査まで進めない)
fn parse_all(file: &str) -> Result<ParsedModules, LoadFailure> {
    let sources = load_modules(Path::new(file)).map_err(LoadFailure::Discover)?;
    let mut units = Vec::with_capacity(sources.len());
    let mut source_map = HashMap::new();
    for m in &sources {
        let name = m.file.display().to_string();
        source_map.insert(name.clone(), m.source.clone());
        match parse(&m.source) {
            Ok(mut program) => {
                if let Err(e) = synthesize_json_decoders(&mut program) {
                    return Err(LoadFailure::Discover(format!("{name}: {e}")));
                }
                units.push(ModuleUnit { pkg: m.pkg.clone(), file: name, program });
            }
            Err(errors) => return Err(LoadFailure::Parse { file: name, source: m.source.clone(), errors }),
        }
    }
    Ok(ParsedModules { units, sources: source_map })
}

// 非JSON経路の共通処理: 失敗を表示して終了コードへ変換する
fn parse_all_reported(file: &str) -> Result<ParsedModules, ExitCode> {
    parse_all(file).map_err(|f| {
        f.report();
        ExitCode::FAILURE
    })
}

// 全モジュールをfull_checkerに通し、(ファイル, 診断)を**TS版と同じ順序**で返す。
// TS版`compileModules`は依存パッケージから順に検査するので、こちらも読み込み順
// (エントリ→依存)を逆にして依存側を先に出す。`fn main`の要求はmainパッケージだけ
// (importされたパッケージには普通mainが無い——`check_program_opts`のrequire_main)
fn check_all(parsed: &ParsedModules) -> Vec<(String, Vec<Diagnostic>)> {
    parsed
        .units
        .iter()
        .rev()
        .map(|u| (u.file.clone(), full_checker::check_program_opts(&u.program, u.pkg == "main")))
        .filter(|(_, diags)| !diags.is_empty())
        .collect()
}

// 診断をstderrへ(TS版`console.error(formatDiagnostics(...))`と同じ)
fn report(parsed: &ParsedModules, per_file: &[(String, Vec<Diagnostic>)]) {
    for (file, diags) in per_file {
        eprint!("{}", format_diagnostics(file, diags, parsed.sources.get(file).map(String::as_str)));
    }
}

// full_checkerのゲート。診断があれば報告して`Err`を返す
fn gate(parsed: &ParsedModules) -> Result<(), ExitCode> {
    let per_file = check_all(parsed);
    if per_file.is_empty() {
        return Ok(());
    }
    report(parsed, &per_file);
    Err(ExitCode::FAILURE)
}

fn compile_file(file: &str) -> Result<String, ExitCode> {
    let parsed = parse_all_reported(file)?;
    gate(&parsed)?;
    codegen::generate_modules(&parsed.units).map_err(|e| {
        eprintln!("{e}");
        ExitCode::FAILURE
    })
}

// ---- 各サブコマンド ----

fn run_ast(file: &str) -> ExitCode {
    match parse_all_reported(file) {
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
        // **`--json`のときは構文エラーもJSONへ載せる**(TS版`compileModules`はパースエラーを
        // 型検査の診断と同じ配列へ畳み込むので、`--json`の出力が常にJSONになる)。
        // これを怠ると、`mesh check --json`をパースするエージェントが構文エラーの
        // ときだけJSONパースに失敗する——`--json`は機械可読の契約なので実害がある
        // (code reviewで発覚。README/requirements.mdが自己修正ループの前提として明記)。
        // モジュール発見の失敗(存在しないパッケージ等)はTS版も素のエラー文なので揃える
        Err(LoadFailure::Parse { file: f, errors, .. }) if json => {
            let diags: Vec<JsonDiag> = errors
                .iter()
                .map(|e| JsonDiag { file: &f, line: e.pos.line, col: e.pos.col, code: e.code.to_string(), message: &e.message })
                .collect();
            println!("{}", json_report(file, &diags));
            return ExitCode::FAILURE;
        }
        Err(f) => {
            f.report();
            return ExitCode::FAILURE;
        }
    };
    if parsed.units.is_empty() {
        eprintln!("{file}: no modules");
        return ExitCode::FAILURE;
    }
    let per_file = check_all(&parsed);
    if json {
        // AIエージェント向けの構造化出力(TS版`diagnosticsToJson`)。トップレベルの`file`は
        // エントリ、各診断の`file`はそれが出たモジュール——TS版の`d.file ?? file`と同じ形。
        // **`fix`(機械適用可能な自動修正)はRust版がまだ持たないので出さない**——TS版は
        // 一部の診断(`use-is-none`の`== none`等)でfixを付けるので、その診断ではJSONの形が
        // TS版と揃わない。fixの移植は将来のmilestone
        println!("{}", diagnostics_to_json(file, &per_file));
        return if per_file.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    if per_file.is_empty() {
        println!("{file}: no errors");
        return ExitCode::SUCCESS;
    }
    report(&parsed, &per_file);
    ExitCode::FAILURE
}

// `mesh fmt`(TS版`cli.ts`のfmtケース)。**モジュールを辿らず対象ファイル1本だけを読む**
// ——整形はファイル単位の操作で、importの解決は要らない(TS版も`readFileSync`+`format`)。
// 構文エラーはソース行つきでstderrへ報告して終了コード1
fn run_fmt(file: &str, write: bool) -> ExitCode {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("error: cannot read file '{file}'");
            return ExitCode::FAILURE;
        }
    };
    let formatted = match formatter::format(&source) {
        Ok(f) => f,
        Err(errors) => {
            eprint!("{}", format_compile_errors(file, &errors, &source));
            return ExitCode::FAILURE;
        }
    };
    if write {
        if let Err(e) = std::fs::write(file, &formatted) {
            eprintln!("error: cannot write {file}: {e}");
            return ExitCode::FAILURE;
        }
    } else {
        print!("{formatted}");
    }
    ExitCode::SUCCESS
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
// JSON出力用の1件ぶん(full_checkerの診断とパーサのCompileErrorを同じ形へ揃える)
struct JsonDiag<'a> {
    file: &'a str,
    line: usize,
    col: usize,
    code: String,
    message: &'a str,
}

fn diagnostics_to_json(entry_file: &str, per_file: &[(String, Vec<Diagnostic>)]) -> String {
    let diags: Vec<JsonDiag> = per_file
        .iter()
        .flat_map(|(f, ds)| {
            ds.iter().map(move |d| JsonDiag { file: f, line: d.pos.line, col: d.pos.col, code: d.code.to_string(), message: &d.message })
        })
        .collect();
    json_report(entry_file, &diags)
}

fn json_report(entry_file: &str, diags: &[JsonDiag]) -> String {
    let items: Vec<String> = diags
        .iter()
        .map(|d| {
            format!(
                "    {{\n      \"file\": {},\n      \"line\": {},\n      \"col\": {},\n      \"severity\": \"error\",\n      \"code\": {},\n      \"message\": {}\n    }}",
                json_string(d.file),
                d.line,
                d.col,
                json_string(&d.code),
                json_string(d.message)
            )
        })
        .collect();
    let body = if items.is_empty() { "[]".to_string() } else { format!("[\n{}\n  ]", items.join(",\n")) };
    format!(
        "{{\n  \"file\": {},\n  \"ok\": {},\n  \"diagnostics\": {}\n}}",
        json_string(entry_file),
        diags.is_empty(),
        body
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
