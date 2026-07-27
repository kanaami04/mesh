// Mesh CLI(Rust版)。milestone 35でTS版`src/cli.ts`のサブコマンド構成へ寄せた:
//   mesh run   <file.mesh> [args...]  コンパイルして即実行
//   mesh build <file.mesh> [-o out]   JavaScriptを書き出す
//   mesh check <file.mesh> [--json]   型検査のみ
//   mesh fmt   <file.mesh> [-w]        正規形へ整形して標準出力へ(-w で書き戻す)
//   mesh test  <file.mesh|dir> [--json] `_test.mesh`の`fn test...()`を実行する(F-15)
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
use mesh::ast::Program;
use mesh::card;
use mesh::codegen::{self, ModuleUnit};
use mesh::diagnostic_codes::Diagnostic;
use mesh::explain;
use mesh::formatter;
use mesh::full_checker;
use mesh::json_decode::synthesize_json_decoders;
use mesh::modules::{load_modules, load_modules_for_test};
use mesh::parser::parse;
use mesh::test_discovery;
use mesh::test_report::{self, TestReport};
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
  mesh test  <file.mesh|dir> [--json]  run 'fn test...()' in *_test.mesh files
  mesh explain <code>                explain a diagnostic code (no code: list them all)
  mesh card                          print the language card (compressed spec for AI context)
  mesh card --for <file.mesh>...     print only the sections the given sources use
  mesh ast   <file.mesh>             print the parsed AST (debug aid for the port)
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(command) = args.get(1).map(String::as_str) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let rest: Vec<String> = args.iter().skip(3).cloned().collect();

    // `card`/`explain`はファイルを取らない(第2引数の意味が他と違う)ので、
    // ファイル必須のディスパッチより手前で分岐する——TS版`cli.ts`と同じ構造
    match command {
        "card" => return run_card(args.get(2).map(String::as_str), &rest),
        "explain" => return run_explain(args.get(2).map(String::as_str)),
        _ => {}
    }

    match command {
        "run" | "build" | "check" | "fmt" | "test" | "ast" => match args.get(2) {
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

// `mesh card` / `mesh card --for <file.mesh>...`(F-13後半)。カード本文と絞り込みは
// card.rsが持つ(本文はTS版`src/card.ts`から取り出している)
fn run_card(arg: Option<&str>, rest: &[String]) -> ExitCode {
    if arg != Some("--for") {
        // `--for`以外の引数は無視する(TS版も同じ——`mesh card`は引数を取らない)
        println!("{}", card::language_card());
        return ExitCode::SUCCESS;
    }
    if rest.is_empty() {
        eprintln!("usage: mesh card --for <file.mesh> [<file2.mesh> ...]");
        return ExitCode::FAILURE;
    }
    let mut sources = Vec::new();
    for f in rest {
        match std::fs::read_to_string(f) {
            Ok(src) => sources.push(src),
            Err(_) => {
                eprintln!("error: cannot read file '{f}'");
                return ExitCode::FAILURE;
            }
        }
    }
    println!("{}", card::subset_card(&sources));
    ExitCode::SUCCESS
}

// `mesh explain <code>`(F-13前半)。引数無しなら一覧。
// **Rust版が出せる診断コードだけ**を扱う点でTS版と件数が違う(explain.rsの冒頭コメント)
fn run_explain(code: Option<&str>) -> ExitCode {
    let Some(code) = code else {
        let codes = explain::all_codes();
        println!("{} diagnostic codes. Run 'mesh explain <code>' for details.\n", codes.len());
        println!("{}", codes.join("\n"));
        return ExitCode::SUCCESS;
    };
    match explain::explanation(code) {
        Some(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("error: unknown diagnostic code '{code}' (run 'mesh explain' with no code to list them all)");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(command: &str, file: &str, rest: &[String]) -> ExitCode {
    match command {
        "ast" => run_ast(file),
        "check" => run_check(file, rest.iter().any(|a| a == "--json")),
        "fmt" => run_fmt(file, rest.iter().any(|a| a == "-w")),
        "test" => run_test(file, rest.iter().any(|a| a == "--json")),
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
    parse_modules(load_modules(Path::new(file)).map_err(LoadFailure::Discover)?)
}

// 読み込み済みのソース列をパースし、json structのデコーダを合成する。
// `mesh test`も含め**全サブコマンドがここを通る**(milestone 28で踏んだ
// 「`mesh check`だけ合成を忘れていた」というドリフトを構造的に防ぐ)
fn parse_modules(sources: Vec<mesh::modules::ModuleSource>) -> Result<ParsedModules, LoadFailure> {
    let mut units = Vec::with_capacity(sources.len());
    let mut source_map = HashMap::new();
    for m in &sources {
        let name = m.file.display().to_string();
        source_map.insert(name.clone(), m.source.clone());
        match parse(&m.source) {
            Ok(mut program) => {
                // **milestone 59**: json structの合成エラーは`CompileError`になったので、
                // パースエラーと同じ経路(位置・診断コード・ソース行つき)で報告する。
                // それまでは`String`で`LoadFailure::Discover`へ流しており、
                // `json-struct-missing-import`/`json-struct-unsupported-field`という
                // TS版の診断コードも位置も付いていなかった
                if let Err(e) = synthesize_json_decoders(&mut program) {
                    return Err(LoadFailure::Parse { file: name, source: m.source.clone(), errors: vec![*e] });
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
    check_all_opts(parsed, false)
}

// **パッケージ単位**で検査する(milestone 37)。同じパッケージの複数ファイルは1つの名前空間を
// 共有するので、まとめて`check_package`へ渡さないと「別ファイルの関数が未定義」という誤検知に
// なる(実際にmilestone 35のゲート統合以降、複数ファイルのパッケージで発生していた)。
// パッケージの並びはTS版と同じ「依存パッケージ→エントリ」(= 読み込み順の逆)。
// `test_mode`のときはmainパッケージにも`fn main`を要求しない(TS版のtestMode)
fn check_all_opts(parsed: &ParsedModules, test_mode: bool) -> Vec<(String, Vec<Diagnostic>)> {
    // パッケージごとにファイルをまとめる(パッケージの初出順を保つ)
    let mut discovered: Vec<String> = Vec::new();
    let mut by_pkg: HashMap<String, Vec<(String, &Program)>> = HashMap::new();
    for u in &parsed.units {
        if !discovered.contains(&u.pkg) {
            discovered.push(u.pkg.clone());
        }
        by_pkg.entry(u.pkg.clone()).or_default().push((u.file.clone(), &u.program));
    }
    // milestone 49: **全パッケージのシンボル表を先に組み立てる**(pkg修飾参照の診断用)。
    // 宣言を見るだけで作れるので検査順に依存せず、循環importがあっても素直に作れる。
    // 組み込みパッケージ(mesh/json等)を先に入れ、ユーザーパッケージで上書きする
    // ——TS版`stdlib.ts`のBUILTIN_PACKAGESもユーザー側と同じレジストリに載る
    let mut pkg_registry = full_checker::builtin_package_exports();
    for (pkg, files) in &by_pkg {
        pkg_registry.insert(pkg.clone(), full_checker::collect_package_exports(files));
    }
    let mut out = Vec::new();
    // メソッド表は全パッケージ共有(TS版`sharedMethods`)。struct名がpkg修飾済みなので衝突しない
    let mut shared = mesh::checker::CheckerCtx::new();
    for pkg in dependency_order(&discovered, &parsed.units) {
        let files = &by_pkg[&pkg];
        let require_main = pkg == "main" && !test_mode;
        let (diags, exports) = full_checker::check_package_shared(files, require_main, &pkg_registry, &pkg, &mut shared);
        // milestone 51: **検査し終えたパッケージの解決済みシンボル表をレジストリへ書き戻す**
        // (TS版`compileModules`が`registry`を持ち回るのと同じ)。依存順に回しているので、
        // あるパッケージを検査する時点で依存先の型は埋まっている
        pkg_registry.insert(pkg.clone(), exports);
        out.extend(diags);
    }
    out
}

// 依存されている側が先に来る順(TS版`checkModules`のDFS後順)。
// **単純な「読み込み順の逆」では足りない**——`main`が`a`と`c`をimportし`c`も`a`をimportする
// ような形(ダイヤモンド)では、読み込み順の逆が依存順にならず、診断の出る順序がTS版と
// 食い違う(code reviewで発覚)。循環があっても後順で確定した分から積むので停止する
fn dependency_order(discovered: &[String], units: &[ModuleUnit]) -> Vec<String> {
    let mut deps: HashMap<&str, Vec<String>> = HashMap::new();
    for u in units {
        deps.entry(u.pkg.as_str()).or_default().extend(u.program.imports.iter().map(|i| i.alias.clone()));
    }
    let known: Vec<&str> = discovered.iter().map(String::as_str).collect();
    let mut order: Vec<String> = Vec::new();
    let mut visiting: Vec<String> = Vec::new();
    fn visit(pkg: &str, deps: &HashMap<&str, Vec<String>>, known: &[&str], order: &mut Vec<String>, visiting: &mut Vec<String>) {
        if order.iter().any(|p| p == pkg) || visiting.iter().any(|p| p == pkg) {
            return; // 訪問済み or 循環(循環の報告はcodegen側の仕事)
        }
        visiting.push(pkg.to_string());
        for d in deps.get(pkg).into_iter().flatten() {
            // 組み込みパッケージ(mesh/json等)や未知の名前は飛ばす
            if known.contains(&d.as_str()) {
                visit(d, deps, known, order, visiting);
            }
        }
        visiting.retain(|p| p != pkg);
        order.push(pkg.to_string());
    }
    for pkg in &known {
        visit(pkg, &deps, &known, &mut order, &mut visiting);
    }
    order
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

// `mesh test`(F-15。TS版`cli.ts`のtestケース)。`_test.mesh`の中の`fn test...() none | error`を
// 集めて実行する。ファイルを渡せばそのファイル+同ディレクトリの`_test.mesh`、ディレクトリを
// 渡せばそのパッケージ自身(テストファイル込み)が対象(`modules::load_modules_for_test`)。
//
// 生成JSの末尾は`main()`ではなくテストハーネス(`__runTests`)になり、**最終行に結果のJSONを
// 1行**書く。テスト自身の`print()`出力がその前に混ざっていてもよい(最後の行だけ見る)
fn run_test(target: &str, json: bool) -> ExitCode {
    let sources = match load_modules_for_test(Path::new(target)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    // パース + json structのデコーダ合成(他のサブコマンドと同じ前処理を`parse_modules`で共有——
    // milestone 28で踏んだ「checkだけ合成を忘れる」ドリフトを構造的に防ぐため)
    let parsed = match parse_modules(sources) {
        Ok(p) => p,
        // **`--json`のときは構文エラーもJSONで返す**(milestone 35の`mesh check --json`と同じ
        // 契約。エージェントが機械的にパースするので、構文エラーのときだけ素のテキストに
        // なると自己修正ループが壊れる。code reviewで発覚)
        Err(LoadFailure::Parse { file: f, errors, .. }) if json => {
            let diags: Vec<JsonDiag> = errors
                .iter()
                .map(|e| JsonDiag { file: &f, line: e.pos.line, col: e.pos.col, code: e.code.to_string(), message: &e.message })
                .collect();
            println!("{}", json_report(target, &diags));
            return ExitCode::FAILURE;
        }
        Err(f) => {
            f.report();
            return ExitCode::FAILURE;
        }
    };

    // テストの発見と、型検査(**mainを要求しない**——テストだけのパッケージにmainは無い。
    // TS版の`testMode`と同じ)。診断はどちらもゲートとして扱う
    let mut tests = Vec::new();
    let mut per_file = check_all_opts(&parsed, true);
    // 発見にはそのファイルが属する**パッケージ全体の型宣言**を渡す(テストの戻り値
    // エイリアスは本体側のファイルで宣言されるのが普通なため)
    let mut types_by_pkg: HashMap<String, Vec<mesh::ast::TypeDecl>> = HashMap::new();
    for u in &parsed.units {
        types_by_pkg.entry(u.pkg.clone()).or_default().extend(u.program.types.iter().cloned());
    }
    for u in &parsed.units {
        let pkg_types = types_by_pkg.get(&u.pkg).map(Vec::as_slice).unwrap_or(&[]);
        let found = test_discovery::discover_in(&u.pkg, &u.file, &u.program, pkg_types);
        tests.extend(found.tests);
        if !found.diagnostics.is_empty() {
            match per_file.iter_mut().find(|(f, _)| f == &u.file) {
                Some((_, diags)) => diags.extend(found.diagnostics),
                None => per_file.push((u.file.clone(), found.diagnostics)),
            }
        }
    }
    if !per_file.is_empty() {
        if json {
            println!("{}", diagnostics_to_json(target, &per_file));
        } else {
            report(&parsed, &per_file);
        }
        return ExitCode::FAILURE;
    }

    let js = match codegen::generate_modules_for_test(&parsed.units, &tests) {
        Ok(js) => js,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    match run_test_harness(target, &js) {
        Ok(report) => print_test_report(target, &report, json),
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

// 生成したハーネスを実行し、最終行のJSONを読んで結果を返す
fn run_test_harness(target: &str, js: &str) -> Result<TestReport, String> {
    let Some(runtime) = ["bun", "node"].into_iter().find(|r| which(r)) else {
        return Err("error: テストを実行するには 'bun' か 'node' が必要です（PATHに見つかりません）".to_string());
    };
    let dir = env::temp_dir().join(format!("mesh-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("error: cannot create temp dir {}: {e}", dir.display()))?;
    let out_path = dir.join("test.mjs");
    std::fs::write(&out_path, js).map_err(|e| format!("error: cannot write {}: {e}", out_path.display()))?;
    let output = Command::new(runtime).arg(&out_path).output();
    let _ = std::fs::remove_dir_all(&dir);
    let output = output.map_err(|e| format!("error: cannot run {runtime}: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let last_line = stdout.trim_end().lines().next_back().unwrap_or("");
    let mut report = test_report::parse(last_line)
        .ok_or_else(|| format!("mesh test: internal error — could not read test results\n{stdout}{stderr}"))?;
    // 防衛的な二重チェック(TS版と同じ): ハーネス側の隔離ロジックに将来また穴があっても、
    // JSONの`ok`を盲信してプロセスの実際の終了コードと食い違うまま緑判定にはしない
    if report.ok && output.status.code() != Some(0) {
        report.ok = false;
        let code = output.status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".to_string());
        let mut message = format!("test process exited with code {code} despite reporting success");
        if !stderr.trim().is_empty() {
            message.push_str(&format!(" — stderr: {}", stderr.trim()));
        }
        report.tests.push(test_report::TestResult { name: "(process)".into(), file: target.to_string(), pass: false, message: Some(message) });
    }
    Ok(report)
}

fn print_test_report(target: &str, report: &TestReport, json: bool) -> ExitCode {
    if json {
        println!("{}", test_report::to_json(report));
    } else if report.tests.is_empty() {
        println!("no tests found (looked for 'fn test...() none | error' in *_test.mesh files)");
    } else {
        for t in &report.tests {
            println!("{} {} ({})", if t.pass { "ok  " } else { "FAIL" }, t.name, t.file);
            if !t.pass && let Some(m) = &t.message {
                println!("     {m}");
            }
        }
        let passed = report.tests.iter().filter(|t| t.pass).count();
        println!("\n{passed}/{} passed", report.tests.len());
    }
    let _ = target;
    if report.ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
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
