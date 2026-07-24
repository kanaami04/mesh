// Codegen: 検査済み(このリゾルバの範囲で解決)のASTからJavaScriptを出力する。
// TS版`src/codegen.ts`(762行、`Codegen`クラス)の移植だが、milestone 2までの対象は
// 「スカラーのMesh」+ plain struct宣言/レシーバメソッドまで(判別可能union・match/is・
// error/jsonマーカー・配列/map・並行処理・パッケージ修飾は未対応。構文はパーサで既に
// パースできるが、まだ対応していない構文には明確なエラーを返す——コンパイラ自体を
// crashさせない。checker.rsのファイル冒頭コメントも参照)
//
// 設計の要(TS版から踏襲): Meshの関数はすべて`async function`として出力し、呼び出しは
// 常にawaitする。これにより将来`<-ch`(チャネル受信)を`await`に変換でき、Goの
// 「ブロックして待つ」をJSの「イベントループに譲って待つ」へ対応させられる
//
// **ランタイム(PRELUDE)の扱い**: TS版は`src/runtime.ts`のテンプレートリテラル文字列を
// 生成JSの先頭にそのまま埋め込むだけ(トランスパイル無し)。出力ターゲットは常にJSなので、
// Rust版も同じ`src/runtime.ts`を`include_str!`で読み込んで埋め込む(ランタイムの二重管理・
// 意味のズレを避ける。TS/Rustどちらのコンパイラが吐いたJSも同じランタイムで動く)

use crate::ast::{Block, ConstDecl, ElseClause, Expr, FnDecl, IfStmt, MatchPattern, Program, Receiver, SelectArm, Stmt, TypeDecl, TypeNode};
use crate::checker::{self, CheckerCtx};
use crate::token::{Pos, TokenType};
use crate::types::{self, INT, Type};
use std::collections::{HashMap, HashSet};

// src/runtime.ts自体はTSファイル(`export const PRELUDE = \`...\`;`というテンプレートリテラル
// 宣言でランタイムJSを包んでいる)なので、include_str!でファイル全体を埋め込むと
// TSの宣言部分(`export const PRELUDE = `や末尾の`;`)まで生成JSに混ざって壊れる。
// バッククォートの中身だけを取り出す(ファイル全体でバッククォートはこの2箇所にしか
// 現れない — ランタイム本体は文字列連結のみで書かれ、テンプレートリテラルを使っていない)
const RUNTIME_TS: &str = include_str!("../../src/runtime.ts");

fn prelude() -> String {
    let start = RUNTIME_TS.find('`').expect("runtime.ts should wrap PRELUDE in a template literal") + 1;
    let end = RUNTIME_TS.rfind('`').expect("runtime.ts should wrap PRELUDE in a template literal");
    // code review発覚・実行確認済みの回帰: ここは`RUNTIME_TS`(runtime.tsのソーステキストその
    // もの)からテンプレートリテラルの中身を素朴に部分文字列として取り出すだけで、JSの
    // テンプレートリテラル自身が持つエスケープ解決(`\\`→`\`等)を一切評価していなかった。
    // runtime.ts側は「正規表現の中でバックスラッシュ1つの意味で書きたい箇所は、外側の
    // テンプレートリテラルが先に1段エスケープを解決することを見込んで`\\`と2つ重ねて書く」
    // という前提で書かれている(`__toInt`の`\\d+`がその唯一の例)——TS版は実際にこの
    // テンプレートリテラルを評価するため`\d+`になるが、ここで単純な部分文字列抽出しか
    // していないRust版はソース上の`\\d+`をそのまま出力してしまい、`\\d`という(実質何にも
    // マッチしない)正規表現になって`toInt`が常に失敗していた。JSの`\\`エスケープだけを
    // 評価して埋め合わせる(runtime.ts全体を確認済みの結果、他のエスケープ〈`` \` ``や
    // `\$`〉はこのテンプレートリテラル内に存在しない)
    RUNTIME_TS[start..end].replace("\\\\", "\\")
}

pub type CodegenResult<T> = Result<T, String>;

pub fn generate(program: &Program, file: &str) -> CodegenResult<String> {
    // 1パッケージ("main")だけのmodulesリストを作ってgenerate_modulesを呼ぶ薄いラッパー。
    // 既存の単一ファイル呼び出し元(main.rs・既存の全テスト)は無変更で今まで通り動く
    generate_modules(&[ModuleUnit { pkg: "main".to_string(), file: file.to_string(), program: program.clone() }])
}

// 複数ファイル/複数パッケージのコンパイル(milestone 6)。TS版`compileModules`に相当するが、
// このリゾルバはcheck/generateが融合しているので2パスではなく1パスで済む
pub fn generate_modules(modules: &[ModuleUnit]) -> CodegenResult<String> {
    Codegen::new().generate_all_modules(modules)
}

// 1ファイルぶんのソース+それが属するパッケージ名。TS版`ModuleSource`/`ParsedModule`に相当
// (パースは呼び出し元が済ませておく——rust/src/modules.rsの`ModuleSource`〈未パース〉とは別物)
pub struct ModuleUnit {
    pub pkg: String,
    pub file: String,
    pub program: Program,
}

struct Codegen {
    out: Vec<String>,
    indent: usize,
    file: String,
    ctx: CheckerCtx,
    // 今生成中の関数本体のどこかで`?`が使われたか。TS版のpropStackに相当——milestone 10で
    // Expr::FnExprに対応し関数本体生成がネストしうるようになったため、単一フラグではなく
    // スタックにしてgen_fn_body呼び出し単位でpush/popする(外側の関数の使用状況を
    // 内側のFnExprが汚さない、逆も同様)
    prop_used: Vec<bool>,
    // 今生成中の関数本体のどこかで(detachではない)spawnが使われたか。TS版のspawnStackに
    // 相当——prop_usedと同じ理由でスタックにする
    spawn_used: Vec<bool>,
    // 今生成中の関数本体のどこかで`defer`が使われたか(milestone 11)。TS版のdeferStackに
    // 相当——prop_used/spawn_usedと同じ理由でスタックにする
    defer_used: Vec<bool>,
    // defer文が引数/レシーバの捕捉に使う一時変数(`__d0`,`__d1`,...)の連番。TS版
    // `deferTempCounter`と同じくコンパイル全体で1つ(関数ごとにはリセットしない)——
    // 全ての一時変数名がコンパイル全体で一意になればよく、関数内で閉じている必要は無い
    defer_temp_counter: u32,
    // これまでに生成したトップレベルconstの名前(全パッケージ分、リセットしない)。
    // code review指摘(milestone 6): トップレベル関数/メソッドはfn_js_name/method_js_nameで
    // pkg接頭辞が付き衝突しないが、constは(呼び出しを伴わない値参照が対象外のため)
    // 意図的に無修飾のまま生成している——2つのパッケージ(または同一パッケージの2ファイル)が
    // 同じ名前のトップレベルconstを宣言すると、生成JSの同じフラットスコープに同名の`const`
    // 宣言が2つ現れ、JS自体が構文エラーで一切パースできなくなる(実行時クラッシュより
    // 悪い、ファイル全体が起動不能になる)。これを静かに`Ok(js)`として返さず、重複を
    // 検出した時点で明確なErrにする
    declared_consts: HashSet<String>,
}

impl Codegen {
    fn new() -> Self {
        Codegen {
            out: Vec::new(),
            indent: 0,
            file: String::new(),
            ctx: CheckerCtx::new(),
            prop_used: Vec::new(),
            spawn_used: Vec::new(),
            defer_used: Vec::new(),
            defer_temp_counter: 0,
            declared_consts: HashSet::new(),
        }
    }

    fn emit(&mut self, line: impl Into<String>) {
        self.out.push("  ".repeat(self.indent) + &line.into());
    }

    // パニックメッセージに埋め込む位置情報: "main.mesh:3:8"。self.fileはgenerate_packageが
    // 生成対象のファイルを切り替えるたびに更新する(1パッケージが複数ファイルを含みうるため)
    fn at(&self, pos: Pos) -> String {
        format!("{:?}", format!("{}:{}:{}", self.file, pos.line, pos.col))
    }

    // パッケージごとにファイルをまとめ(同一パッケージ内はimport不要のフラット名前空間、
    // TS版compileModulesと同じ)、import依存グラフの依存順(importされる側が先)にソート
    // してから1パッケージずつ処理する
    fn generate_all_modules(&mut self, modules: &[ModuleUnit]) -> CodegenResult<String> {
        // 組み込みパッケージ(milestone 9・`mesh/json`)を、ユーザーパッケージの処理が
        // 始まる前に登録しておく(依存グラフのソート対象には現れない——
        // topo_sort_packagesはpackagesに無い名前への参照を無視するため無害)
        self.ctx.register_package("json", json_stdlib_symbols());

        let mut packages: Vec<(String, Vec<&ModuleUnit>)> = Vec::new();
        for m in modules {
            match packages.iter_mut().find(|(pkg, _)| pkg == &m.pkg) {
                Some((_, files)) => files.push(m),
                None => packages.push((m.pkg.clone(), vec![m])),
            }
        }

        let order = topo_sort_packages(&packages)?;

        self.out.push(prelude().trim_end().to_string());
        self.out.push(String::new());

        for pkg in &order {
            let files = &packages.iter().find(|(p, _)| p == pkg).expect("order only contains known packages").1;
            self.generate_package(pkg, files)?;
        }

        self.out.push("main().catch(__panic);".to_string());
        Ok(self.out.join("\n") + "\n")
    }

    // 1パッケージぶんの処理: struct解決→fn/メソッドのシグネチャ登録(前方参照・他ファイル
    // からの参照に対応するため本体生成より先に全ファイルぶん済ませる)→exportedシンボルを
    // registryへ確定登録(依存先パッケージが後で引けるように)→本体生成(ファイルごとに
    // self.fileを切り替える——1パッケージが複数ファイルを含みうるため、パニック位置情報が
    // 生成元のファイルを正しく指すようにする)
    fn generate_package(&mut self, pkg: &str, files: &[&ModuleUnit]) -> CodegenResult<()> {
        let mut import_aliases = HashSet::new();
        for f in files {
            for imp in &f.program.imports {
                import_aliases.insert(imp.alias.clone());
            }
        }
        self.ctx.begin_package(pkg, import_aliases);

        // plain struct宣言(json struct、milestone 9含む——is_jsonはdecode<X>自動生成
        // 〈json_decode.rs、main.rsでparse直後に済ませてある〉の対象を決めるだけで、
        // struct自体の型解決には影響しない)+ error struct宣言(milestone 3)+
        // 判別可能union型alias(`type X = A | B`、milestone 7)+ error typeのunion形式
        // (`error type X = A | B`、milestone 8)まで
        let all_types: Vec<TypeDecl> = files.iter().flat_map(|f| f.program.types.iter().cloned()).collect();
        for t in &all_types {
            if !matches!(t.node, TypeNode::StructType { .. } | TypeNode::Union { .. }) {
                return Err(format!("codegen: only plain struct declarations and union type aliases are supported so far (type '{}' at {}:{})", t.name, t.pos.line, t.pos.col));
            }
        }
        checker::resolve_type_decls(&mut self.ctx, &all_types)?;

        // 呼び出し先の戻り値型を解決できるよう、先に全トップレベル関数・メソッドのシグネチャを
        // (このパッケージの全ファイルぶん)登録してから本体を出力する
        for f in files {
            for fn_decl in &f.program.fns {
                if !fn_decl.type_params.is_empty() {
                    return Err(format!("codegen: generic functions are not yet supported (fn '{}' at {}:{})", fn_decl.name, fn_decl.pos.line, fn_decl.pos.col));
                }
                let params = fn_decl.params.iter().map(|p| checker::resolve_type_node(&self.ctx, &p.type_node)).collect();
                let ret = Box::new(checker::resolve_return_type(&self.ctx, &fn_decl.ret));
                match &fn_decl.receiver {
                    Some(recv) => {
                        let bare_name = receiver_struct_name(recv)?;
                        // レシーバが未宣言/非struct型(例: `fn (x: int) foo()`)なら殻へ静かに
                        // フォールバックさせず、明確なErrにする(おかしなJS関数名`__m_int_foo`等を
                        // 生成しないため)
                        if self.ctx.lookup_struct(&bare_name).is_none() {
                            return Err(format!(
                                "codegen: receiver type '{bare_name}' is not a declared struct (fn '{}' at {}:{})",
                                fn_decl.name, fn_decl.pos.line, fn_decl.pos.col
                            ));
                        }
                        // method_tableは全パッケージ共有なのでpkg修飾済みの名前で登録する
                        // (milestone 6——mainパッケージなら無修飾のまま、既存挙動と同じ)
                        let struct_name = checker::qualify_struct_name(pkg, &bare_name);
                        let mut all_params = vec![checker::resolve_type_node(&self.ctx, &recv.type_node)];
                        all_params.extend(params);
                        self.ctx.declare_method(&struct_name, &fn_decl.name, Type::Fn { params: all_params, ret });
                    }
                    None => self.ctx.declare_fn(&fn_decl.name, Type::Fn { params, ret }),
                }
            }
        }

        // このパッケージのexportedなstruct型/union型alias/自由関数をレジストリへ確定登録する
        // (依存先で処理される他パッケージが後から`alias.Name`で引けるように——パッケージは
        // 依存順に処理されるので、この時点で依存先は無くこのパッケージ自身の話だけでよい)。
        // union型alias(milestone 7)の登録漏れ(milestone 8のcode reviewで発覚・実行確認済み——
        // exportされたerror type union形式がパッケージ越しに構築できず明確なErrになり、
        // さらに`fn f() int | pkg.DbError`のようなpkg修飾された戻り値型注釈では
        // is_error_typeが付かない殻structへ静かにフォールバックしてhas_structured_failureの
        // 安全ガードごと素通りしてしまう、milestone 3の安全ガードの目的を回避する深刻な
        // バグだった)を修正——union型aliasも同じ`symbols.types`へ登録する
        // (`lookup_package_type`は型の種類を区別しないので、pkg修飾側の参照箇所は
        // 変更不要)。exportedなconstは今回のどの検証exampleも使わないため対象外のまま
        // (PackageSymbols.constsは常に空——将来pkg修飾constの読み出しに対応する際に埋める)
        let mut symbols = checker::PackageSymbols::default();
        for t in &all_types {
            if !t.exported {
                continue;
            }
            let resolved = match &t.node {
                TypeNode::StructType { .. } => self.ctx.lookup_struct(&t.name).cloned(),
                TypeNode::Union { .. } => self.ctx.lookup_union(&t.name).cloned(),
                _ => None,
            };
            if let Some(ty) = resolved {
                symbols.types.insert(t.name.clone(), ty);
            }
        }
        for f in files {
            for fn_decl in &f.program.fns {
                if fn_decl.exported
                    && fn_decl.receiver.is_none()
                    && let Some(ty) = self.ctx.lookup_fn(&fn_decl.name)
                {
                    symbols.fns.insert(fn_decl.name.clone(), ty.clone());
                }
            }
        }
        self.ctx.register_package(pkg, symbols);

        for f in files {
            self.file = f.file.clone();
            for c in &f.program.consts {
                self.gen_const_decl(c)?;
            }
        }
        for f in files {
            self.file = f.file.clone();
            for fn_decl in &f.program.fns {
                self.gen_fn_decl(fn_decl)?;
                self.out.push(String::new());
            }
        }
        Ok(())
    }

    fn gen_const_decl(&mut self, c: &ConstDecl) -> CodegenResult<()> {
        // code review指摘(milestone 6): トップレベル関数/メソッドはfn_js_name/method_js_name
        // でpkg接頭辞が付き衝突しないが、constは(呼び出しを伴わない値参照が対象外のため)
        // 意図的に無修飾のまま生成している——2つのパッケージ(または同一パッケージの2ファイル)が
        // 同じ名前のトップレベルconstを宣言すると、生成JSの同じフラットスコープに同名の
        // `const`宣言が2つ現れ、実行時クラッシュではなくJS自体がパースできない
        // (`SyntaxError: Identifier 'x' has already been declared`)という、ファイル全体が
        // 起動不能になるもっと悪い壊れ方をする。これを静かに`Ok(js)`として返さず、
        // 重複を検出した時点で明確なErrにする
        if !self.declared_consts.insert(c.name.clone()) {
            return Err(format!(
                "codegen: top-level const '{}' is declared more than once across the compiled packages ({}:{})",
                c.name, c.pos.line, c.pos.col
            ));
        }
        let value = self.gen_expr(&c.value)?;
        // 型注釈があればそちらが「本当の型」(TS版checker/modules.tsの`declared ?? valueType`)
        let ty = c.type_node.as_ref().map(|t| checker::resolve_type_node(&self.ctx, t)).unwrap_or_else(|| checker::infer_expr(&self.ctx, &c.value));
        self.ctx.declare(&c.name, ty);
        self.emit(format!("const {} = {value};", c.name));
        Ok(())
    }

    fn gen_fn_decl(&mut self, fn_decl: &FnDecl) -> CodegenResult<()> {
        let recv_params = fn_decl.receiver.as_ref().map(|r| r.name.as_str());
        let params =
            recv_params.into_iter().chain(fn_decl.params.iter().map(|p| p.name.as_str())).collect::<Vec<_>>().join(", ");
        let js_name = match &fn_decl.receiver {
            Some(recv) => {
                let struct_name = checker::qualify_struct_name(self.ctx.pkg(), &receiver_struct_name(recv)?);
                method_js_name(&struct_name, &fn_decl.name)
            }
            None => fn_js_name(self.ctx.pkg(), &fn_decl.name),
        };
        self.emit(format!("async function {js_name}({params}) {{"));
        self.ctx.push_scope();
        if let Some(recv) = &fn_decl.receiver {
            self.ctx.declare(&recv.name, checker::resolve_type_node(&self.ctx, &recv.type_node));
        }
        for p in &fn_decl.params {
            self.ctx.declare(&p.name, checker::resolve_type_node(&self.ctx, &p.type_node));
        }

        let body_result = self.gen_fn_body(&fn_decl.body.stmts);
        self.ctx.pop_scope();
        body_result?;
        self.emit("}");
        Ok(())
    }

    // 関数本体(FnDecl・Expr::FnExpr共通)を生成する: `?`/(detachでない)spawn/`defer`が
    // 本体のどこかに現れたかどうかは生成してみるまで分からない(if/forの中にネストして
    // いてもよい)ので、本体をいったん別バッファに生成してから、try/catch(?用)・
    // finally(spawn/defer用)で包むかどうかを事後に決める(TS版genFnBodyの
    // propStack/spawnStack/deferStackと同じ設計)。milestone 10でExpr::FnExprに対応し
    // 関数本体生成がネストしうるようになったため、prop_used/spawn_used/defer_usedは
    // スタックにしてこの呼び出し単位でpush/popする(外側の関数の使用状況を内側のFnExprが
    // 汚さない、逆も同様——無名関数の中のdeferはその無名関数自身を抜けるときに実行される)。
    // `self.out`/`self.indent`の意味(トップレベル関数なら直接追記、Expr::FnExprなら
    // 隔離済みバッファ)は呼び出し元が管理する——この関数は現在の`self.out`/`self.indent`
    // をそのまま使う
    fn gen_fn_body(&mut self, stmts: &[Stmt]) -> CodegenResult<()> {
        self.prop_used.push(false);
        self.spawn_used.push(false);
        self.defer_used.push(false);
        let saved_out = std::mem::take(&mut self.out);
        self.indent += 1;
        let body_result = self.gen_stmts(stmts);
        // indentはまだ+1のまま——try/catch/finallyの枠自体もこの深さ(関数の中、本体と同じ階層)に出す
        let body_lines = std::mem::replace(&mut self.out, saved_out);
        let used_prop = self.prop_used.pop().expect("pushed at the start of gen_fn_body");
        let used_spawn = self.spawn_used.pop().expect("pushed at the start of gen_fn_body");
        let used_defer = self.defer_used.pop().expect("pushed at the start of gen_fn_body");

        if used_prop || used_spawn || used_defer {
            if used_defer {
                self.emit("const __defers = [];");
            }
            if used_spawn {
                self.emit("__waitStack.push([]);");
            }
            self.emit("try {");
            for line in &body_lines {
                self.out.push(format!("  {line}")); // 本体行(indent+1で生成済み)をさらに1段深くする
            }
            if used_prop {
                self.emit("} catch (e) {");
                self.indent += 1;
                self.emit("if (e instanceof __Propagate) return e.value;");
                self.emit("throw e;");
                self.indent -= 1;
            }
            if used_spawn || used_defer {
                self.emit("} finally {");
                self.indent += 1;
                // spawnした子タスクを先に待ってから、自分のdeferを最後の後片付けとして走らせる
                if used_spawn {
                    self.emit("await Promise.all(__waitStack.pop());");
                }
                if used_defer {
                    self.emit("for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();");
                }
                self.indent -= 1;
            }
            self.emit("}");
        } else {
            self.out.extend(body_lines);
        }
        self.indent -= 1;
        body_result
    }

    fn gen_stmts(&mut self, stmts: &[Stmt]) -> CodegenResult<()> {
        for stmt in stmts {
            self.gen_stmt(stmt)?;
        }
        Ok(())
    }

    fn gen_block(&mut self, block: &Block) -> CodegenResult<()> {
        self.ctx.push_scope();
        self.indent += 1;
        self.gen_stmts(&block.stmts)?;
        self.indent -= 1;
        self.ctx.pop_scope();
        Ok(())
    }

    fn gen_stmt(&mut self, stmt: &Stmt) -> CodegenResult<()> {
        match stmt {
            Stmt::ShortVarDecl { names, values, mutable, pos } => {
                if names.len() != 1 || values.len() != 1 {
                    return Err(format!("codegen: multi-value declarations are not yet supported ({}:{})", pos.line, pos.col));
                }
                let kw = if *mutable { "let" } else { "const" };
                let value = self.gen_expr(&values[0])?;
                let ty = checker::infer_expr(&self.ctx, &values[0]);
                if names[0] == "_" {
                    self.emit(format!("{value};"));
                } else {
                    self.ctx.declare(&names[0], ty);
                    self.emit(format!("{kw} {} = {value};", names[0]));
                }
                Ok(())
            }
            Stmt::TypedVarDecl { name, type_node, value, mutable, .. } => {
                let kw = if *mutable { "let" } else { "const" };
                let js_value = self.gen_expr(value)?;
                self.ctx.declare(name, checker::resolve_type_node(&self.ctx, type_node));
                self.emit(format!("{kw} {name} = {js_value};"));
                Ok(())
            }
            Stmt::Assign { targets, values, compound_op, pos } => {
                if targets.len() != 1 || values.len() != 1 {
                    return Err(format!("codegen: multi-target assignment is not yet supported ({}:{})", pos.line, pos.col));
                }
                // 添字代入(`a[i] = v`/`m[k] = v`)はgen_lvalueに渡す前に横取りする——
                // targetがmapかarrayかで生成JSの形が丸ごと変わるため(gen_lvalue自身は
                // Indexターゲットに対応しない設計のまま。下のgen_index_assign参照)
                if let Expr::Index { target: container, index, pos: idx_pos } = &targets[0] {
                    return self.gen_index_assign(&targets[0], container, index, *idx_pos, *compound_op, &values[0]);
                }
                let lvalue = self.gen_lvalue(&targets[0])?;
                let rhs = self.gen_expr(&values[0])?;
                let value =
                    if let Some(op) = compound_op { self.gen_compound_value(*op, &targets[0], &lvalue, &values[0], &rhs, *pos)? } else { rhs };
                self.emit(format!("{lvalue} = {value};"));
                Ok(())
            }
            Stmt::ExprStmt { expr, .. } => {
                let js = self.gen_expr(expr)?;
                self.emit(format!("{js};"));
                Ok(())
            }
            Stmt::Return { value, .. } => {
                match value {
                    None => self.emit("return;"),
                    Some(v) => {
                        let js = self.gen_expr(v)?;
                        self.emit(format!("return {js};"));
                    }
                }
                Ok(())
            }
            Stmt::If(if_stmt) => self.gen_if(if_stmt),
            Stmt::For { init, cond, post, body, .. } => {
                self.ctx.push_scope();
                let init_js = init.as_deref().map(|s| self.gen_simple_stmt(s)).transpose()?.unwrap_or_default();
                let cond_js = cond.as_ref().map(|c| self.gen_expr(c)).transpose()?.unwrap_or_default();
                let post_js = post.as_deref().map(|s| self.gen_simple_stmt(s)).transpose()?.unwrap_or_default();
                self.emit(format!("for ({init_js}; {cond_js}; {post_js}) {{"));
                self.gen_block(body)?;
                self.emit("}");
                self.ctx.pop_scope();
                Ok(())
            }
            Stmt::IncDec { target, op, pos } => {
                if let Expr::Index { target: container, index, pos: idx_pos } = target {
                    return self.gen_index_incdec(container, index, *idx_pos, *op);
                }
                let target_ty = checker::infer_expr(&self.ctx, target);
                checker::check_inc_dec(*op, &target_ty, *pos)?;
                let lvalue = self.gen_lvalue(target)?;
                self.emit(format!("{lvalue}{op};"));
                Ok(())
            }
            Stmt::Break { .. } => {
                self.emit("break;");
                Ok(())
            }
            Stmt::Continue { .. } => {
                self.emit("continue;");
                Ok(())
            }
            // wait{}: 中でspawnされたタスクを全部待つ明示スコープ。TS版codegen.ts:322-332と
            // 同一構造で、中身にspawnがあるかどうかを見ずに無条件で包む(関数丸ごとの暗黙wait枠
            // 〈gen_fn_decl〉とは独立——__waitStackは本物のスタックなのでネストしても正しく動く)
            Stmt::Wait { body, .. } => {
                self.emit("__waitStack.push([]);");
                self.emit("try {");
                self.gen_block(body)?;
                self.emit("} finally {");
                self.indent += 1;
                self.emit("await Promise.all(__waitStack.pop());");
                self.indent -= 1;
                self.emit("}");
                Ok(())
            }
            // ch <- v。TS版のnot-a-channel診断に相当するRust版だけの安全ガード——診断を
            // 出さないこのリゾルバでは非chanへのsendが実際に到達しうるため(milestone 2〜4と
            // 同じ考え方)。確実に非chan/非anyだと分かる場合だけ弾く
            Stmt::Send { channel, value, pos } => {
                let ch_ty = checker::infer_expr(&self.ctx, channel);
                if !matches!(ch_ty, Type::Chan(_) | Type::Any) {
                    return Err(format!(
                        "codegen: cannot send to non-channel type '{}' ({}:{})",
                        types::type_to_string(&ch_ty), pos.line, pos.col
                    ));
                }
                let ch_js = self.gen_expr(channel)?;
                let val_js = self.gen_expr(value)?;
                self.emit(format!("(await {ch_js}.send({val_js}));"));
                Ok(())
            }
            Stmt::RangeFor { names, subject, body, pos } => self.gen_range_for(names, subject, body, *pos),
            Stmt::DeferStmt { call, pos } => self.gen_defer_stmt(call, *pos),
        }
    }

    // defer f(a, b) / defer recv.method(a)(milestone 11): 引数(メソッドならレシーバも)は
    // defer文を書いた時点の値で固定する(Goと同じ——mutな変数を後で書き換えても、
    // deferした呼び出しは古い値を見る)。呼び出し本体はgen_callの通常の分岐(パッケージ
    // 修飾/メソッド/組み込み/素の関数)をそのまま再利用したいので、レシーバ・引数を
    // 一時変数への参照に差し替えた「影武者」のcall式を作ってgen_callに渡す——呼び出し形の
    // 分岐ロジックを重複させずに済む(TS版genDeferStmtの移植)。パーサーは任意の式を
    // deferの後ろに許す(callであることの検証はここの仕事、ast.rs参照)ので、Callでなければ
    // 診断を出さないこのリゾルバでは明確なErrにする(TS版のdefer-requires-call診断に相当)
    fn gen_defer_stmt(&mut self, call: &Expr, pos: Pos) -> CodegenResult<()> {
        // code review発覚・実行確認済みの回帰: 影武者call式にはdefer文自体の`pos`ではなく
        // 元のcall式自身の`pos`を使う(TS版genDeferStmtの`{ ...call, ... }`と同じ——
        // calleeの解決不能エラーやgen_call内部のpanic位置情報が、defer文の位置ではなく
        // 元の呼び出し式自身の位置を指すようにする)
        let Expr::Call { callee, args, pos: call_pos } = call else {
            return Err(format!(
                "codegen: 'defer' must be followed by a function or method call, e.g. 'defer f(x)' ({}:{})",
                pos.line, pos.col
            ));
        };

        let mut assigns: Vec<String> = Vec::new();

        // メソッド呼び出しなら(recv.method(...)、gen_callの既存のメソッド判定と同じ——
        // struct型かつ同名フィールドが無い)、レシーバもdefer時点の値で固定する。
        // 影武者のIdentが指す一時変数は、TS版なら`resolvedType`をノードへ直接埋め込む
        // だけで済む(checkerが別パスで済んでいるため)が、このリゾルバはchecker/codegenが
        // 融合していて`gen_call`が`self.ctx`を都度引くため、一時変数の型を`self.ctx`へも
        // 宣言しておかないと(例えば)`gen_call`自身のメソッド判定がANY扱いになってしまう
        let invoke_callee: Expr = if let Expr::Member { target, name, pos: member_pos } = &**callee {
            let target_ty = checker::infer_expr(&self.ctx, target);
            let is_method = matches!(&target_ty, Type::Struct { fields, .. } if !fields.iter().any(|f| &f.name == name));
            if is_method {
                let recv_js = self.gen_expr(target)?;
                let recv_temp = format!("__d{}", self.defer_temp_counter);
                self.defer_temp_counter += 1;
                assigns.push(format!("const {recv_temp} = {recv_js};"));
                self.ctx.declare(&recv_temp, target_ty);
                Expr::Member { target: Box::new(Expr::Ident { name: recv_temp, pos: *member_pos }), name: name.clone(), pos: *member_pos }
            } else {
                (**callee).clone()
            }
        } else {
            (**callee).clone()
        };

        let mut invoke_args = Vec::with_capacity(args.len());
        for a in args {
            let arg_ty = checker::infer_expr(&self.ctx, a);
            let arg_js = self.gen_expr(a)?;
            let temp = format!("__d{}", self.defer_temp_counter);
            self.defer_temp_counter += 1;
            assigns.push(format!("const {temp} = {arg_js};"));
            self.ctx.declare(&temp, arg_ty);
            invoke_args.push(Expr::Ident { name: temp, pos: a.pos() });
        }

        let shadow_call = Expr::Call { callee: Box::new(invoke_callee), args: invoke_args, pos: *call_pos };
        let invoke_js = self.gen_call(&shadow_call)?;

        *self.defer_used.last_mut().expect("inside a function body") = true;
        self.emit("{");
        self.indent += 1;
        for a in assigns {
            self.emit(a);
        }
        self.emit(format!("__defers.push(async () => {{ {invoke_js}; }});"));
        self.indent -= 1;
        self.emit("}");
        Ok(())
    }

    // `if x is T { ... }`という単純形(condが裸Identに対する`is`)ならnarrowing(milestone 7)
    // を適用する——TS版のnarrowing.ts/statements.tsと同じ目的で、codegen側の型依存判断
    // (`__iarith`等)を正しくするためだけに必要(生成JSの「形」自体は変えない、
    // milestone 5のselect/orElseの束縛パターンと同じ設計)。`&&`/`||`/`!`との複合条件は
    // 対象外のまま(実際のexampleに複合条件のnarrowingは存在しない)
    fn gen_if(&mut self, if_stmt: &IfStmt) -> CodegenResult<()> {
        if let Expr::Is { operand, target, .. } = &if_stmt.cond
            && let Expr::Ident { name, .. } = &**operand
        {
            let subject_ty = checker::infer_expr(&self.ctx, operand);
            let (then_ty, else_ty) = checker::narrow_for_is(&self.ctx, &subject_ty, target);
            let cond = self.gen_expr(&if_stmt.cond)?;
            self.emit(format!("if ({cond}) {{"));
            self.ctx.push_scope();
            self.ctx.declare(name, then_ty);
            self.gen_block(&if_stmt.then)?;
            self.ctx.pop_scope();
            match if_stmt.else_.as_deref() {
                None => {
                    self.emit("}");
                    // then節が必ず終端する(return/break/continue)なら、else側の絞り込みを
                    // 現在のスコープへ反映し、後続の同ブロック内の文が引き継げるようにする
                    // (`if v is closed { break } total = total + v`でvが正しくintと
                    // narrowされ__iarithを使うために必須)
                    if block_always_terminates(&if_stmt.then) {
                        self.ctx.declare(name, else_ty);
                    }
                }
                Some(ElseClause::Block(block)) => {
                    self.emit("} else {");
                    self.ctx.push_scope();
                    self.ctx.declare(name, else_ty.clone());
                    self.gen_block(block)?;
                    self.ctx.pop_scope();
                    self.emit("}");
                    // then節が必ず終端するなら、if/elseの後に到達できるのはelse経由だけ
                    // なので、else側の絞り込みをここでも現在のスコープへ反映する
                    // (else節が無い場合の直上の分岐と同じ理由)
                    if block_always_terminates(&if_stmt.then) {
                        self.ctx.declare(name, else_ty);
                    }
                }
                // else ifチェーンはnarrowing対象外のまま(実際のexampleに存在しない組み合わせ)
                Some(other) => self.gen_else(Some(other))?,
            }
            return Ok(());
        }
        let cond = self.gen_expr(&if_stmt.cond)?;
        self.emit(format!("if ({cond}) {{"));
        self.gen_block(&if_stmt.then)?;
        self.gen_else(if_stmt.else_.as_deref())
    }

    fn gen_else(&mut self, else_: Option<&ElseClause>) -> CodegenResult<()> {
        match else_ {
            None => {
                self.emit("}");
                Ok(())
            }
            Some(ElseClause::If(if_stmt)) => {
                let cond = self.gen_expr(&if_stmt.cond)?;
                self.emit(format!("}} else if ({cond}) {{"));
                self.gen_block(&if_stmt.then)?;
                self.gen_else(if_stmt.else_.as_deref())
            }
            Some(ElseClause::Block(block)) => {
                self.emit("} else {");
                self.gen_block(block)?;
                self.emit("}");
                Ok(())
            }
        }
    }

    // `for i, v := range arr` / `for k, v := range m` / `for i := range n`。subjectの型で
    // 3形態に分岐する(TS版codegen.tsのrangeForケースと同じJS形)。
    // **アリティ不一致は明確なErr**: TS版のcodegen自体は無条件分岐なので、Array+1名だと
    // 「数値と配列を比較し続けて0回で終わるループ」を、Int+2名だと`.entries is not a
    // function`のクラッシュを静かに/クラッシュで生成してしまう——これはTS本体では
    // range-arity診断で到達不能な組み合わせ。診断を出さないこのリゾルバでは実際に
    // 到達しうるため、ここで明確なErrにする(Anyのsubjectは元々ANY型の一般的な限界であり、
    // 今回新たに導入するものではないので対象外のまま)
    fn gen_range_for(&mut self, names: &[String], subject: &Expr, body: &Block, pos: Pos) -> CodegenResult<()> {
        let subject_ty = checker::infer_expr(&self.ctx, subject);
        let is_array_or_map = matches!(subject_ty, Type::Array(_) | Type::Map { .. });
        let is_int = types::type_equals(&subject_ty, &INT);
        if (is_array_or_map && names.len() != 2) || (is_int && names.len() != 1) {
            return Err(format!(
                "codegen: range-for over {} expects {} name(s), got {} ({}:{})",
                types::type_to_string(&subject_ty),
                if is_int { 1 } else { 2 },
                names.len(),
                pos.line,
                pos.col
            ));
        }
        let subject_js = self.gen_expr(subject)?;
        self.ctx.push_scope();
        checker::declare_range_for_names(&mut self.ctx, &subject_ty, names);
        let js_names: Vec<String> = names.iter().map(|n| if n == "_" { String::new() } else { n.clone() }).collect();
        if matches!(subject_ty, Type::Map { .. }) {
            self.emit(format!("for (const [{}] of {subject_js}) {{", js_names.join(", ")));
        } else if names.len() == 1 {
            let i = if js_names[0].is_empty() { "__i" } else { &js_names[0] };
            self.emit(format!("for (let {i} = 0, __n = {subject_js}; {i} < __n; {i}++) {{"));
        } else {
            self.emit(format!("for (const [{}] of {subject_js}.entries()) {{", js_names.join(", ")));
        }
        self.gen_block(body)?;
        self.emit("}");
        self.ctx.pop_scope();
        Ok(())
    }

    // forヘッダ内のinit/post用: セミコロン無しの1行表現にする。TS版のgenSimpleStmtと同じく
    // ヘッダ変数は常にletで出す(Cスタイルforのヘッダ変数はデフォルト不変の唯一の構造的例外
    // ——TS版checkerが`mutable = true`を強制するのと同じ結果を、ここでは`mutable`を見ずに
    // 常にletを出すことで実現している)
    fn gen_simple_stmt(&mut self, stmt: &Stmt) -> CodegenResult<String> {
        match stmt {
            Stmt::ShortVarDecl { names, values, pos, .. } => {
                if names.len() != 1 || values.len() != 1 {
                    return Err(format!("codegen: multi-value declarations are not yet supported ({}:{})", pos.line, pos.col));
                }
                let value = self.gen_expr(&values[0])?;
                let ty = checker::infer_expr(&self.ctx, &values[0]);
                self.ctx.declare(&names[0], ty);
                Ok(format!("let {} = {value}", names[0]))
            }
            Stmt::TypedVarDecl { name, type_node, value, .. } => {
                let js_value = self.gen_expr(value)?;
                self.ctx.declare(name, checker::resolve_type_node(&self.ctx, type_node));
                Ok(format!("let {name} = {js_value}"))
            }
            Stmt::Assign { targets, values, compound_op, pos } => {
                if targets.len() != 1 || values.len() != 1 {
                    return Err(format!("codegen: multi-target assignment is not yet supported ({}:{})", pos.line, pos.col));
                }
                let lvalue = self.gen_lvalue(&targets[0])?;
                let rhs = self.gen_expr(&values[0])?;
                let value =
                    if let Some(op) = compound_op { self.gen_compound_value(*op, &targets[0], &lvalue, &values[0], &rhs, *pos)? } else { rhs };
                Ok(format!("{lvalue} = {value}"))
            }
            Stmt::IncDec { target, op, pos } => {
                let target_ty = checker::infer_expr(&self.ctx, target);
                checker::check_inc_dec(*op, &target_ty, *pos)?;
                let lvalue = self.gen_lvalue(target)?;
                Ok(format!("{lvalue}{op}"))
            }
            Stmt::ExprStmt { expr, .. } => self.gen_expr(expr),
            other => Err(format!("codegen: unsupported statement in for-header ({}:{})", other_pos(other).line, other_pos(other).col)),
        }
    }

    // F-9b: 複合代入(x += 1等)の右辺値を組み立てる。current_codeは代入先の「今の値」を
    // 読むコード片(二項演算式と同じくint_div/int_mod/int_arithフラグでpanic層のヘルパを挟む)
    fn gen_compound_value(&self, op: TokenType, target: &Expr, current_code: &str, value_expr: &Expr, rhs: &str, pos: Pos) -> CodegenResult<String> {
        let info = checker::infer_binary(&self.ctx, op, target, value_expr, pos)?;
        let at = self.at(pos);
        if info.int_div {
            return Ok(format!("__idiv({current_code}, {rhs}, {at})"));
        }
        if info.int_mod {
            return Ok(format!("__imod({current_code}, {rhs}, {at})"));
        }
        if info.int_arith {
            return Ok(format!("__iarith({current_code}, \"{op}\", {rhs}, {at})"));
        }
        Ok(format!("({current_code} {op} {rhs})"))
    }

    // 添字代入(`a[i] = v`/`m[k] = v`)。targetの型(Map/Array)で生成JSの形が丸ごと変わるため
    // gen_lvalueより前段でこの専用ヘルパへ振り分ける(gen_lvalue自体はIndexターゲットに
    // 対応しない設計のまま——forヘッダ内での添字代入は引き続き明確なErrになる)。
    // index_exprは複合代入時にinfer_binaryへ渡す「対象式全体」(gen_compound_valueが
    // infer_expr(target)で正しくelem型を引けるようにするため、containerとindexを
    // 分解する前のExpr::Index自体を渡す)
    fn gen_index_assign(
        &mut self,
        index_expr: &Expr,
        container: &Expr,
        index: &Expr,
        pos: Pos,
        compound_op: Option<TokenType>,
        value_expr: &Expr,
    ) -> CodegenResult<()> {
        let container_ty = checker::infer_expr(&self.ctx, container);
        // code review指摘(PR #19): map<K, map<...>>のようなネストしたmapを読むと`V | none`
        // になり(Expr::Indexのinfer_expr参照)、`matches!(container_ty, Type::Map{..})`という
        // 厳密一致だけではUnionをすり抜けて配列扱い(__idxset)になってしまう——TS版の
        // checker(src/checker/expressions.ts)はそもそもUnion型への添字を`not-indexable`
        // 診断で拒否しており(noneかもしれない値へさらに添字を続けるのは`or`/`is none`で
        // 絞り込んでからでないと安全でないため)、それに倣い明確なErrにする
        if let Type::Union { .. } = container_ty {
            return Err(format!(
                "codegen: cannot index into '{}' — narrow away 'none' first (e.g. with 'or') ({}:{})",
                types::type_to_string(&container_ty), pos.line, pos.col
            ));
        }
        let container_js = self.gen_expr(container)?;
        let index_js = self.gen_expr(index)?;
        let rhs = self.gen_expr(value_expr)?;
        if matches!(container_ty, Type::Map { .. }) {
            // TS版の`compound-assign-on-map`診断と同じ理由で複合代入は明確なErrにする——
            // mapの「今の値」は`V | none`であり、noneに対する算術は無意味(診断を出さない
            // このリゾルバでは、ここで拾わないと__mgetが返すnullに__iarith等を適用する
            // 壊れたJSを静かに生成してしまう)
            if compound_op.is_some() {
                return Err(format!(
                    "codegen: compound assignment on a map entry is not yet supported (the current value may be none) ({}:{})",
                    pos.line, pos.col
                ));
            }
            self.emit(format!("{container_js}.set({index_js}, {rhs});"));
            return Ok(());
        }
        let at = self.at(pos);
        let current_code = format!("__idx({container_js}, {index_js}, {at})");
        let value = match compound_op {
            Some(op) => self.gen_compound_value(op, index_expr, &current_code, value_expr, &rhs, pos)?,
            None => rhs,
        };
        self.emit(format!("__idxset({container_js}, {index_js}, {value}, {at});"));
        Ok(())
    }

    // 添字のインクリメント/デクリメント(`a[i]++`等)。TS版のcodegen自体はarray/map問わず
    // 無条件で__idx/__idxsetを使うが、それは`m[k]++`がTS本体のisNumericチェック
    // (map読みは常に`V | none`)で弾かれ実際には到達しないコードだから安全なだけ——
    // 診断を出さないこのリゾルバでは実際に到達しうるため、mapは明確なErrにする
    fn gen_index_incdec(&mut self, container: &Expr, index: &Expr, pos: Pos, op: TokenType) -> CodegenResult<()> {
        let container_ty = checker::infer_expr(&self.ctx, container);
        // gen_index_assignと同じ理由(ネストしたmapの読みは`V | none`のUnionになり、
        // 厳密一致のMapチェックをすり抜けてしまうため、Unionは明確なErrにする)
        if let Type::Union { .. } = container_ty {
            return Err(format!(
                "codegen: cannot index into '{}' — narrow away 'none' first (e.g. with 'or') ({}:{})",
                types::type_to_string(&container_ty), pos.line, pos.col
            ));
        }
        if matches!(container_ty, Type::Map { .. }) {
            return Err(format!("codegen: increment/decrement of a map entry is not yet supported ({}:{})", pos.line, pos.col));
        }
        // milestone 13・git historyレビュー指摘・実行確認済み: 配列要素の型がint/float
        // でない場合(例: `bools := [true, false]; bools[0]++`)は、check_arith_opと
        // 同じ`invalid-operation`診断が要る——以前はここに検査が無く、JSの暗黙の
        // bool→number変換で意味不明な値が静かに書き込まれてしまっていた
        if let Type::Array(elem) = &container_ty {
            checker::check_inc_dec(op, elem, pos)?;
        }
        let container_js = self.gen_expr(container)?;
        let index_js = self.gen_expr(index)?;
        let at = self.at(pos);
        let delta = match op {
            TokenType::PlusPlus => "+",
            TokenType::MinusMinus => "-",
            _ => unreachable!("IncDec op is always ++ or --"),
        };
        self.emit(format!("__idxset({container_js}, {index_js}, (__idx({container_js}, {index_js}, {at}) {delta} 1), {at});"));
        Ok(())
    }

    // 代入・インクリメント/デクリメント対象(lvalue)のJSコード片を組み立てる。Identはそのまま、
    // Memberはフィールド書き込み(`target.name`)。`__proto__`はTS版が過去に実際に踏んだ
    // prototype汚染バグ(struct literalのフィールドと同じ攻撃面が、フィールドの直接代入
    // `u.__proto__ = ...`という新しい経路でも起こりうる)の再発防止として明確なErrにする。
    // Index等(配列/mapは対象外のまま)は他の対応構文が無いのでこの後の default アームで弾く
    fn gen_lvalue(&mut self, expr: &Expr) -> CodegenResult<String> {
        match expr {
            Expr::Ident { name, .. } => Ok(name.clone()),
            Expr::Member { target, name, pos } => {
                if name == "__proto__" {
                    return Err(format!("codegen: '__proto__' cannot be used as a field name ({}:{})", pos.line, pos.col));
                }
                // targetがstruct型だと分かる場合だけフィールド書き込みとして許す——パッケージ
                // 修飾(`math.x = ...`)はまだ実装が無く「未解決の識別子」としてANYへ落ちるので、
                // ここで弾かないと`math.x = ...`のような実行時ReferenceErrorになるJSを
                // 静かに生成してしまう
                let target_ty = checker::infer_expr(&self.ctx, target);
                if !matches!(target_ty, Type::Struct { .. }) {
                    return Err(format!("codegen: package/member access is not yet supported ({}:{})", pos.line, pos.col));
                }
                // milestone 15・PR #17以来の既知の限界を解消: 代入先のフィールド名も
                // struct literal構築(milestone 12)と同じくtypoを検出する
                // (以前は`u.nmae = ...`のようなtypoが無診断でコンパイルされ、JSの
                // 新規プロパティとして黙って書き込まれていた)
                checker::validate_struct_field(&target_ty, name, &self.ctx, *pos)?;
                let target_js = self.gen_expr(target)?;
                Ok(format!("{target_js}.{name}"))
            }
            other => Err(format!("codegen: assignment to this kind of target is not yet supported ({}:{})", other.pos().line, other.pos().col)),
        }
    }

    // ---- 式 ----

    fn gen_expr(&mut self, expr: &Expr) -> CodegenResult<String> {
        match expr {
            Expr::Int { value, .. } | Expr::Float { value, .. } => Ok(value.clone()),
            Expr::String { value, .. } => Ok(format!("{value:?}")),
            Expr::Interp { segments, .. } => {
                let mut pieces = Vec::new();
                for seg in segments {
                    match seg {
                        crate::ast::InterpSegment::Text { text } => pieces.push(format!("{text:?}")),
                        crate::ast::InterpSegment::Expr { expr } => pieces.push(format!("__fmt({})", self.gen_expr(expr)?)),
                    }
                }
                if !matches!(segments.first(), Some(crate::ast::InterpSegment::Text { .. })) {
                    pieces.insert(0, "\"\"".to_string());
                }
                Ok(format!("({})", pieces.join(" + ")))
            }
            Expr::Bool { value, .. } => Ok(value.to_string()),
            Expr::None { .. } => Ok("null".to_string()), // noneの実行時表現はnull
            Expr::Ident { name, .. } => Ok(name.clone()),
            // milestone 14 code review発覚・実行確認済みの回帰: `x is int && x > 0`のような
            // 複合条件は、右辺(および右辺の中でさらに算術/比較する式)の型検査・コード生成の
            // 両方に左辺のnarrowing結果を反映しないと、TS版でテスト済みの正当なコード
            // (F-6: `&&`は左のisが右辺に効く、De Morganで`||`はelse側)が誤ってErrになる。
            // check_logical_op自身の内部スクラッチctxはinfer_expr経由の推論だけを守り
            // (Errを飲み込むため無害)、ここではcodegenが右辺を実際に生成する際に
            // self.ctxそのものを一時的に絞り込む——gen_ifの単純な`if x is T {...}`と
            // 同じnarrowing技法(push_scope/declare/pop_scope)をここでも使う。
            // 単純なidentオペランドのみ対応(gen_ifと同じ範囲、多段フィールドパス等は対象外)
            Expr::Binary { op, left, right, pos } if matches!(op, TokenType::AndAnd | TokenType::OrOr) => {
                let l = self.gen_expr(left)?;
                let popped = if let Expr::Is { operand, target, .. } = left.as_ref()
                    && let Expr::Ident { name, .. } = operand.as_ref()
                {
                    let subject_ty = checker::infer_expr(&self.ctx, operand);
                    let (then_ty, else_ty) = checker::narrow_for_is(&self.ctx, &subject_ty, target);
                    self.ctx.push_scope();
                    self.ctx.declare(name, if *op == TokenType::AndAnd { then_ty } else { else_ty });
                    true
                } else {
                    false
                };
                checker::infer_binary(&self.ctx, *op, left, right, *pos)?;
                let r = self.gen_expr(right)?;
                if popped {
                    self.ctx.pop_scope();
                }
                Ok(format!("({l} {} {r})", return_op_str(op)))
            }
            Expr::Binary { op, left, right, pos } => {
                let info = checker::infer_binary(&self.ctx, *op, left, right, *pos)?;
                let l = self.gen_expr(left)?;
                let r = self.gen_expr(right)?;
                let at = self.at(*pos);
                if info.int_div {
                    return Ok(format!("__idiv({l}, {r}, {at})"));
                }
                if info.int_mod {
                    return Ok(format!("__imod({l}, {r}, {at})"));
                }
                if info.int_arith {
                    return Ok(format!("__iarith({l}, \"{op}\", {r}, {at})"));
                }
                let js_op = match op {
                    TokenType::EqEq => "===",
                    TokenType::NotEq => "!==",
                    other => return_op_str(other),
                };
                Ok(format!("({l} {js_op} {r})"))
            }
            // milestone 13: 単項`-`はcheck_arith_opと同じ`invalid-operation`診断を共有する
            // ため妥当性検査する。milestone 14・code review発覚: `!`も兄弟演算子として
            // 同じ`not-bool`診断を共有するため、`&&`/`||`のnot-bool検査実装時に見落とさず
            // あわせて検査する(§計画参照)
            Expr::Unary { op, operand, pos } => {
                let operand_ty = checker::infer_expr(&self.ctx, operand);
                if *op == TokenType::Minus {
                    checker::check_unary_minus(&operand_ty, *pos)?;
                } else if *op == TokenType::Bang {
                    checker::check_logical_not(&operand_ty, *pos)?;
                }
                Ok(format!("({op}{})", self.gen_expr(operand)?))
            }
            Expr::Call { .. } => self.gen_call(expr),
            // is式(milestone 7): 裸Identならそのまま参照して二重評価を避け、それ以外は
            // 一度だけ評価して束縛するIIFE(TS版と同じ——struct形パターンはoperandを
            // 複数回参照しうるため)
            Expr::Is { operand, target, .. } => {
                if let Expr::Ident { name, .. } = &**operand {
                    Ok(gen_type_test(name, target))
                } else {
                    let operand_js = self.gen_expr(operand)?;
                    Ok(format!("((__v) => {})({operand_js})", gen_type_test("__v", target)))
                }
            }
            // match式(milestone 7): TS版と同じ三項演算子の連鎖に、subjectを渡すIIFEで包む
            // (`(await (async (__m) => test1 ? body1 : ... : lastBody)(subject))`)。
            // 各アームの本体は、subjectが裸Identの場合だけそのアームのパターン集合で
            // 絞り込んだ型を一時的に同じ名前で再宣言してから生成する(checker::infer_exprの
            // Matchアームと同じロジック)。**exhaustiveでない場合だけ**(Rust版だけの安全
            // ガード——TS本体はexhaustiveness診断で「どのアームにも一致しない値」を到達
            // 不能にするが、診断を出さないこのリゾルバでは実際に到達しうる)最後のアームにも
            // 明確なテストを付け、どれにも一致しない場合の明確なランタイムpanicを追加する
            Expr::Match { subject, arms, pos } => {
                let subject_ty = checker::infer_expr(&self.ctx, subject);
                let exhaustive = checker::match_is_exhaustive(&self.ctx, &subject_ty, arms);
                let bare_ident = if let Expr::Ident { name, .. } = &**subject { Some(name.clone()) } else { None };
                let subject_js = self.gen_expr(subject)?;

                let mut parts: Vec<String> = Vec::with_capacity(arms.len());
                for (i, arm) in arms.iter().enumerate() {
                    let is_last = i == arms.len() - 1;
                    let body_js = if let Some(name) = &bare_ident {
                        let narrowed = checker::narrow_for_match_patterns(&self.ctx, &subject_ty, &arm.patterns);
                        self.ctx.push_scope();
                        self.ctx.declare(name, narrowed);
                        let js = self.gen_expr(&arm.body)?;
                        self.ctx.pop_scope();
                        js
                    } else {
                        self.gen_expr(&arm.body)?
                    };
                    if is_last && exhaustive {
                        parts.push(body_js);
                    } else {
                        let test = arm
                            .patterns
                            .iter()
                            .map(|p| match p {
                                MatchPattern::Wildcard { .. } => "true".to_string(),
                                MatchPattern::Type(node) => gen_type_test("__m", node),
                            })
                            .collect::<Vec<_>>()
                            .join(" || ");
                        parts.push(format!("{test} ? {body_js} :"));
                    }
                }
                if !exhaustive {
                    let at = self.at(*pos);
                    parts.push(format!("(() => {{ throw new __Panic({at} + \": no arm of this match matched\"); }})()"));
                }
                Ok(format!("(await (async (__m) => {})({subject_js}))", parts.join(" ")))
            }
            // 裸のメンバーアクセス(呼び出しではない)。targetがstruct型かつnameが宣言済み
            // フィールドのときだけ素の`.name`を出す——パッケージ修飾(`math.add`)はまだ
            // 実装が無く「未解決の識別子」としてANYへ落ちるので、ここで弾かないと
            // 実行時ReferenceErrorになるJSを静かに生成してしまう。メソッド名(フィールドでは
            // ない名前)を値として参照する式もTS版と同じく対象外のまま(呼び出し式側でだけ解決)。
            // milestone 15: フィールド未検出時に無条件で「メソッド名の値参照」と決め打って
            // いた(method_tableを実際には見ていなかった)ため、単なるtypoでも常に
            // 誤解を招くメッセージになっていた——validate_struct_fieldで実際に
            // method_tableを確認し、真にメソッド名ならmethod-not-called相当、
            // そうでなければunknown-field相当のメッセージを出し分ける
            Expr::Member { target, name, pos } => {
                let target_ty = checker::infer_expr(&self.ctx, target);
                if !matches!(target_ty, Type::Struct { .. }) {
                    return Err(format!("codegen: package/member access is not yet supported ({}:{})", pos.line, pos.col));
                }
                checker::validate_struct_field(&target_ty, name, &self.ctx, *pos)?;
                Ok(format!("{}.{name}", self.gen_expr(target)?))
            }
            // 生成JSにはstruct名自体は現れない(TS版と同じ、プレーンなobject literal)。
            // error struct(error type X = ...で宣言されたstruct)のインスタンスだけ
            // __errTagで実行時マーカーを付ける(TS版のexpr.isErrorInstanceと同じ判定を
            // ctx.lookup_structの結果から行う)
            Expr::StructLit { name, pkg, fields, pos } => {
                // pkg修飾(`mathutil.Point{...}`、milestone 6)ならパッケージのレジストリから
                // 引く——未import/未exportなら実行時にプロパティが噛み合わない壊れたJSを
                // 静かに生成せず、ここで明確なErrにする
                let base = match pkg {
                    // code review指摘: import宣言していないパッケージ名でも(別経路で
                    // ロードされてさえいれば)レジストリを直接引けてしまっていた——
                    // is_package_aliasで実際にインポートされていることも確認する
                    // (依存グラフの循環検出がimport文だけを見て構築されるため、import文を
                    // 経由しないパッケージ間参照を許すと循環検出をすり抜けられてしまう)
                    Some(alias) if self.ctx.is_package_alias(alias) => {
                        let Some(ty) = self.ctx.lookup_package_type(alias, name) else {
                            return Err(format!("codegen: package '{alias}' has no exported struct '{name}' ({}:{})", pos.line, pos.col));
                        };
                        ty.clone()
                    }
                    Some(alias) => {
                        return Err(format!("codegen: package '{alias}' has no exported struct '{name}' ({}:{})", pos.line, pos.col));
                    }
                    // pkg無し(milestone 12): 未宣言の型名は既存のresolve_type_node/infer_exprと
                    // 同じ「空フィールドの殻structへフォールバック」——それ自体は落とさず、
                    // フィールド検証の方で自然にunknown-field/missing-fieldsとして顕在化させる
                    None => self
                        .ctx
                        .lookup_struct(name)
                        .or_else(|| self.ctx.lookup_union(name))
                        .cloned()
                        .unwrap_or_else(|| Type::Struct { name: name.clone(), fields: vec![], is_error_type: false }),
                };
                // milestone 16: pkg修飾側もフィールド名/型/判別可能unionのdisambiguationを
                // 実際に検証するようにした(以前はmilestone 8の"all"ヒューリスティック
                // 〈type_is_error_instance〉でerrTagの要否だけ見ており、フィールド自体は
                // 一切検証していなかった——他パッケージのunion構造をここで持たない制約が
                // あると説明していたが、実際は`lookup_package_type`が解決済みの完全な
                // `Type`〈fields/discriminant_tag/is_error_type込み〉を返すため技術的な
                // 制約ではなく、単にスコープを広げすぎないための意図的な選択だったと
                // milestone 12のcode reviewで訂正済み。§計画参照)。
                // TS版`structLit`の`displayName`計算(union構築ならexpr自身が書いた素の
                // 名前、名前付きstruct構築なら解決済みの型自身の〈pkg修飾済みの〉名前)を
                // そのまま再現する——pkg無し側は元々main packageの名前が無修飾なので
                // この2つが一致しており区別不要だったが、pkg修飾側では区別が必要
                let display_name = if matches!(base, Type::Union { .. }) { name.clone() } else { types::type_to_string(&base) };
                let field_types: Vec<Type> = fields.iter().map(|f| checker::infer_expr(&self.ctx, &f.value)).collect();
                let member = checker::resolve_struct_lit_member(&base, &display_name, fields, &field_types, *pos)?;
                checker::validate_struct_lit_fields(&member, &display_name, fields, &field_types, *pos)?;
                let is_error_instance = matches!(member, Type::Struct { is_error_type: true, .. });
                let mut js_fields = Vec::with_capacity(fields.len());
                for f in fields {
                    if f.name == "__proto__" {
                        return Err(format!("codegen: '__proto__' cannot be used as a field name ({}:{})", f.pos.line, f.pos.col));
                    }
                    js_fields.push(format!("{}: {}", f.name, self.gen_expr(&f.value)?));
                }
                let obj = format!("{{ {} }}", js_fields.join(", "));
                Ok(if is_error_instance { format!("__errTag({obj})") } else { format!("({obj})") })
            }
            // <-ch。Sendと同じ理由のRust版だけの安全ガード(確実に非chan/非anyだと
            // 分かる場合だけ弾く)
            Expr::Recv { channel, pos } => {
                let ch_ty = checker::infer_expr(&self.ctx, channel);
                if !matches!(ch_ty, Type::Chan(_) | Type::Any) {
                    return Err(format!(
                        "codegen: cannot receive from non-channel type '{}' ({}:{})",
                        types::type_to_string(&ch_ty), pos.line, pos.col
                    ));
                }
                Ok(format!("(await __recv({}))", self.gen_expr(channel)?))
            }
            // chan<T>(cap): F-11によりcapacityは常に必須。'none'(無制限)は__Channelを
            // 引数無しで呼ぶことに落とす(引数省略時はInfinity扱い。runtime.ts参照)
            Expr::Chan { capacity, .. } => match &**capacity {
                Expr::None { .. } => Ok("new __Channel()".to_string()),
                other => Ok(format!("new __Channel({})", self.gen_expr(other)?)),
            },
            Expr::Spawn { call, detached, pos } => self.gen_spawn(call, *detached, *pos),
            Expr::Select { arms, default_arm, pos } => self.gen_select(arms, default_arm.as_deref(), *pos),
            // f()? / f() ? "context" — 失敗(none/error/構造化error)なら呼び出し元へ即座に
            // 伝播する。関数本体側の対応(try/catchで包むか)はgen_fn_declが本体生成後に決める
            Expr::Prop { operand, context, pos } => {
                let operand_ty = checker::infer_expr(&self.ctx, operand);
                if context.is_some() && checker::has_structured_failure(&operand_ty) {
                    // ランタイムの__propCtxはnull/instanceof Errorしか特別扱いせず、構造化error
                    // (__ERRタグ付きのstruct)は素通りして「成功扱い」になってしまう
                    // (checker.rsのhas_structured_failureのコメント参照)——診断を出さない
                    // このリゾルバでは、ここで明確なErrにしないと実行時に静かに壊れる
                    return Err(format!(
                        "codegen: '?' with a context message cannot propagate a structured error (error struct) — use plain '?' instead ({}:{})",
                        pos.line, pos.col
                    ));
                }
                let operand_js = self.gen_expr(operand)?;
                *self.prop_used.last_mut().expect("inside a function body") = true;
                match context {
                    Some(ctx_expr) => {
                        let ctx_js = self.gen_expr(ctx_expr)?;
                        Ok(format!("(await __propCtx({operand_js}, async () => {ctx_js}))"))
                    }
                    None => Ok(format!("__prop({operand_js})")),
                }
            }
            // f() or fallback / f() or e => fallback — 失敗なら(遅延評価の)fallbackの値。
            // 束縛形はfallback式のスコープ内だけ`e`を失敗メンバーの型で見えるようにする
            Expr::OrElse { left, right, binding, .. } => {
                let left_ty = checker::infer_expr(&self.ctx, left);
                let left_js = self.gen_expr(left)?;
                let bind_name = binding.as_deref().filter(|n| *n != "_");
                self.ctx.push_scope();
                if let Some(name) = bind_name {
                    self.ctx.declare(name, checker::or_binding_type(&left_ty));
                }
                let right_js = self.gen_expr(right)?;
                self.ctx.pop_scope();
                Ok(format!("(await __or({left_js}, async ({}) => {right_js}))", bind_name.unwrap_or("")))
            }
            // elem_typeはcodegenでは一切参照しない(TS版と同じく構文のみ——素のJS配列リテラル)
            Expr::ArrayLit { elems, .. } => {
                let js_elems = elems.iter().map(|e| self.gen_expr(e)).collect::<CodegenResult<Vec<_>>>()?;
                Ok(format!("[{}]", js_elems.join(", ")))
            }
            Expr::MapLit { entries, .. } => {
                if entries.is_empty() {
                    return Ok("new Map()".to_string());
                }
                let mut js_entries = Vec::with_capacity(entries.len());
                for e in entries {
                    js_entries.push(format!("[{}, {}]", self.gen_expr(&e.key)?, self.gen_expr(&e.value)?));
                }
                Ok(format!("new Map([{}])", js_entries.join(", ")))
            }
            // 添字読み: targetがMap型なら`__mget`(欠損キーはnone)、それ以外は`__idx`
            // (配列・文字列どちらも`.length`/`[i]`を持つのでこのまま使える。範囲外はpanic)
            Expr::Index { target, index, pos } => {
                let target_ty = checker::infer_expr(&self.ctx, target);
                // code review指摘(PR #19): ネストしたmap(例: map<string, map<string,int>>)を
                // 読むとtargetの型が`V | none`のUnionになり、`Type::Map`との厳密一致では
                // すり抜けて配列扱い(`__idx`)になってしまう。TS版のchecker(src/checker/
                // expressions.ts)はUnion型への添字自体を`not-indexable`診断で拒否しており
                // (noneかもしれない値へ安全に添字を続けられないため)、それに倣い明確なErrにする
                if let Type::Union { .. } = target_ty {
                    return Err(format!(
                        "codegen: cannot index into '{}' — narrow away 'none' first (e.g. with 'or') ({}:{})",
                        types::type_to_string(&target_ty), pos.line, pos.col
                    ));
                }
                let target_js = self.gen_expr(target)?;
                let index_js = self.gen_expr(index)?;
                if matches!(target_ty, Type::Map { .. }) {
                    Ok(format!("__mget({target_js}, {index_js})"))
                } else {
                    Ok(format!("__idx({target_js}, {index_js}, {})", self.at(*pos)))
                }
            }
            // 無名関数式: fn(x: int) int { return x * 2 }。値として使われる式なので、
            // gen_stmtのように`self.out`へ直接追記せず、隔離したバッファへ生成してから
            // 呼び出し元の式の一部として埋め込む文字列にする(TS版codegen.tsのfnExpr
            // ケースと同じトリック——一旦indent=0にリセットして生成し、出来上がった
            // 各行の先頭に呼び出し元の実際のindentを後付けで足す)。本体の生成
            // (?/spawnの使用有無に応じたtry/catch/finally包み)自体はgen_fn_body
            // (FnDeclと共通)を再利用する
            Expr::FnExpr { params, body, .. } => {
                let params_js = params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
                self.ctx.push_scope();
                for p in params {
                    let ty = checker::resolve_type_node(&self.ctx, &p.type_node);
                    self.ctx.declare(&p.name, ty);
                }
                let saved_out = std::mem::take(&mut self.out);
                let saved_indent = self.indent;
                self.indent = 0;
                let body_result = self.gen_fn_body(&body.stmts);
                let body_lines = std::mem::replace(&mut self.out, saved_out);
                self.indent = saved_indent;
                self.ctx.pop_scope();
                body_result?;
                let pad = "  ".repeat(self.indent);
                // code review発覚・実行確認済みの回帰: `body_lines`の各要素へ`pad`を
                // 1回ずつ足すだけだと、その要素自体が(さらにネストしたExpr::FnExprの
                // 出力を埋め込んでいるなどの理由で)改行を内包している場合、2行目以降が
                // 余分なindentを受け取れない。TS版codegen.tsのfnExprケースと同じく、
                // 一旦全体を1つの文字列に結合してから改めて改行で分割し、全ての
                // 物理行へ`pad`を付け直す(全体を跨いで正しくインデントし直す)
                let body_joined = body_lines.join("\n");
                let indented =
                    body_joined.split('\n').map(|l| if l.is_empty() { l.to_string() } else { format!("{pad}{l}") }).collect::<Vec<_>>().join("\n");
                Ok(format!("(async ({params_js}) => {{\n{indented}\n{pad}}})"))
            }
        }
    }

    fn gen_call(&mut self, expr: &Expr) -> CodegenResult<String> {
        let Expr::Call { callee, args, pos } = expr else { unreachable!("caller guarantees Expr::Call") };

        // 組み込み関数はランタイムの同期ヘルパへ直接変換
        if let Expr::Ident { name, .. } = &**callee
            && checker::is_builtin(name)
        {
            let js_args = args.iter().map(|a| self.gen_expr(a)).collect::<CodegenResult<Vec<_>>>()?;
            return self.gen_builtin_call(name, args, &js_args, *pos);
        }

        // パッケージ修飾の自由関数呼び出し(`mathutil.add(...)`、milestone 6)。ローカル変数に
        // よるshadowが優先される(is_package_alias参照)ので、真にパッケージ参照と判定
        // できたものだけをここで処理する
        if let Expr::Member { target, name, .. } = &**callee
            && let Expr::Ident { name: alias, .. } = &**target
            && self.ctx.is_package_alias(alias)
        {
            if self.ctx.lookup_package_fn(alias, name).is_none() {
                // 未importどおりexportされていない/存在しない関数——実行時に
                // `undefined is not a function`でクラッシュさせず、明確なErrにする
                return Err(format!("codegen: package '{alias}' has no exported function '{name}' ({}:{})", pos.line, pos.col));
            }
            let args_js = args.iter().map(|a| self.gen_expr(a)).collect::<CodegenResult<Vec<_>>>()?;
            return Ok(format!("(await {}({}))", fn_js_name(alias, name), args_js.join(", ")));
        }

        // メソッド呼び出し: recv.method(args) → __m_Struct_method(recv, args)
        if let Some((target, js_name)) = self.resolve_method_target(callee, *pos)? {
            let recv_js = self.gen_expr(target)?;
            let args_js = args.iter().map(|a| self.gen_expr(a)).collect::<CodegenResult<Vec<_>>>()?;
            let all_args = std::iter::once(recv_js).chain(args_js).collect::<Vec<_>>().join(", ");
            return Ok(format!("(await {js_name}({all_args}))"));
        }

        // ユーザー定義関数はすべてasyncなので常にawait
        let callee_js = self.resolve_free_fn_value(callee)?;
        let args_js = args.iter().map(|a| self.gen_expr(a)).collect::<CodegenResult<Vec<_>>>()?;
        Ok(format!("(await {callee_js}({}))", args_js.join(", ")))
    }

    // 自由関数の呼び出し先を素のJS識別子へ解決する。自パッケージの既知のトップレベル関数
    // (fn_decls)ならpkg接頭辞付きの名前(mainパッケージなら無修飾のまま——fn_js_name参照)、
    // パッケージ修飾(`mathutil.add`)ならそのパッケージのexportedな関数か確認して
    // 同様にpkg接頭辞付きの名前(code review指摘: この分岐が無いと`spawn mathutil.add(...)`
    // が素の関数値を得られず、既存のMember読み取りガードに落ちて「package/member access
    // is not yet supported」という紛らわしいエラーになっていた——gen_callの呼び出し形
    // 〈`(await ...)`まで含めて組み立てる〉とは別に、ここでは呼び出し先の値だけを解決する)、
    // それ以外(ローカル変数に入った関数値等)はgen_exprへ素通しする。gen_call/gen_spawnで共有
    fn resolve_free_fn_value(&mut self, callee: &Expr) -> CodegenResult<String> {
        if let Expr::Ident { name, .. } = callee
            && self.ctx.lookup_fn(name).is_some()
        {
            return Ok(fn_js_name(self.ctx.pkg(), name));
        }
        if let Expr::Member { target, name, pos } = callee
            && let Expr::Ident { name: alias, .. } = &**target
            && self.ctx.is_package_alias(alias)
        {
            if self.ctx.lookup_package_fn(alias, name).is_none() {
                return Err(format!("codegen: package '{alias}' has no exported function '{name}' ({}:{})", pos.line, pos.col));
            }
            return Ok(fn_js_name(alias, name));
        }
        self.gen_expr(callee)
    }

    // Member呼び出しがフィールドでなくメソッドかどうかを判定する共通ヘルパ。gen_call・
    // gen_spawn(spawn/detachの呼び出し先解決)で同じ判定ロジックを共有する(TS版は
    // genCall/genExprの"spawn"ケースに同じ判定を2回書いているが、Rust版は1箇所にまとめる)。
    // TS版calls.ts/codegen.tsと同じ「フィールドが勝つ」順序——targetがstruct型で
    // nameが宣言済みフィールドでなければメソッドと判定する。Someなら(レシーバ式,
    // メソッドのJS関数名)、Noneなら「フィールドまたは自由関数」呼び出し。
    // milestone 17: 未知の名前(フィールドでもメソッドでもない)の判定・エラー文言は
    // `checker::resolve_method_call_target`(TS版`inferCall`のrecv.method(args)分岐と
    // 同じ「has no field or method」文言)に委譲——以前はここで独自に
    // `'{struct}' has no method '{name}'`という、TS版と食い違う簡略化したメッセージを
    // 組み立てていた(PR #30のcode reviewで指摘・記録された、pkg修飾struct literal
    // 検証とは別の「フィールド名判定の統一漏れ」)
    fn resolve_method_target<'e>(&self, callee: &'e Expr, call_pos: Pos) -> CodegenResult<Option<(&'e Expr, String)>> {
        let Expr::Member { target, name, .. } = callee else { return Ok(None) };
        let target_ty = checker::infer_expr(&self.ctx, target);
        let Type::Struct { fields, name: struct_name, .. } = &target_ty else {
            return Ok(None);
        };
        if checker::resolve_method_call_target(fields, &self.ctx, struct_name, name, call_pos)? {
            Ok(Some((target, method_js_name(struct_name, name))))
        } else {
            Ok(None)
        }
    }

    // spawn f(...) / detach f(...)。引数はspawn時点で評価する(Goと同じ)。呼び出し先は
    // 素の関数値として__spawn/__detachへ渡す(即座には呼ばない——ランタイム側が呼ぶ)。
    // メソッド呼び出し(spawn recv.method())はgen_callと同じ判定でレシーバを引数列の
    // 先頭に回す——この特別扱いを忘れると`recv.method`という素のプロパティ参照を渡して
    // しまい実行時`f is not a function`でクラッシュする(TS版が過去にcode reviewで発見して
    // 直したバグ、TS版codegen.tsのコメント参照)ので、gen_call同様resolve_method_targetを再利用する
    fn gen_spawn(&mut self, call: &Expr, detached: bool, pos: Pos) -> CodegenResult<String> {
        let Expr::Call { callee, args, .. } = call else {
            unreachable!("parser guarantees spawn/detach always wraps Expr::Call")
        };
        let (callee_js, all_args_js) = match self.resolve_method_target(callee, pos)? {
            Some((target, js_name)) => {
                let recv_js = self.gen_expr(target)?;
                let mut all = vec![recv_js];
                for a in args {
                    all.push(self.gen_expr(a)?);
                }
                (js_name, all)
            }
            None => {
                let callee_js = self.resolve_free_fn_value(callee)?;
                let args_js = args.iter().map(|a| self.gen_expr(a)).collect::<CodegenResult<Vec<_>>>()?;
                (callee_js, args_js)
            }
        };
        let args_array = format!("[{}]", all_args_js.join(", "));
        if detached {
            Ok(format!("__detach({callee_js}, {args_array})"))
        } else {
            // TS版spawnStack[..] = trueに相当。gen_fn_bodyが関数丸ごとのwait枠を付けるかの判定に使う
            *self.spawn_used.last_mut().expect("inside a function body") = true;
            Ok(format!("__spawn({callee_js}, {args_array})"))
        }
    }

    // select { name := <-ch => body ...  _ => defaultBody }。全ての「準備できるまで待って
    // 選ぶ」ロジックはランタイムの__selectへ委譲する——codegenはchannel式・ハンドラ・
    // defaultの3つの配列/値を組み立てるだけ。各アームの束縛名は`elem型 | closed`として
    // 正しくスコープに宣言してからbodyを生成する(OrElseの束縛パターンと同じ——外側の
    // 同名変数〈型が違う〉をshadowする際に誤って型依存のcodegen判断〈__iarith等〉を
    // 誤らせないため)
    fn gen_select(&mut self, arms: &[SelectArm], default_arm: Option<&Expr>, pos: Pos) -> CodegenResult<String> {
        let _ = pos; // 現状は使わない(将来の診断用に受け取っておく)
        let mut channels = Vec::with_capacity(arms.len());
        let mut handlers = Vec::with_capacity(arms.len());
        for arm in arms {
            let ch_ty = checker::infer_expr(&self.ctx, &arm.channel);
            if !matches!(ch_ty, Type::Chan(_) | Type::Any) {
                return Err(format!(
                    "codegen: select arm requires a channel, got '{}' ({}:{})",
                    types::type_to_string(&ch_ty), arm.pos.line, arm.pos.col
                ));
            }
            let channel_js = self.gen_expr(&arm.channel)?;
            let bind_ty = match ch_ty {
                Type::Chan(elem) => types::union_of(vec![*elem, types::CLOSED]),
                _ => Type::Any,
            };
            self.ctx.push_scope();
            self.ctx.declare(&arm.name, bind_ty); // パーサ保証によりarm.nameは絶対に"_"ではない
            let body_js = self.gen_expr(&arm.body)?;
            self.ctx.pop_scope();
            channels.push(channel_js);
            handlers.push(format!("(async ({}) => {body_js})", arm.name));
        }
        // 空アーム+defaultなしは構文上は許されてしまう(パーサーはempty selectを拒否しない)が、
        // 診断は出さない設計なので__selectへ空配列を渡し、Goのselect{}と同じ「永久に完了
        // しないPromise」にフォールバックさせる(パニックはしない、対応不要)
        let default_js = match default_arm {
            Some(body) => format!("(async () => {})", self.gen_expr(body)?),
            None => "null".to_string(),
        };
        Ok(format!("(await __select([{}], [{}], {default_js}))", channels.join(", "), handlers.join(", ")))
    }

    fn gen_builtin_call(&self, name: &str, arg_exprs: &[Expr], args: &[String], pos: Pos) -> CodegenResult<String> {
        // code review指摘: パーサ/checkerのどちらも組み込みの引数個数を検査しないため、
        // 以前は`args[0]`/`args[1]`への直接インデックスが足りない引数でパニックしていた
        // (例: `round()`)。「まだ対応していない構文はErrで返す、パニックさせない」という
        // 設計原則(ast.rsコメント参照)に反するため、個数を先に検査してから分岐する
        let required = match name {
            "print" => 0,
            "str" | "sleep" | "toInt" | "toFloat" | "round" | "floor" | "ceil" | "error" | "trim" | "upper" | "lower" | "sort" | "close" | "len"
            | "keys" | "values" => 1,
            "contains" | "indexOf" | "get" | "split" | "join" | "push" | "delete" | "filter" | "map" => 2,
            "reduce" => 3,
            _ => 0, // 未対応の組み込みはこの後のmatchのdefaultアームでエラーになる
        };
        if args.len() < required {
            return Err(format!(
                "codegen: builtin '{name}' expects at least {required} argument(s), got {} ({}:{})",
                args.len(),
                pos.line,
                pos.col
            ));
        }
        let at = self.at(pos);
        match name {
            "print" => Ok(format!("__print({})", args.join(", "))),
            "str" => Ok(format!("__fmt({})", args[0])),
            "sleep" => Ok(format!("(await __sleep({}))", args[0])),
            "toInt" => Ok(format!("__toInt({})", args[0])),
            "toFloat" => Ok(args[0].clone()), // int/floatは同じJS number
            "round" => Ok(format!("__toIntSafe(Math.round({}), {at})", args[0])),
            "floor" => Ok(format!("__toIntSafe(Math.floor({}), {at})", args[0])),
            "ceil" => Ok(format!("__toIntSafe(Math.ceil({}), {at})", args[0])),
            "error" => Ok(format!("__error({})", args[0])),
            "contains" => Ok(format!("{}.includes({})", args[0], args[1])),
            "indexOf" => Ok(format!("__indexOf({}, {})", args[0], args[1])),
            "get" => Ok(format!("__get({}, {})", args[0], args[1])),
            "split" => Ok(format!("{}.split({})", args[0], args[1])),
            "join" => Ok(format!("{}.join({})", args[0], args[1])),
            "trim" => Ok(format!("{}.trim()", args[0])),
            "upper" => Ok(format!("{}.toUpperCase()", args[0])),
            "lower" => Ok(format!("{}.toLowerCase()", args[0])),
            "push" => Ok(format!("{}.push({})", args[0], args[1])),
            "sort" => Ok(format!("__sorted({})", args[0])),
            "close" => Ok(format!("{}.close()", args[0])),
            // mapは.size、配列・文字列は.length
            "len" => {
                if matches!(arg_exprs.first().map(|a| checker::infer_expr(&self.ctx, a)), Some(Type::Map { .. })) {
                    Ok(format!("{}.size", args[0]))
                } else {
                    Ok(format!("{}.length", args[0]))
                }
            }
            // code review指摘(PR #19): lenはmap/配列を型で分岐しているのに、deleteは
            // 分岐せず無条件で`.delete()`を出していた——配列に`delete(xs, i)`を渡すと
            // 実行時に`xs.delete is not a function`でクラッシュする(配列にはdeleteの
            // 意味的な対応物が無い——TS版もdeleteはmap限定の組み込み)。型が確実に
            // Map/Any以外だと分かる場合だけ明確なErrにする(ANYは診断を出さない設計上
            // 許容——他の型に確実だと分かる場合だけ弾く、既存のrange-forアリティ
            // ガードと同じ考え方)
            "delete" => {
                let confidently_not_map = matches!(
                    arg_exprs.first().map(|a| checker::infer_expr(&self.ctx, a)),
                    Some(t) if !matches!(t, Type::Map { .. } | Type::Any)
                );
                if confidently_not_map {
                    return Err(format!("codegen: 'delete' requires a map argument ({}:{})", pos.line, pos.col));
                }
                Ok(format!("{}.delete({})", args[0], args[1]))
            }
            "keys" => Ok(format!("Array.from({}.keys())", args[0])),
            "values" => Ok(format!("Array.from({}.values())", args[0])),
            // 高階関数(milestone 10、F-8旧transform)。ランタイムヘルパ(__filter/__map/__reduce)
            // はH-2実装時にruntime.ts全体を移植済みで既に揃っている——ここでは呼び出すだけでよい
            "filter" => Ok(format!("(await __filter({}, {}))", args[0], args[1])),
            "map" => Ok(format!("(await __map({}, {}))", args[0], args[1])),
            "reduce" => Ok(format!("(await __reduce({}, {}, {}))", args[0], args[1], args[2])),
            _ => Err(format!("codegen: builtin '{name}' is not yet supported ({}:{})", pos.line, pos.col)),
        }
    }
}

// mesh/json(組み込みパッケージ、milestone 9)のシグネチャ定義。`.mesh`ソースを持たない——
// TS版`stdlib.ts`のBUILTIN_PACKAGESに相当し、この定義から直接`checker::PackageSymbols`へ
// 登録する(generate_all_modules参照)。ランタイムの実体(json$parse等)は既にprelude側に
// 実装済み(H-2実装時にruntime.ts全体を移植済みのため、ここではシグネチャの登録だけでよい)。
// json.Valueは真に自己参照する判別可能union(TS版はarr/objメンバーがValue自身を配列/map
// 越しに参照する共有可変オブジェクトとして手組みする)——milestone 2以来一貫して
// 「対応不可、明確なErr」としてきた自己参照型の壁そのもの(examples/tree.mesh参照)なので、
// 不透明な殻structとして扱う(json struct合成のdecode<X>はjson.field/asXxx等の不透明な
// ヘルパー越しにしかValueへ触れないため実害が無い、生の`is`/`match`でValueを直接構造的に
// 分解する手書きデコーダだけがこのスコープ縮小の影響を受ける——milestone 9のスコープ外)
fn json_stdlib_symbols() -> checker::PackageSymbols {
    fn fn_ty(params: Vec<Type>, ret: Type) -> Type {
        Type::Fn { params, ret: Box::new(ret) }
    }
    fn anon_struct(fields: Vec<types::StructField>) -> Type {
        Type::Struct { name: types::ANONYMOUS_STRUCT_NAME.to_string(), fields, is_error_type: false }
    }
    fn field(name: &str, type_: Type) -> types::StructField {
        types::StructField { name: name.to_string(), type_ }
    }

    // json.Value = { kind: "str", s: string } | { kind: "num", n: float } | { kind: "bool", b: bool }
    //            | { kind: "null" } | { kind: "arr", items: Value[] } | { kind: "obj", entries: map<string, Value> }
    // TS版はarr/objメンバーがValue自身を配列/map越しに参照する真の自己参照型(共有可変
    // オブジェクトとして手組み)だが、Rustの所有権ベースのType表現では真の自己参照を
    // 表せない(milestone 2以来の壁、examples/tree.mesh参照)。ここでは再帰位置
    // (arr.items/obj.entriesの要素/値型)だけを名前だけの不透明な殻
    // (is_error_instanceの再帰と紛れないよう`hollow_value`と呼ぶ)に置き換え、
    // それ以外の各メンバー自身は本物の構造(kind判別フィールド+実フィールド)を持たせる——
    // これにより`if v is {kind:"obj"} { len(v.entries) }`のような1階層の構造分解
    // (TS版のF-14既存機能、json struct機能そのものより前からある、tests/e2e.test.ts:
    // 1146-1160で確認)が正しい型で narrowing・フィールド推論される。2階層以上の
    // 入れ子destructureだけがこのスコープ縮小の影響を受ける(is/matchの実行時テスト
    // 自体はASTから直接組み立てるため2階層以上でも動く——影響を受けるのは
    // checker側の型推論の精度だけ、milestone 7のgen_type_test参照)
    let hollow_value = Type::Struct { name: "json.Value".to_string(), fields: vec![], is_error_type: false };
    let value_ty = Type::Union {
        members: vec![
            anon_struct(vec![field("kind", Type::Literal("str".to_string())), field("s", types::STRING)]),
            anon_struct(vec![field("kind", Type::Literal("num".to_string())), field("n", types::FLOAT)]),
            anon_struct(vec![field("kind", Type::Literal("bool".to_string())), field("b", types::BOOL)]),
            anon_struct(vec![field("kind", Type::Literal("null".to_string()))]),
            anon_struct(vec![field("kind", Type::Literal("arr".to_string())), field("items", Type::Array(Box::new(hollow_value.clone())))]),
            anon_struct(vec![
                field("kind", Type::Literal("obj".to_string())),
                field("entries", Type::Map { key: Box::new(types::STRING), value: Box::new(hollow_value) }),
            ]),
        ],
        // milestone 12: 6メンバー全てが"kind"を共有タグとして持つ(値は全て異なるリテラル)ため、
        // `find_discriminant_tag`を通せば得られるのと同じ値をここで直接設定しておく。
        // json.Valueは`.mesh`のTypeDeclではなくRustコードとして直接組み立てられるため
        // resolve_type_decls(に組み込まれたcompute_discriminant_tag)を経由しない——
        // ここで計算し忘れると、pkg修飾struct literalのdisambiguationが将来
        // resolve_struct_lit_member経由になった際にNoneのまま素通りしてしまう
        discriminant_tag: Some("kind".to_string()),
    };
    let array_of_value = Type::Array(Box::new(value_ty.clone()));

    let mut types = HashMap::new();
    types.insert("Value".to_string(), value_ty.clone());

    let mut fns = HashMap::new();
    fns.insert("parse".to_string(), fn_ty(vec![types::STRING], types::union_of(vec![value_ty.clone(), types::ERROR])));
    fns.insert("stringify".to_string(), fn_ty(vec![value_ty.clone()], types::STRING));
    fns.insert("field".to_string(), fn_ty(vec![value_ty.clone(), types::STRING], types::union_of(vec![value_ty.clone(), types::ERROR])));
    fns.insert("optField".to_string(), fn_ty(vec![value_ty.clone(), types::STRING], types::union_of(vec![value_ty.clone(), types::NONE])));
    fns.insert("asString".to_string(), fn_ty(vec![value_ty.clone()], types::union_of(vec![types::STRING, types::ERROR])));
    fns.insert("asInt".to_string(), fn_ty(vec![value_ty.clone()], types::union_of(vec![INT, types::ERROR])));
    fns.insert("asFloat".to_string(), fn_ty(vec![value_ty.clone()], types::union_of(vec![types::FLOAT, types::ERROR])));
    fns.insert("asBool".to_string(), fn_ty(vec![value_ty.clone()], types::union_of(vec![types::BOOL, types::ERROR])));
    fns.insert("asArray".to_string(), fn_ty(vec![value_ty], types::union_of(vec![array_of_value, types::ERROR])));

    checker::PackageSymbols { types, fns, consts: HashMap::new() }
}

// レシーバの型注釈から素の(pkg修飾されていない)struct名を取り出す。レシーバは常に
// 自パッケージ内のstructを指す前提(`fn (p: Point) ...`はPointが今処理中のパッケージの
// struct、という意味)——他パッケージの型に生やす拡張メソッド的な書き方
// (`fn (p: math.Point) ...`)はモジュールのこの段階でも対象外のまま
fn receiver_struct_name(recv: &Receiver) -> CodegenResult<String> {
    match &recv.type_node {
        TypeNode::Name { name, pkg: None, .. } => Ok(name.clone()),
        TypeNode::Name { pkg: Some(_), pos, .. } => {
            Err(format!("codegen: package-qualified receivers are not yet supported ({}:{})", pos.line, pos.col))
        }
        other => Err(format!("codegen: receiver type must be a plain struct name ({}:{})", other.pos().line, other.pos().col)),
    }
}

// メソッドの生成JS名: struct名+メソッド名で一意にする(他structの同名メソッドと衝突しない
// ように)。TS版のmethodJsNameを移植。struct_nameは既にpkg修飾済み("mathutil.Point"等)の
// 前提で"."を"$"に変換する(milestone 6でパッケージが導入されて初めて意味を持つ変換——
// mainパッケージのstructは無修飾なのでreplaceは何もしない)
fn method_js_name(struct_name: &str, method_name: &str) -> String {
    format!("__m_{}_{}", struct_name.replace('.', "$"), method_name)
}

// トップレベル自由関数の生成JS名: mainパッケージは素の名前のまま、それ以外は
// "{pkg}${name}"(TS版fnJsNameと同じ)。パッケージ修飾呼び出し(`mathutil.add(...)`)の
// 呼び出し先と、その関数自身の宣言側の両方でこの名前が一致していないと参照が壊れる。
// トップレベルconstにはこの接頭辞を付けていない(パッケージ修飾された「呼び出しを
// 伴わない」値参照が対象外のため——gen_const_decl参照。複数パッケージで同名の
// トップレベルconstが衝突する場合はgen_const_declが明確なErrで検出する)
fn fn_js_name(pkg: &str, name: &str) -> String {
    if pkg == "main" { name.to_string() } else { format!("{pkg}${name}") }
}

// is/matchのパターン(型)を実行時テストのJS式へ変換する(TS版codegen.tsのgenTypeTestの
// 移植、milestone 7・判別可能union対応)。discriminant_tagは一切参照せず、ASTのTypeNodeから
// 直接構造的なテストを組み立てる(struct形パターンは各フィールドを`ref.field`へ再帰)。
// Union形パターン(`A | B`をそのままis/matchのパターンに書く形)はTS版と同じく"false"
// (単一型のみが渡ってくる前提——このリゾルバでも到達しない想定だが、クラッシュを
// 避けるため安全な既定値にしておく)
fn gen_type_test(ref_js: &str, target: &TypeNode) -> String {
    match target {
        TypeNode::Literal { value, .. } => format!("({ref_js} === {value:?})"),
        TypeNode::Array { .. } => format!("Array.isArray({ref_js})"),
        TypeNode::Chan { .. } => format!("({ref_js} instanceof __Channel)"),
        TypeNode::Union { .. } => "false".to_string(),
        TypeNode::MapType { .. } => format!("({ref_js} instanceof Map)"),
        TypeNode::FnType { .. } => format!("(typeof {ref_js} === \"function\")"),
        TypeNode::StructType { fields, .. } => {
            let obj_test =
                format!("(typeof {ref_js} === \"object\" && {ref_js} !== null && !({ref_js} instanceof Error) && !Array.isArray({ref_js}))");
            std::iter::once(obj_test)
                .chain(fields.iter().map(|f| gen_type_test(&format!("{ref_js}.{}", f.name), &f.type_node)))
                .collect::<Vec<_>>()
                .join(" && ")
        }
        TypeNode::Name { name, .. } => match name.as_str() {
            "none" => format!("({ref_js} === null)"),
            "closed" => format!("({ref_js} === __CLOSED)"),
            "error" => format!("({ref_js} instanceof Error)"),
            "int" => format!("Number.isInteger({ref_js})"),
            "float" => format!("(typeof {ref_js} === \"number\")"),
            "string" => format!("(typeof {ref_js} === \"string\")"),
            "bool" => format!("(typeof {ref_js} === \"boolean\")"),
            // ユーザー定義struct(pkg修飾された型名パターンも含む)は汎用オブジェクトテスト
            _ => format!("(typeof {ref_js} === \"object\" && {ref_js} !== null && !({ref_js} instanceof Error) && !Array.isArray({ref_js}))"),
        },
    }
}

// パッケージ間のimport依存グラフを依存順(importされる側が先)にソートする(TS版
// checkModulesの依存順ソート+循環検出に相当)。診断ではなく明確なErr——循環があると
// 依存先のexportedシンボルがまだregistryに無い状態で参照してしまい、静かに未解決
// (ANY)へフォールバックする壊れたJSを生成してしまうため、先に弾く
fn topo_sort_packages(packages: &[(String, Vec<&ModuleUnit>)]) -> CodegenResult<Vec<String>> {
    let names: HashSet<&str> = packages.iter().map(|(p, _)| p.as_str()).collect();
    let mut deps: HashMap<String, HashSet<String>> = HashMap::new();
    for (pkg, files) in packages {
        let mut set = HashSet::new();
        for f in files {
            for imp in &f.program.imports {
                if names.contains(imp.alias.as_str()) {
                    set.insert(imp.alias.clone());
                }
            }
        }
        deps.insert(pkg.clone(), set);
    }

    let mut order: Vec<String> = Vec::new();
    let mut state: HashMap<String, u8> = HashMap::new(); // 1=訪問中、2=完了
    for (pkg, _) in packages {
        visit_package(pkg, &deps, &mut state, &mut order)?;
    }
    Ok(order)
}

fn visit_package(pkg: &str, deps: &HashMap<String, HashSet<String>>, state: &mut HashMap<String, u8>, order: &mut Vec<String>) -> CodegenResult<()> {
    match state.get(pkg) {
        Some(2) => return Ok(()),
        Some(1) => return Err(format!("codegen: import cycle detected involving package '{pkg}'")),
        _ => {}
    }
    state.insert(pkg.to_string(), 1);
    if let Some(ds) = deps.get(pkg) {
        for d in ds {
            visit_package(d, deps, state, order)?;
        }
    }
    state.insert(pkg.to_string(), 2);
    order.push(pkg.to_string());
    Ok(())
}

// ブロックが必ず終端する(return/break/continueで終わる)かどうかの単純な判定
// (milestone 7・if-isのnarrowing用)。TS版のblockTerminatesはif/elseの両分岐が終端するか
// 等も見るフルのCFG解析だが、実際のexample(`if v is closed { break }`のような単純な
// 単文ブロック)はこの単純化で十分カバーできるため、あえて最後の文だけを見る
fn block_always_terminates(block: &Block) -> bool {
    matches!(block.stmts.last(), Some(Stmt::Return { .. }) | Some(Stmt::Break { .. }) | Some(Stmt::Continue { .. }))
}

fn other_pos(stmt: &Stmt) -> Pos {
    match stmt {
        Stmt::ShortVarDecl { pos, .. }
        | Stmt::TypedVarDecl { pos, .. }
        | Stmt::Assign { pos, .. }
        | Stmt::ExprStmt { pos, .. }
        | Stmt::Return { pos, .. }
        | Stmt::For { pos, .. }
        | Stmt::IncDec { pos, .. }
        | Stmt::Break { pos }
        | Stmt::Continue { pos }
        | Stmt::Wait { pos, .. }
        | Stmt::Send { pos, .. }
        | Stmt::RangeFor { pos, .. }
        | Stmt::DeferStmt { pos, .. } => *pos,
        Stmt::If(if_stmt) => if_stmt.pos,
    }
}

fn return_op_str(op: &TokenType) -> &'static str {
    match op {
        TokenType::AndAnd => "&&",
        TokenType::OrOr => "||",
        TokenType::Lt => "<",
        TokenType::Le => "<=",
        TokenType::Gt => ">",
        TokenType::Ge => ">=",
        TokenType::Plus => "+",
        TokenType::Minus => "-",
        TokenType::Star => "*",
        TokenType::Slash => "/",
        TokenType::Percent => "%",
        other => panic!("unexpected binary operator token: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn gen_js(src: &str) -> CodegenResult<String> {
        let program = parse(src).unwrap_or_else(|e| panic!("parse error: {e:?}"));
        generate(&program, "t.mesh")
    }

    fn gen_body(src: &str) -> String {
        let js = gen_js(src).unwrap_or_else(|e| panic!("codegen error: {e}"));
        // PRELUDEと末尾の起動コードを除いた、fn宣言以降の本体だけを取り出す
        let marker = "// ===== end runtime =====\n\n";
        js.split_once(marker).expect("prelude marker not found").1.to_string()
    }

    // 複数パッケージのテスト用ヘルパ。(pkg, file, src)のリストをModuleUnitへ変換して
    // generate_modulesを呼ぶ
    fn gen_modules(units: &[(&str, &str, &str)]) -> CodegenResult<String> {
        let modules: Vec<ModuleUnit> = units
            .iter()
            .map(|(pkg, file, src)| {
                let program = parse(src).unwrap_or_else(|e| panic!("parse error in {file}: {e:?}"));
                ModuleUnit { pkg: pkg.to_string(), file: file.to_string(), program }
            })
            .collect();
        generate_modules(&modules)
    }

    fn gen_modules_body(units: &[(&str, &str, &str)]) -> String {
        let js = gen_modules(units).unwrap_or_else(|e| panic!("codegen error: {e}"));
        let marker = "// ===== end runtime =====\n\n";
        js.split_once(marker).expect("prelude marker not found").1.to_string()
    }

    #[test]
    fn preludeはランタイムのjs本体だけを含みtsの宣言文は含まない() {
        let full = gen_js("fn main() {}").unwrap();
        assert!(RUNTIME_TS.contains("export const PRELUDE")); // 元ファイル(.ts)には含まれる
        assert!(!full.contains("export const PRELUDE")); // 生成JSからは取り除かれている
        assert!(full.contains("class __Channel")); // ランタイム本体は残っている
        let js = gen_body("fn main() {}");
        assert!(js.starts_with("async function main"), "got: {js}");
    }

    #[test]
    fn helloが期待通りのjsを生成する() {
        let js = gen_body("fn main() {\n  print(\"Hello, Mesh!\")\n}");
        assert!(js.contains("async function main() {"));
        assert!(js.contains("__print(\"Hello, Mesh!\");"));
        assert!(js.ends_with("main().catch(__panic);\n"));
    }

    #[test]
    fn int同士の剰余はimodヘルパを呼ぶjsになる() {
        let js = gen_body("fn main() {\n  x := 15 % 3\n}");
        assert!(js.contains("__imod(15, 3, \"t.mesh:2:11\")"), "got: {js}");
    }

    #[test]
    fn int同士の除算はidivヘルパを呼ぶjsになる() {
        let js = gen_body("fn main() {\n  x := 7 / 2\n}");
        assert!(js.contains("__idiv(7, 2, \"t.mesh:2:10\")"), "got: {js}");
    }

    #[test]
    fn int同士の加算はiarithヘルパを呼ぶjsになる() {
        let js = gen_body("fn main() {\n  x := 1 + 2\n}");
        assert!(js.contains("__iarith(1, \"+\", 2, \"t.mesh:2:10\")"), "got: {js}");
    }

    #[test]
    fn floatが混ざるとint系ヘルパは呼ばずそのまま演算子を出す() {
        let js = gen_body("fn main() {\n  x := 1 + 2.5\n}");
        assert!(js.contains("(1 + 2.5);"), "got: {js}");
        assert!(!js.contains("__iarith"));
    }

    #[test]
    fn 比較演算子は等価判定へ変換される() {
        let js = gen_body("fn main() {\n  x := 1 == 2\n}");
        assert!(js.contains("(1 === 2);"), "got: {js}");
    }

    #[test]
    fn if_else_ifチェーンとfor文を生成できる() {
        let js = gen_body(
            "fn main() {\n  for i := 1; i <= 3; i++ {\n    if i == 1 {\n      print(\"a\")\n    } else if i == 2 {\n      print(\"b\")\n    } else {\n      print(\"c\")\n    }\n  }\n}",
        );
        assert!(js.contains("for (let i = 1; (i <= 3); i++) {"), "got: {js}");
        assert!(js.contains("if ((i === 1)) {"));
        assert!(js.contains("} else if ((i === 2)) {"));
        assert!(js.contains("} else {"));
    }

    #[test]
    fn 複合代入は現在値を読んでからint系ヘルパを呼ぶ() {
        let js = gen_body("fn main() {\n  mut x := 1\n  x += 2\n}");
        assert!(js.contains("x = __iarith(x, \"+\", 2, \"t.mesh:3:3\");"), "got: {js}");
    }

    #[test]
    fn 文字列補間はfmtヘルパで連結したjsになる() {
        let js = gen_body("fn main() {\n  n := 3\n  print(\"n = ${n}\")\n}");
        assert!(js.contains("(\"n = \" + __fmt(n))"), "got: {js}");
    }

    #[test]
    fn 自由関数呼び出しはawaitされる() {
        let js = gen_body("fn add(a: int, b: int) int {\n  return a + b\n}\nfn main() {\n  x := add(1, 2)\n}");
        assert!(js.contains("(await add(1, 2))"), "got: {js}");
    }

    #[test]
    fn round等の組み込みの戻り値はint扱いされ演算がidivになる() {
        // code review(PR #16)で発覚: infer_callが組み込みを素通ししてANYへ落としていたため、
        // round()の結果同士の割り算がint演算と分からずJSの素の/になっていた(2.5になる実バグ)
        let js = gen_body("fn main() {\n  a := round(5.0)\n  b := round(2.0)\n  x := a / b\n}");
        assert!(js.contains("__idiv(a, b, "), "got: {js}");
    }

    #[test]
    fn toint等の組み込みは引数が足りないとパニックせず明確なエラーになる() {
        // code review(PR #16)で発覚: gen_builtin_callがargs[0]/args[1]へ直接インデックスして
        // いたため、引数不足の組み込み呼び出し(例: round())がコンパイラをパニックさせていた
        let err = gen_js("fn main() {\n  x := round()\n}").unwrap_err();
        assert!(err.contains("expects at least 1 argument"), "got: {err}");
        let err2 = gen_js("fn main() {\n  x := contains(\"a\")\n}").unwrap_err();
        assert!(err2.contains("expects at least 2 argument"), "got: {err2}");
    }

    #[test]
    fn 定数の型注釈は推論より優先される() {
        // xはリテラル値からはintと推れるが、明示された型注釈floatにより
        // 後続の算術がfloat扱いになる(__iarithが呼ばれない)ことを確認する
        let js = gen_body("x: float = 1\nfn main() {\n  y := x + 2\n}");
        assert!(js.contains("(x + 2);"), "got: {js}");
        assert!(!js.contains("__iarith"));
    }

    #[test]
    fn struct_litとフィールド読み書きが生成できる() {
        let js = gen_body(
            "struct User {\n  name: string\n  age: int\n}\nfn main() {\n  u := User{name: \"alice\", age: 30}\n  print(u.name)\n  u.age = u.age + 1\n  u.age += 1\n}",
        );
        assert!(js.contains("const u = ({ name: \"alice\", age: 30 });"), "got: {js}");
        assert!(js.contains("__print(u.name);"), "got: {js}");
        // u.ageはstructのフィールド型(int)として正しく推論されるので、フィールド越しの
        // 演算も__iarith等のint安全ヘルパを通る(単なる素の`+`にはならない)
        assert!(js.contains("u.age = __iarith(u.age, \"+\", 1,"), "got: {js}");
        assert!(js.matches("__iarith(u.age, \"+\", 1,").count() == 2, "got: {js}");
    }

    #[test]
    fn レシーバメソッドの呼び出しが生成できる() {
        let js = gen_body(
            "struct User {\n  name: string\n  age: int\n}\nfn (u: User) describe() string {\n  return \"${u.name} (${u.age})\"\n}\nfn main() {\n  u := User{name: \"alice\", age: 30}\n  print(u.describe())\n}",
        );
        assert!(js.contains("async function __m_User_describe(u) {"), "got: {js}");
        assert!(js.contains("(await __m_User_describe(u))"), "got: {js}");
    }

    #[test]
    fn 生成直後のstruct_litへ直接メソッドチェーンできる() {
        // README記載のTodo{...}.complete().render()のようなチェーンが
        // 正しくtargetの型を追えることを確認する(struct_lit → メソッド呼び出し → メソッド呼び出し)
        let js = gen_body(
            "struct Todo {\n  title: string\n  done: bool\n}\nfn (t: Todo) complete() Todo {\n  return Todo{title: t.title, done: true}\n}\nfn (t: Todo) render() string {\n  return t.title\n}\nfn main() {\n  print(Todo{title: \"x\", done: false}.complete().render())\n}",
        );
        assert!(js.contains("(await __m_Todo_render((await __m_Todo_complete(({ title: \"x\", done: false })))))"), "got: {js}");
    }

    #[test]
    fn フィールドと同名メソッドはフィールドアクセスが勝つ() {
        // TS版calls.ts/codegen.tsと同じ「フィールドが勝つ」順序: 同名のメソッドがあっても
        // 裸のメンバーアクセスはフィールドを読む
        let js = gen_body("struct Box {\n  value: int\n}\nfn (b: Box) value() int {\n  return 999\n}\nfn main() {\n  b := Box{value: 1}\n  x := b.value\n}");
        assert!(js.contains("const x = b.value;"), "got: {js}");
    }

    #[test]
    fn proto拒否_struct_litのフィールド名として使えない() {
        let err = gen_js("struct User {\n  name: string\n}\nfn main() {\n  u := User{__proto__: \"x\"}\n}").unwrap_err();
        assert!(err.contains("__proto__"), "got: {err}");
    }

    #[test]
    fn proto拒否_代入先のフィールド名としても使えない() {
        let err = gen_js("struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  u.__proto__ = \"x\"\n}").unwrap_err();
        assert!(err.contains("__proto__"), "got: {err}");
    }

    #[test]
    fn 未宣言_非struct型のレシーバは明確なエラーになる() {
        let err = gen_js("fn (u: int) describe() {\n  print(u)\n}\nfn main() {}").unwrap_err();
        assert!(err.contains("not a declared struct"), "got: {err}");
    }

    #[test]
    fn structにもフィールドにもないメソッド呼び出しは明確なエラーになる() {
        // milestone 17: TS版`inferCall`のrecv.method(args)分岐と同じ文言
        // (「has no field or method」、fields:一覧込み)に統一済み
        let err = gen_js("struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  u.unknown()\n}").unwrap_err();
        assert!(err.contains("User has no field or method 'unknown' (fields: name)"), "got: {err}");
    }

    #[test]
    fn フィールドが1つも無いstructでの不明な呼び出しはfields_noneと表示される() {
        // TS版はfields一覧が空のとき`.join(", ") || "none"`でリテラル"none"を表示する
        // (実行確認済み)
        let err = gen_js("struct Empty {\n}\nfn (e: Empty) foo() string {\n  return \"x\"\n}\nfn main() {\n  e := Empty{}\n  print(e.bar())\n}")
            .unwrap_err();
        assert!(err.contains("Empty has no field or method 'bar' (fields: none)"), "got: {err}");
    }

    #[test]
    fn error_structのリテラルはerrtagで包まれる() {
        let js = gen_body("error struct Oops {\n  message: string\n}\nfn main() {\n  o := Oops{message: \"x\"}\n}");
        assert!(js.contains("const o = __errTag({ message: \"x\" });"), "got: {js}");
    }

    #[test]
    fn 通常structのリテラルはerrtagで包まれない() {
        let js = gen_body("struct Point {\n  x: int\n}\nfn main() {\n  p := Point{x: 1}\n}");
        assert!(js.contains("const p = ({ x: 1 });"), "got: {js}");
        assert!(!js.contains("__errTag"));
    }

    #[test]
    fn json_structは普通のstructとして解決・構築できる() {
        // milestone 9: is_jsonはdecode<X>自動生成(json_decode.rs、main.rsでparse直後に
        // 済ませる)の対象を決めるだけで、struct自体の型解決には影響しない
        // (TS版resolveAlias/resolveTypeがisJsonを一切参照しないことに合わせた)。
        // gen_js/gen_bodyは合成ステップを経ないが、それでもData自体は普通のstructとして
        // 解決・構築でき、フィールドアクセスも正しい型で推論される(__iarithが選ばれる)
        let js = gen_body("json struct Data {\n  n: int\n}\nfn main() {\n  d := Data{n: 1}\n  print(d.n + 1)\n}");
        assert!(js.contains("const d = ({ n: 1 });"), "got: {js}");
        assert!(js.contains("__iarith(d.n, \"+\", 1,"), "got: {js}");
    }

    #[test]
    fn 未登録パッケージへのstructリテラル修飾は明確なエラーになる() {
        // milestone 6: パッケージ修飾自体はサポートされるが、"math"はどこにもimport/登録
        // されていない(単一パッケージ"main"のみのgen_js経由)ため、明確なエラーになる
        let err = gen_js("fn main() {\n  x := math.Point{x: 1, y: 2}\n}").unwrap_err();
        assert!(err.contains("package 'math' has no exported struct 'Point'"), "got: {err}");
    }

    #[test]
    fn bareのpropはpropヘルパを生成し関数をtry_catchで包む() {
        let js = gen_body("fn f() int | error {\n  return 1\n}\nfn main() {\n  x := f()?\n  print(x)\n}");
        assert!(js.contains("const x = __prop((await f()));"), "got: {js}");
        assert!(js.contains("try {"), "got: {js}");
        assert!(js.contains("} catch (e) {"), "got: {js}");
        assert!(js.contains("if (e instanceof __Propagate) return e.value;"), "got: {js}");
    }

    #[test]
    fn context付きpropはpropctxヘルパを生成する() {
        let js = gen_body("fn f() int | error {\n  return 1\n}\nfn main() {\n  x := f() ? \"failed\"\n  print(x)\n}");
        assert!(js.contains("__propCtx((await f()), async () => \"failed\")"), "got: {js}");
    }

    #[test]
    fn 構造化errorへのcontext付きpropは明確なエラーになる() {
        let err = gen_js(
            "error struct Oops {\n  message: string\n}\nfn f() int | Oops {\n  return 1\n}\nfn main() {\n  x := f() ? \"failed\"\n  print(x)\n}",
        )
        .unwrap_err();
        assert!(err.contains("structured error"), "got: {err}");
        // 同じoperandへのbare `?`(contextなし)は成功する
        let js = gen_body(
            "error struct Oops {\n  message: string\n}\nfn f() int | Oops {\n  return 1\n}\nfn main() {\n  x := f()?\n  print(x)\n}",
        );
        assert!(js.contains("__prop((await f()))"), "got: {js}");
    }

    #[test]
    fn propを使わない関数は従来通りtry_catchで包まれない() {
        let js = gen_body("fn main() {\n  x := 1\n  print(x)\n}");
        assert!(!js.contains("try {"), "got: {js}");
        assert!(!js.contains("__Propagate"), "got: {js}");
    }

    #[test]
    fn if文の中にネストしたpropでも囲む関数レベルでtry_catchが付く() {
        let js = gen_body("fn f() int | error {\n  return 1\n}\nfn main() {\n  if true {\n    x := f()?\n    print(x)\n  }\n}");
        assert!(js.contains("try {"), "got: {js}");
        assert!(js.contains("} catch (e) {"), "got: {js}");
    }

    #[test]
    fn orの裸形式は空引数のラムダになる() {
        let js = gen_body("fn f() int | none {\n  return 1\n}\nfn main() {\n  x := f() or 0\n}");
        assert!(js.contains("__or((await f()), async () => 0)"), "got: {js}");
    }

    #[test]
    fn or_の捨てる形も空引数のラムダになる() {
        let js = gen_body("fn f() int | error {\n  return 1\n}\nfn main() {\n  x := f() or _ => 0\n}");
        assert!(js.contains("__or((await f()), async () => 0)"), "got: {js}");
    }

    #[test]
    fn or束縛形は束縛名がラムダの引数になりフィールドアクセスも通る() {
        let js = gen_body(
            "error struct Oops {\n  message: string\n}\nfn f() int | Oops {\n  return 1\n}\nfn main() {\n  x := f() or e => e.message\n  print(x)\n}",
        );
        // eがOopsのフィールド型として束縛され(=Oopsのstruct型として解決され)、
        // e.messageがフィールドアクセスとして生成できている(未対応エラーにならない)ことを確認する
        assert!(js.contains("__or((await f()), async (e) => e.message)"), "got: {js}");
    }

    #[test]
    fn 多値の短縮変数宣言は未対応として明確なエラーになる() {
        let err = gen_js("fn f() int | error {\n  return 1\n}\nfn main() {\n  v, err := f()\n}").unwrap_err();
        assert!(err.contains("multi-value"), "got: {err}");
    }

    #[test]
    fn 配列リテラルとmapリテラルが生成できる() {
        let js = gen_body("fn main() {\n  xs := [1, 2, 3]\n  m := map<string, int>{\"a\": 1, \"b\": 2}\n  empty := map<string, int>{}\n}");
        assert!(js.contains("const xs = [1, 2, 3];"), "got: {js}");
        assert!(js.contains("const m = new Map([[\"a\", 1], [\"b\", 2]]);"), "got: {js}");
        assert!(js.contains("const empty = new Map();"), "got: {js}");
    }

    #[test]
    fn 添字読みはmapと配列で使い分けられる() {
        let js = gen_body("fn main() {\n  xs := [1, 2, 3]\n  m := map<string, int>{\"a\": 1}\n  x := xs[0]\n  v := m[\"a\"]\n}");
        assert!(js.contains("const x = __idx(xs, 0,"), "got: {js}");
        assert!(js.contains("const v = __mget(m, \"a\");"), "got: {js}");
    }

    #[test]
    fn 添字書き込みは配列がidxset_mapがsetになる() {
        let js = gen_body(
            "fn main() {\n  mut xs := [1, 2, 3]\n  m := map<string, int>{}\n  xs[0] = 10\n  m[\"a\"] = 1\n  xs[0] += 1\n  xs[0]++\n}",
        );
        assert!(js.contains("__idxset(xs, 0, 10,"), "got: {js}");
        assert!(js.contains("m.set(\"a\", 1);"), "got: {js}");
        assert!(js.contains("__idxset(xs, 0, __iarith(__idx(xs, 0,"), "got: {js}");
        assert!(js.contains("__idxset(xs, 0, (__idx(xs, 0,"), "got: {js}");
    }

    #[test]
    fn map要素への複合代入とincdecは明確なエラーになる() {
        let err1 = gen_js("fn main() {\n  m := map<string, int>{}\n  m[\"a\"] += 1\n}").unwrap_err();
        assert!(err1.contains("compound assignment on a map entry"), "got: {err1}");
        let err2 = gen_js("fn main() {\n  m := map<string, int>{}\n  m[\"a\"]++\n}").unwrap_err();
        assert!(err2.contains("increment/decrement of a map entry"), "got: {err2}");
    }

    #[test]
    fn ネストしたmapへの添字は読み書きincdecともに明確なエラーになる() {
        let src = "fn main() {\n  m := map<string, map<string, int>>{}\n  x := m[\"a\"][\"b\"]\n}";
        assert!(gen_js(src).unwrap_err().contains("narrow away 'none'"), "read");

        let src = "fn main() {\n  m := map<string, map<string, int>>{}\n  m[\"a\"][\"b\"] = 1\n}";
        assert!(gen_js(src).unwrap_err().contains("narrow away 'none'"), "assign");

        let src = "fn main() {\n  m := map<string, map<string, int>>{}\n  m[\"a\"][\"b\"] += 1\n}";
        assert!(gen_js(src).unwrap_err().contains("narrow away 'none'"), "compound assign");

        let src = "fn main() {\n  m := map<string, map<string, int>>{}\n  m[\"a\"][\"b\"]++\n}";
        assert!(gen_js(src).unwrap_err().contains("narrow away 'none'"), "incdec");
    }

    #[test]
    fn 範囲forの3形態が生成できる() {
        let js = gen_body(
            "fn main() {\n  xs := [1, 2, 3]\n  m := map<string, int>{\"a\": 1}\n  for i, v := range xs { print(i, v) }\n  for k, v := range m { print(k, v) }\n  for i := range 3 { print(i) }\n}",
        );
        assert!(js.contains("for (const [i, v] of xs.entries()) {"), "got: {js}");
        assert!(js.contains("for (const [k, v] of m) {"), "got: {js}");
        assert!(js.contains("for (let i = 0, __n = 3; i < __n; i++) {"), "got: {js}");
    }

    #[test]
    fn 範囲forのブランク名は正しいjsになる() {
        let js = gen_body("fn main() {\n  xs := [1, 2, 3]\n  for _, v := range xs { print(v) }\n}");
        assert!(js.contains("for (const [, v] of xs.entries()) {"), "got: {js}");
        // 単一名がブランクだとCスタイルループ変数に使えないため__iにフォールバックする
        let js2 = gen_body("fn main() {\n  for _ := range 3 { }\n}");
        assert!(js2.contains("for (let __i = 0, __n = 3; __i < __n; __i++) {"), "got: {js2}");
    }

    #[test]
    fn 範囲forのアリティ不一致は明確なエラーになる() {
        let err1 = gen_js("fn main() {\n  xs := [1, 2, 3]\n  for i := range xs { print(i) }\n}").unwrap_err();
        assert!(err1.contains("expects 2 name"), "got: {err1}");
        let err2 = gen_js("fn main() {\n  for i, j := range 3 { print(i, j) }\n}").unwrap_err();
        assert!(err2.contains("expects 1 name"), "got: {err2}");
    }

    #[test]
    fn len_はmapとarrayで使い分けられる() {
        let js = gen_body("fn main() {\n  xs := [1, 2, 3]\n  m := map<string, int>{\"a\": 1}\n  a := len(xs)\n  b := len(m)\n}");
        assert!(js.contains("const a = xs.length;"), "got: {js}");
        assert!(js.contains("const b = m.size;"), "got: {js}");
    }

    #[test]
    fn delete_keys_valuesが生成できる() {
        let js = gen_body("fn main() {\n  m := map<string, int>{\"a\": 1}\n  delete(m, \"a\")\n  ks := keys(m)\n  vs := values(m)\n}");
        assert!(js.contains("m.delete(\"a\");"), "got: {js}");
        assert!(js.contains("const ks = Array.from(m.keys());"), "got: {js}");
        assert!(js.contains("const vs = Array.from(m.values());"), "got: {js}");
    }

    #[test]
    fn delete_を配列に使うと明確なerrになる() {
        let err = gen_js("fn main() {\n  mut xs := [1, 2, 3]\n  delete(xs, 0)\n}").unwrap_err();
        assert!(err.contains("delete"), "got: {err}");
    }

    #[test]
    fn get_sort_の生成jsを確認する() {
        let js = gen_body("fn main() {\n  xs := [3, 1, 2]\n  x := get(xs, 0)\n  s := sort(xs)\n}");
        assert!(js.contains("const x = __get(xs, 0);"), "got: {js}");
        assert!(js.contains("const s = __sorted(xs);"), "got: {js}");
    }

    #[test]
    fn chan生成はcapacityでnew_channelの引数が変わる() {
        let js = gen_body("fn main() {\n  a := chan<int>(none)\n  b := chan<int>(5)\n}");
        assert!(js.contains("const a = new __Channel();"), "got: {js}");
        assert!(js.contains("const b = new __Channel(5);"), "got: {js}");
    }

    #[test]
    fn recvはrecvヘルパを呼ぶjsになる() {
        let js = gen_body("fn main() {\n  ch := chan<int>(none)\n  v := <-ch\n  print(v)\n}");
        assert!(js.contains("const v = (await __recv(ch));"), "got: {js}");
    }

    #[test]
    fn sendはawaitch_sendになる() {
        let js = gen_body("fn main() {\n  ch := chan<int>(none)\n  ch <- 1\n}");
        assert!(js.contains("(await ch.send(1));"), "got: {js}");
    }

    #[test]
    fn 非chanへのsend_recvは明確なエラーになる() {
        let err1 = gen_js("fn main() {\n  x := 1\n  x <- 1\n}").unwrap_err();
        assert!(err1.contains("cannot send to non-channel"), "got: {err1}");
        let err2 = gen_js("fn main() {\n  x := 1\n  v := <-x\n  print(v)\n}").unwrap_err();
        assert!(err2.contains("cannot receive from non-channel"), "got: {err2}");
    }

    #[test]
    fn spawn自由関数はspawnヘルパになり関数丸ごとwait枠が付く() {
        let js = gen_body("fn f(x: int) {\n  print(x)\n}\nfn main() {\n  spawn f(1)\n}");
        assert!(js.contains("__spawn(f, [1]);"), "got: {js}");
        assert!(js.contains("__waitStack.push([]);"), "got: {js}");
        assert!(js.contains("} finally {"), "got: {js}");
        assert!(js.contains("await Promise.all(__waitStack.pop());"), "got: {js}");
    }

    #[test]
    fn detachはdetachヘルパになりwaitstackを使わない() {
        let js = gen_body("fn f(x: int) {\n  print(x)\n}\nfn main() {\n  detach f(1)\n}");
        assert!(js.contains("__detach(f, [1]);"), "got: {js}");
        assert!(!js.contains("__waitStack"), "got: {js}");
    }

    #[test]
    fn spawnのメソッド呼び出しはレシーバを引数先頭に回す() {
        let js = gen_body(
            "struct W {\n  id: int\n}\nfn (w: W) greet() string {\n  return \"hi\"\n}\nfn main() {\n  w := W{id: 1}\n  spawn w.greet()\n}",
        );
        assert!(js.contains("__spawn(__m_W_greet, [w]);"), "got: {js}");
    }

    #[test]
    fn spawnで存在しないメソッドは明確なエラーになる() {
        // milestone 17: TS版`inferCall`のrecv.method(args)分岐と同じ文言に統一済み
        let err = gen_js("struct W {\n  id: int\n}\nfn main() {\n  w := W{id: 1}\n  spawn w.bogus()\n}").unwrap_err();
        assert!(err.contains("W has no field or method 'bogus' (fields: id)"), "got: {err}");
    }

    #[test]
    fn selectはchannels_handlers_defaultの3配列になる() {
        let js = gen_body(
            "fn main() {\n  a := chan<int>(none)\n  b := chan<int>(none)\n  r := select {\n    v := <-a => v\n    v := <-b => v\n    _ => 0\n  }\n  print(r)\n}",
        );
        assert!(
            js.contains("__select([a, b], [(async (v) => v), (async (v) => v)], (async () => 0))"),
            "got: {js}"
        );
    }

    #[test]
    fn defaultなしのselectはnullになる() {
        let js = gen_body("fn main() {\n  a := chan<int>(none)\n  r := select {\n    v := <-a => v\n  }\n  print(r)\n}");
        assert!(js.contains("], null))"), "got: {js}");
    }

    #[test]
    fn select以外の型のアームchannelは明確なエラーになる() {
        let err = gen_js("fn main() {\n  x := 1\n  r := select {\n    v := <-x => v\n  }\n  print(r)\n}").unwrap_err();
        assert!(err.contains("select arm requires a channel"), "got: {err}");
    }

    #[test]
    fn selectの結果を使った添字_recvにも正しい型で安全ガードが効く() {
        // code review指摘: 以前はselectのアーム束縛名がスコープに宣言されず(checker.rs参照)、
        // bodyがその束縛名をそのまま返すとselect式全体の型が常にANYへ潰れていた——ANYは
        // 「確実に非map/非chanと分かる場合だけ弾く」設計の安全ガードを常に素通しするため、
        // select結果への添字/recvが本来効くべきガードをすり抜けていた。アーム束縛名が
        // 正しく型付けされ、ガードが機能することを確認する
        let err1 = gen_js(
            "fn main() {\n  a := chan<int[]>(none)\n  b := chan<int[]>(none)\n  close(b)\n  msg := select {\n    v := <-a => v\n    v := <-b => v\n  }\n  print(msg[0])\n}",
        )
        .unwrap_err();
        assert!(err1.contains("cannot index into"), "got: {err1}");

        let err2 = gen_js(
            "fn main() {\n  a := chan<chan<int>>(none)\n  b := chan<chan<int>>(none)\n  close(b)\n  msg := select {\n    v := <-a => v\n    v := <-b => v\n  }\n  x := <-msg\n  print(x)\n}",
        )
        .unwrap_err();
        assert!(err2.contains("cannot receive from non-channel"), "got: {err2}");
    }

    #[test]
    fn wait文はwaitstackで包む() {
        let js = gen_body("fn f() {\n  print(1)\n}\nfn main() {\n  wait {\n    spawn f()\n  }\n}");
        assert!(js.contains("__waitStack.push([]);"), "got: {js}");
        assert!(js.contains("try {"), "got: {js}");
        assert!(js.contains("__spawn(f, []);"), "got: {js}");
        assert!(js.contains("} finally {"), "got: {js}");
        assert!(js.contains("await Promise.all(__waitStack.pop());"), "got: {js}");
    }

    #[test]
    fn 明示的waitの中のspawnでも関数丸ごとの暗黙wait枠が付く() {
        // TS版と同じ「フラットなフラグ」挙動: spawnが明示的wait{}の中だけにあっても
        // 関数側のspawn_usedは立つため、__waitStack.push([]);が2回(外側の暗黙+内側の
        // 明示)現れる(内側は空のPromise.all([])という無害な冗長さになるだけ)
        let js = gen_body("fn f() {\n  print(1)\n}\nfn main() {\n  wait {\n    spawn f()\n  }\n}");
        let push_count = js.matches("__waitStack.push([]);").count();
        assert_eq!(push_count, 2, "got: {js}");
    }

    #[test]
    fn propとspawnを両方使う関数はtry_catch_finallyの順で包む() {
        let js = gen_body(
            "fn f() int | error {\n  return 1\n}\nfn g() {\n  print(1)\n}\nfn main() {\n  x := f()?\n  spawn g()\n  print(x)\n}",
        );
        assert!(js.contains("__waitStack.push([]);"), "got: {js}");
        assert!(js.contains("try {"), "got: {js}");
        assert!(js.contains("} catch (e) {"), "got: {js}");
        assert!(js.contains("if (e instanceof __Propagate) return e.value;"), "got: {js}");
        assert!(js.contains("} finally {"), "got: {js}");
        assert!(js.contains("await Promise.all(__waitStack.pop());"), "got: {js}");
        let catch_pos = js.find("} catch (e) {").unwrap();
        let finally_pos = js.find("} finally {").unwrap();
        assert!(catch_pos < finally_pos, "got: {js}");
    }

    #[test]
    fn spawn_usedは関数ごとにリセットされ次の関数へ漏れない() {
        let js = gen_body(
            "fn f() {\n  print(1)\n}\nfn hasSpawn() {\n  spawn f()\n}\nfn noSpawn() {\n  print(2)\n}\nfn main() {\n  hasSpawn()\n  noSpawn()\n}",
        );
        let no_spawn_body = js.split("async function noSpawn").nth(1).unwrap().split("async function main").next().unwrap();
        assert!(!no_spawn_body.contains("__waitStack"), "got: {no_spawn_body}");
    }

    // ---- milestone 6: 複数ファイル/パッケージ修飾 ----

    const OPS_MESH: &str = "export fn add(a: int, b: int) int {\n  return a + b\n}\nfn double(n: int) int {\n  return n * 2\n}\nexport fn quadruple(n: int) int {\n  return double(double(n))\n}\n";
    const POINT_MESH: &str = "export struct Point {\n  x: int\n  y: int\n}\nfn (p: Point) magnitudeSq() int {\n  return p.x * p.x + p.y * p.y\n}\nexport fn origin() Point {\n  return Point{x: 0, y: 0}\n}\n";

    #[test]
    fn パッケージ修飾の自由関数呼び出しはpkg接頭辞付きの名前になる() {
        let js = gen_modules_body(&[
            ("main", "main.mesh", "import \"mathutil\"\nfn main() {\n  x := mathutil.add(1, 2)\n  print(x)\n}"),
            ("mathutil", "mathutil/ops.mesh", OPS_MESH),
        ]);
        assert!(js.contains("const x = (await mathutil$add(1, 2));"), "got: {js}");
        assert!(js.contains("async function mathutil$add(a, b)"), "got: {js}");
    }

    #[test]
    fn mesh_json組み込みパッケージの関数呼び出しとvalue構築が解決できる() {
        // milestone 9: mesh/jsonは.messソースを持たない組み込みパッケージ(json_stdlib_symbols)。
        // gen_body(単一"main"パッケージ)経由でも他パッケージ同様に解決できることを確認する
        // (decode<X>合成〈json_decode.rs〉自体はmain.rsの仕事なので、ここでは合成後の
        // コードが実際に使う経路——関数呼び出し・struct literal構築——だけを確認する)
        let js = gen_body(
            "import \"mesh/json\"\nfn main() {\n  v := json.parse(\"1\") or _ => json.Value{kind: \"null\"}\n  print(json.stringify(v))\n}",
        );
        assert!(js.contains("(await json$parse(\"1\"))"), "got: {js}");
        assert!(js.contains("({ kind: \"null\" })"), "got: {js}"); // Valueはis_error_type無し、errTagで包まれない
        assert!(js.contains("(await json$stringify(v))"), "got: {js}");
    }

    #[test]
    fn json_valueは1階層のis絞り込みでentriesがmap型に推論されlenがsizeを選ぶ() {
        // code review発覚・実行確認済みの回帰(tests/e2e.test.ts:1146-1160、json struct機能
        // より前からある既存のmesh/json手書きdestructure): json.Valueを完全に不透明な殻
        // (フィールド無し)にすると、絞り込み後の`v.entries`がANY型になり`len()`が
        // `.length`(mapには存在せずundefinedになる)を選んでしまっていた。arr/objの
        // 再帰位置(items/entries)だけを不透明な殻に留め、それ以外の構造
        // (kind判別フィールド+実フィールド)は本物のmap/配列型にすることで、1階層の
        // 絞り込み+フィールドアクセスが正しい型(map)で推論され`len`が`.size`を選ぶ
        let js = gen_body(
            "import \"mesh/json\"\nfn main() {\n  v := json.parse(\"1\") or _ => json.Value{kind: \"null\"}\n  if v is { kind: \"obj\" } {\n    print(len(v.entries))\n  }\n}",
        );
        assert!(js.contains("v.entries.size"), "got: {js}");
    }

    #[test]
    fn mesh_jsonとユーザーパッケージのimportが共存できる() {
        let js = gen_modules_body(&[
            (
                "main",
                "main.mesh",
                "import \"mesh/json\"\nimport \"mathutil\"\nfn main() {\n  v := json.parse(\"1\") or _ => json.Value{kind: \"null\"}\n  print(mathutil.add(1, 2), json.stringify(v))\n}",
            ),
            ("mathutil", "mathutil/ops.mesh", OPS_MESH),
        ]);
        assert!(js.contains("(await mathutil$add(1, 2))"), "got: {js}");
        assert!(js.contains("(await json$stringify(v))"), "got: {js}");
    }

    #[test]
    fn 同一パッケージ内の複数ファイルはimport無しで互いに見える() {
        // ops.meshのquadruple(export)がdouble(未export)をimport無しで呼べる。
        // 未exportな自由関数もmainパッケージと衝突しないようpkg接頭辞が付く
        let js = gen_modules_body(&[
            ("main", "main.mesh", "import \"mathutil\"\nfn main() {\n  print(mathutil.quadruple(3))\n}"),
            ("mathutil", "mathutil/ops.mesh", OPS_MESH),
        ]);
        assert!(js.contains("async function mathutil$double(n)"), "got: {js}");
        assert!(js.contains("(await mathutil$double((await mathutil$double(n))));"), "got: {js}");
    }

    #[test]
    fn パッケージ修飾のstruct_literalと型注釈とメソッド呼び出しが解決できる() {
        let js = gen_modules_body(&[
            (
                "main",
                "main.mesh",
                "import \"mathutil\"\nfn main() {\n  p := mathutil.Point{x: 3, y: 4}\n  print(p.magnitudeSq())\n  q: mathutil.Point = mathutil.origin()\n  print(q.x)\n}",
            ),
            ("mathutil", "mathutil/point.mesh", POINT_MESH),
        ]);
        assert!(js.contains("const p = ({ x: 3, y: 4 });"), "got: {js}"); // struct名自体はJSに現れない
        assert!(js.contains("__m_mathutil$Point_magnitudeSq(p)"), "got: {js}");
        assert!(js.contains("const q = (await mathutil$origin());"), "got: {js}");
        assert!(js.contains("async function __m_mathutil$Point_magnitudeSq(p)"), "got: {js}");
    }

    // ---- milestone 16: pkg修飾struct literalの厳密検証 ----

    #[test]
    fn pkg修飾struct_literalのtypoしたフィールド名は明確なerrになりpkg修飾済みの名前で表示される() {
        // PR #17以来の既知の限界(milestone 12はpkg無し側だけ厳密化していた)を解消。
        // 表示名はTS版と同じくpkg修飾済みの型自身の名前("mathutil.Point")を使う
        // (union構築時はexpr自身が書いた素の名前を使う、下のテスト参照)
        let err = gen_modules(&[
            ("main", "main.mesh", "import \"mathutil\"\nfn main() {\n  p := mathutil.Point{x: 1, typo: 2}\n  print(p)\n}"),
            ("mathutil", "mathutil/point.mesh", POINT_MESH),
        ])
        .unwrap_err();
        assert!(err.contains("mathutil.Point has no field 'typo' (fields: x, y)"), "got: {err}");
    }

    #[test]
    fn pkg修飾struct_literalの欠落フィールドも明確なerrになる() {
        let err = gen_modules(&[
            ("main", "main.mesh", "import \"mathutil\"\nfn main() {\n  p := mathutil.Point{x: 1}\n  print(p)\n}"),
            ("mathutil", "mathutil/point.mesh", POINT_MESH),
        ])
        .unwrap_err();
        assert!(err.contains("missing field(s) in mathutil.Point: y"), "got: {err}");
    }

    #[test]
    fn pkg修飾error_type_unionの構築は判別可能unionのdisambiguation経由で正しくerrtagされる() {
        // milestone 8の"all"ヒューリスティック(type_is_error_instance)に代わり、
        // pkg修飾側もmilestone 12のタグdisambiguation経由で特定した具体的なメンバー自身の
        // is_error_typeを見る、より正確な経路を通るようになった
        const ERR_MESH: &str = "export error type DbError = { kind: \"notFound\", table: string } | { kind: \"timeout\" }\n";
        let js = gen_modules_body(&[
            (
                "main",
                "main.mesh",
                "import \"errpkg\"\nfn main() {\n  e := errpkg.DbError{kind: \"notFound\", table: \"users\"}\n  print(e)\n}",
            ),
            ("errpkg", "errpkg/err.mesh", ERR_MESH),
        ]);
        assert!(js.contains("__errTag({ kind: \"notFound\", table: \"users\" })"), "got: {js}");
    }

    #[test]
    fn pkg修飾判別可能unionのタグ値不一致は明確なerrになりunion自身の素の名前で表示される() {
        // 組み込みパッケージ(mesh/json)のjson.Valueも同じ経路を通ることを実行確認済み
        // (git historyレビューが指摘していた"json.Value{kind: \"bogus\", ...}"が
        // 無検証で素通りしていた穴も、この修正でまとめて閉じる)
        const RESP_MESH: &str =
            "export type GetUserResponse = { kind: \"ok\", value: int } | { kind: \"notFound\" }\n";
        let err = gen_modules(&[
            (
                "main",
                "main.mesh",
                "import \"userpkg\"\nfn main() {\n  r := userpkg.GetUserResponse{kind: \"bogus\"}\n  print(r)\n}",
            ),
            ("userpkg", "userpkg/resp.mesh", RESP_MESH),
        ])
        .unwrap_err();
        // displayNameは(unionなので)pkg修飾されない、TS版の`expr.name`と同じ挙動
        assert!(err.contains("no member of 'GetUserResponse' has kind: \"bogus\""), "got: {err}");
    }

    #[test]
    fn json_valueの自己参照する再帰位置への実際の構築は今まで通りコンパイルできる() {
        // git historyレビュー発覚・実行確認済みの回帰: json.Valueのarr/objは自己参照する
        // 再帰位置(items/entries)を空フィールドの不透明な殻(milestone 9)として表すため、
        // このmilestoneが追加したフィールド型検証がunion全体を渡そうとする側の型と
        // 一致せず、`json.Value{kind:"arr", items:[json.Value{...}, ...]}`のような
        // 正当な構築まで誤ってtype-mismatchにしてしまっていた
        let js = gen_body(
            "import \"mesh/json\"\nfn main() {\n  v := json.Value{kind: \"arr\", items: [\n    json.Value{kind: \"num\", n: 1.0},\n    json.Value{kind: \"null\"},\n  ]}\n  print(json.stringify(v))\n}",
        );
        assert!(js.contains("items: [({ kind: \"num\", n: 1.0 }), ({ kind: \"null\" })]"), "got: {js}");

        let js2 = gen_body(
            "import \"mesh/json\"\nfn main() {\n  m := map<string, json.Value>{\"a\": json.Value{kind: \"num\", n: 1.0}}\n  v := json.Value{kind: \"obj\", entries: m}\n  print(json.stringify(v))\n}",
        );
        assert!(js2.contains("entries: m"), "got: {js2}");
    }

    const DBERROR_MESH: &str = "export error type DbError = { kind: \"notFound\", table: string } | { kind: \"timeout\", ms: int }\nexport fn find(id: int) int | DbError {\n  if id == 1 {\n    return 100\n  }\n  return DbError{kind: \"notFound\", table: \"users\"}\n}\n";

    #[test]
    fn パッケージ修飾のerror_type_union形式はerrtag付きで構築できる() {
        // code review発覚・実行確認済みの回帰: exportedなunion型alias(error type含む)が
        // パッケージレジストリに登録されておらず、パッケージ越しの構築が
        // 「no exported struct」という明確なErrになっていた(TS版は成功する)
        let js = gen_modules_body(&[
            (
                "main",
                "main.mesh",
                "import \"errpkg\"\nfn main() {\n  e := errpkg.DbError{kind: \"notFound\", table: \"users\"}\n  print(e)\n}",
            ),
            ("errpkg", "errpkg/errs.mesh", DBERROR_MESH),
        ]);
        assert!(js.contains("const e = __errTag({ kind: \"notFound\", table: \"users\" });"), "got: {js}");
    }

    #[test]
    fn パッケージ修飾のerror_type_union形式でも構造化errorへの文脈付きは安全ガードが効く() {
        // code review発覚・実行確認済みの回帰(より深刻な方): 修正前はpkg修飾された戻り値型
        // 注釈(`int | errpkg.DbError`)がis_error_typeの付かない殻structへ静かに
        // フォールバックしており、has_structured_failureの安全ガード(checker.rs参照)が
        // 素通りしてしまっていた。結果、文脈付き`?`が構造化errorに対してコンパイルを
        // 通ってしまい、実行時に__propCtxがnull/instanceof Errorしか見ないため
        // 構造化errorを「成功扱い」して静かに壊れた挙動になっていた
        let err = gen_modules(&[
            (
                "main",
                "main.mesh",
                "import \"errpkg\"\nfn wrap(id: int) int | errpkg.DbError {\n  return errpkg.find(id)\n}\nfn useIt(id: int) int | errpkg.DbError {\n  v := wrap(id) ? \"lookup failed\"\n  return v\n}\nfn main() {}",
            ),
            ("errpkg", "errpkg/errs.mesh", DBERROR_MESH),
        ])
        .unwrap_err();
        assert!(err.contains("cannot propagate a structured error"), "got: {err}");
    }

    #[test]
    fn 未exportな関数をパッケージ修飾で呼ぶと明確なエラーになる() {
        let err = gen_modules(&[
            ("main", "main.mesh", "import \"mathutil\"\nfn main() {\n  print(mathutil.double(3))\n}"),
            ("mathutil", "mathutil/ops.mesh", OPS_MESH),
        ])
        .unwrap_err();
        assert!(err.contains("package 'mathutil' has no exported function 'double'"), "got: {err}");
    }

    #[test]
    fn ローカル変数がパッケージエイリアスをshadowするとパッケージ修飾扱いされない() {
        // "mathutil"という名前のローカル変数がある場合、mathutil.add(...)はパッケージ
        // 修飾ではなく通常のメンバー呼び出しとして扱われる(TS版tryPackageMemberと同じ
        // 優先順位——ローカル変数が勝つ)。ここでは"mathutil"はint(structではない)なので
        // 明確なエラーになる——パッケージのadd関数が誤って呼ばれることはない、という確認
        let err = gen_modules(&[
            (
                "main",
                "main.mesh",
                "import \"mathutil\"\nfn main() {\n  mathutil := 1\n  print(mathutil.add(1, 2))\n}",
            ),
            ("mathutil", "mathutil/ops.mesh", OPS_MESH),
        ])
        .unwrap_err();
        assert!(err.contains("package/member access is not yet supported"), "got: {err}");
    }

    #[test]
    fn 存在しないパッケージへの参照は明確なエラーになる() {
        // importしていない("mathutil"を一切importしない)パッケージへの参照は
        // ローカルスコープに無くimport_aliasesにも無いので、パッケージ参照とは判定されず、
        // 通常のフィールドアクセス扱いとなり明確なエラーになる(クラッシュしない)
        let err = gen_js("fn main() {\n  x := mathutil.add(1, 2)\n  print(x)\n}").unwrap_err();
        assert!(!err.is_empty(), "got: {err}");
    }

    #[test]
    fn パッケージ間のimport循環は明確なエラーになる() {
        let err = gen_modules(&[
            ("main", "main.mesh", "import \"a\"\nfn main() {}"),
            ("a", "a/a.mesh", "import \"b\"\nexport fn f() {}"),
            ("b", "b/b.mesh", "import \"a\"\nexport fn g() {}"),
        ])
        .unwrap_err();
        assert!(err.contains("import cycle"), "got: {err}");
    }

    #[test]
    fn spawn_detachでのパッケージ修飾自由関数呼び出しが解決できる() {
        // code review指摘: gen_call(通常呼び出し)はパッケージ修飾を解決していたが、
        // gen_spawnが使うresolve_free_fn_valueには同じ分岐が無く、`spawn mathutil.add(...)`
        // が素の関数値を得られず「package/member access is not yet supported」という
        // 紛らわしいエラーになっていた
        let js = gen_modules_body(&[
            (
                "main",
                "main.mesh",
                "import \"mathutil\"\nfn main() {\n  spawn mathutil.add(3, 4)\n  detach mathutil.add(5, 6)\n  print(\"ok\")\n}",
            ),
            ("mathutil", "mathutil/ops.mesh", OPS_MESH),
        ]);
        assert!(js.contains("__spawn(mathutil$add, [3, 4]);"), "got: {js}");
        assert!(js.contains("__detach(mathutil$add, [5, 6]);"), "got: {js}");
    }

    #[test]
    fn 未exportな関数をspawnでパッケージ修飾呼び出しすると明確なエラーになる() {
        let err = gen_modules(&[
            ("main", "main.mesh", "import \"mathutil\"\nfn main() {\n  spawn mathutil.double(3)\n}"),
            ("mathutil", "mathutil/ops.mesh", OPS_MESH),
        ])
        .unwrap_err();
        assert!(err.contains("package 'mathutil' has no exported function 'double'"), "got: {err}");
    }

    #[test]
    fn 別パッケージのstructと同じ素の名前でも誤った循環エラーにならない() {
        // code review指摘: pkg修飾された型注釈(otherpkg.Point)を循環検出が素の名前
        // (Point)だけで見ていたため、同一パッケージ内にたまたま同じ素の名前のstructが
        // あると無関係な相互参照だと誤認し、実際には循環していないのに
        // 「self-referential/cyclic struct」という誤ったエラーになっていた
        let js = gen_modules_body(&[
            ("main", "main.mesh", "import \"otherpkg\"\nstruct Point {\n  other: otherpkg.Point\n}\nfn main() {\n  print(1)\n}"),
            ("otherpkg", "otherpkg/p.mesh", "export struct Point {\n  x: int\n}\n"),
        ]);
        assert!(js.contains("async function main()"), "got: {js}");
    }

    #[test]
    fn 複数パッケージにまたがる同名のトップレベルconstは明確なエラーになる() {
        // code review指摘: トップレベル関数/メソッドはpkg接頭辞で衝突しないが、constは
        // 無修飾のまま生成するため、2つのパッケージが同じ名前のトップレベルconstを
        // 宣言すると生成JSの同じフラットスコープに同名のconst宣言が2つ現れ、
        // 実行時クラッシュではなくJS自体がパースできない(SyntaxError)という
        // もっと悪い壊れ方をする。静かに`Ok(js)`を返さず明確なErrにする
        let err = gen_modules(&[
            ("main", "main.mesh", "import \"pkgb\"\ndebug := 1\nfn main() {\n  print(debug)\n  print(pkgb.f())\n}"),
            ("pkgb", "pkgb/b.mesh", "debug := 2\nexport fn f() int {\n  return debug\n}"),
        ])
        .unwrap_err();
        assert!(err.contains("top-level const 'debug' is declared more than once"), "got: {err}");
    }

    #[test]
    fn import宣言していないパッケージのstruct_literalは明確なエラーになる() {
        // code review指摘: is_package_aliasを確認せずレジストリを直接引いていたため、
        // importを宣言していない(が別経路でロードされてさえいる)パッケージの
        // struct literalも解決できてしまっていた——依存グラフの循環検出はimport文だけを
        // 見て構築されるため、import文を経由しないパッケージ間参照を許すと循環検出を
        // すり抜けられてしまう
        let err = gen_modules(&[
            ("main", "main.mesh", "import \"a\"\nfn main() {\n  x := b.Bar{}\n  print(x)\n}"),
            ("a", "a/a.mesh", "export fn f() {}"),
            ("b", "b/b.mesh", "export struct Bar {}"),
        ])
        .unwrap_err();
        assert!(err.contains("package 'b' has no exported struct 'Bar'"), "got: {err}");
    }

    // ---- milestone 7: match/is式・判別可能union ----

    #[test]
    fn is式は裸identならそのままテストを組み立てる() {
        let js = gen_body("fn main() {\n  x := 1\n  ok := x is int\n  print(ok)\n}");
        assert!(js.contains("const ok = Number.isInteger(x);"), "got: {js}");
    }

    #[test]
    fn is式は非identなら二重評価を避けるiifeで包む() {
        let js = gen_body("fn f() int {\n  return 1\n}\nfn main() {\n  ok := f() is int\n  print(ok)\n}");
        assert!(js.contains("((__v) => Number.isInteger(__v))((await f()))"), "got: {js}");
    }

    #[test]
    fn is式のstruct形パターンはオブジェクト形テストとフィールドテストを組み立てる() {
        let js = gen_body(
            "type Resp = { kind: \"ok\" } | { kind: \"err\" }\nfn main() {\n  r := Resp{kind: \"ok\"}\n  ok := r is { kind: \"ok\" }\n  print(ok)\n}",
        );
        assert!(
            js.contains("const ok = (typeof r === \"object\" && r !== null && !(r instanceof Error) && !Array.isArray(r)) && (r.kind === \"ok\");"),
            "got: {js}"
        );
    }

    #[test]
    fn union型aliasの名前でstruct_literalを構築できる() {
        let js = gen_body("type Resp = { kind: \"ok\" } | { kind: \"err\" }\nfn main() {\n  r := Resp{kind: \"ok\"}\n  print(r)\n}");
        assert!(js.contains("const r = ({ kind: \"ok\" });"), "got: {js}");
    }

    #[test]
    fn error_typeのunion形式で構築したstruct_literalはerrtagでラップされる() {
        let js = gen_body(
            "error type DbError = { kind: \"notFound\", table: string } | { kind: \"timeout\", ms: int }\nfn main() {\n  e := DbError{kind: \"notFound\", table: \"users\"}\n  print(e)\n}",
        );
        assert!(js.contains("const e = __errTag({ kind: \"notFound\", table: \"users\" });"), "got: {js}");
    }

    #[test]
    fn error_type_unionと通常structを混ぜたさらに外側のunionでは成功値をerrtagで包まない() {
        // code review発覚・実行確認済みの回帰: "any"判定だと、DbError由来のタグ付き
        // メンバーが1つでも混じっているだけでSuccess{value:...}という普通の成功値まで
        // errTagで包んでしまい、?/orが成功値を誤って失敗として握り潰していた。
        // milestone 12で判明: `Result{value: 42}`(union自身の名前での構築)はTS版でも
        // 実際にdiscriminated-union-tag-missingで拒否される——DbErrorの2つの無名メンバーが
        // Resultにもタグ("kind")を要求し、無名メンバー以外(Successのような名前付きメンバー)は
        // タグ経由のdisambiguationの対象外になるため。有効な構築方法は具体的なstructの
        // 名前(`Success{...}`)を使うことで、TS版で実行確認済み(§計画参照)
        let js = gen_body(
            "error type DbError = { kind: \"notFound\" } | { kind: \"timeout\" }\nstruct Success { value: int }\ntype Result = Success | DbError\nfn getIt() Result {\n  return Success{value: 42}\n}\nfn main() {\n  print(getIt())\n}",
        );
        assert!(js.contains("return ({ value: 42 });"), "got: {js}");
        assert!(!js.contains("__errTag"), "got: {js}");
    }

    #[test]
    fn matchはexhaustiveならts版と同じ三項演算子の連鎖になり最後のアームは無条件() {
        let js = gen_body(
            "type Status = \"active\" | \"banned\"\nfn label(s: Status) string {\n  return match s {\n    \"active\" => \"a\"\n    \"banned\" => \"b\"\n  }\n}\nfn main() {\n  print(label(\"active\"))\n}",
        );
        assert!(js.contains("(await (async (__m) => (__m === \"active\") ? \"a\" : \"b\")(s))"), "got: {js}");
    }

    #[test]
    fn matchが非exhaustiveなら最後のアームにもテストを付け不一致時にpanicする() {
        // code review的な観点(milestone 2〜6と同じ「TS本体は診断で到達不能」パターン):
        // TS本体はexhaustiveness診断でこの入力自体を拒否するが、診断を出さないこの
        // リゾルバでは実際に到達しうるため、Rust版だけの安全ガードとしてpanicを追加する
        let js = gen_body(
            "type Status = \"active\" | \"banned\" | \"pending\"\nfn label(s: Status) string {\n  return match s {\n    \"active\" => \"a\"\n  }\n}\nfn main() {\n  print(label(\"active\"))\n}",
        );
        assert!(js.contains("(__m === \"active\") ? \"a\" : (() => { throw new __Panic("), "got: {js}");
        assert!(js.contains("no arm of this match matched"), "got: {js}");
    }

    #[test]
    fn matchのnarrowingでフィールドアクセスがiarithを選ぶ() {
        let js = gen_body(
            "type Resp = { kind: \"ok\", value: int } | { kind: \"err\" }\nfn describe(r: Resp) int {\n  return match r {\n    { kind: \"ok\" } => r.value + 1\n    { kind: \"err\" } => 0\n  }\n}\nfn main() {\n  print(describe(Resp{kind: \"ok\", value: 1}))\n}",
        );
        assert!(js.contains("__iarith(r.value, \"+\", 1,"), "got: {js}");
    }

    #[test]
    fn if_isのnarrowingはthen節fallthrough節else節いずれでもiarithを選ぶ() {
        let js_fallthrough =
            gen_body("fn f(x: int | error) int {\n  if x is error {\n    return 0\n  }\n  return x + 1\n}\nfn main() {\n  print(1)\n}");
        assert!(js_fallthrough.contains("__iarith(x, \"+\", 1,"), "got: {js_fallthrough}");

        let js_else = gen_body(
            "fn g(x: int | error) int {\n  if x is error {\n    return 0\n  } else {\n    return x + 1\n  }\n}\nfn main() {\n  print(1)\n}",
        );
        assert!(js_else.contains("__iarith(x, \"+\", 1,"), "got: {js_else}");
    }

    #[test]
    fn 自己参照するunion型aliasは明確なエラーになる() {
        let err = gen_js("type Tree = { kind: \"leaf\" } | { kind: \"node\", left: Tree, right: Tree }\nfn main() {}").unwrap_err();
        assert!(err.contains("self-referential/cyclic type definitions"), "got: {err}");
    }

    #[test]
    fn アーム0個のmatchは非union_subjectでもpanicのみの正しい構文のjsになる() {
        // 回帰テスト: subjectがUnion以外だと`match_is_exhaustive`が無条件trueを返して
        // いた頃は、空のアーム本体がそのまま生成され構文的に壊れたJS
        // (`async (__m) => `)になっていた
        let js = gen_body("fn main() {\n  n := 5\n  r := match n {}\n  print(r)\n}");
        assert!(js.contains("(await (async (__m) => (() => { throw new __Panic("), "got: {js}");
        assert!(!js.contains("=> )("), "got: {js}");
    }

    #[test]
    fn if_isのnarrowingはelse節がありthen節が終端する場合も後続文へ伝播する() {
        // 回帰テスト: else節がある場合の絞り込み伝播が抜けていて、if/elseの後の文が
        // 絞り込み前のUnion型のまま扱われ__idivではなく素の`/`が生成されていた
        let js = gen_body(
            "fn g(x: int | error) int {\n  if x is error {\n    return 0\n  } else {\n    print(\"ok\")\n  }\n  return x / 2\n}\nfn main() {\n  print(1)\n}",
        );
        assert!(js.contains("__idiv(x, 2,"), "got: {js}");
    }

    #[test]
    fn 無名関数式は即時評価可能な非同期アロー関数として生成される() {
        let js = gen_body("fn main() {\n  double := fn(n: int) int { return n * 2 }\n  print(double(3))\n}");
        assert!(js.contains("const double = (async (n) => {"), "got: {js}");
        assert!(js.contains("__iarith(n, \"*\", 2,"), "got: {js}"); // 無名関数の中でも型推論が効きintの乗算にiarithが選ばれる
        assert!(js.contains("(await double(3))"), "got: {js}"); // ユーザー定義関数(無名関数含む)呼び出しは常にawait
    }

    #[test]
    fn filter_map_reduceはランタイムヘルパ呼び出しに変換され無名関数を引数に取れる() {
        let js = gen_body(
            "fn main() {\n  nums := [1, 2, 3]\n  evens := filter(nums, fn(n: int) bool { return n % 2 == 0 })\n  doubled := map(nums, fn(n: int) int { return n * 2 })\n  total := reduce(nums, fn(acc: int, n: int) int { return acc + n }, 0)\n  print(evens, doubled, total)\n}",
        );
        assert!(js.contains("(await __filter(nums, (async (n) => {"), "got: {js}");
        assert!(js.contains("(await __map(nums, (async (n) => {"), "got: {js}");
        assert!(js.contains("(await __reduce(nums, (async (acc, n) => {"), "got: {js}");
    }

    #[test]
    fn 無名関数式の本体のprop_spawn使用は外側の関数を汚さずネストで独立に扱われる() {
        // 回帰テスト観点: milestone 10でprop_used/spawn_usedを単一フラグからスタックに
        // 変えたのは、まさにこの「無名関数の中の?/spawnが外側の関数のtry/catch/finally
        // 判定に漏れてはいけない(逆も同様)」を保証するため
        let js = gen_body(
            "fn f() int | error {\n  return 1\n}\nfn main() {\n  g := fn() int {\n    return f()?\n  }\n  print(g())\n}",
        );
        // 無名関数の内側だけがtry/catchで包まれる(?を使っているため)
        assert!(js.contains("const g = (async () => {"), "got: {js}");
        assert!(js.contains("try {\n      return __prop((await f()));"), "got: {js}");
        assert!(js.contains("if (e instanceof __Propagate) return e.value;"), "got: {js}");
        // 外側のmainは?を使っていないのでtry/catchで包まれない(const g = ...の直後にprintが続く)
        assert!(js.contains("});\n  __print((await g()));"), "got: {js}");
    }

    #[test]
    fn 無名関数式の中のspawnは無名関数自身のwait枠だけを付け外側へ漏れない() {
        let js = gen_body("fn f() {\n  print(1)\n}\nfn main() {\n  g := fn() {\n    spawn f()\n  }\n  g()\n}");
        let push_count = js.matches("__waitStack.push([]);").count();
        assert_eq!(push_count, 1, "got: {js}"); // 無名関数の中の1回だけ(外側のmainはspawnを使っていない)
    }

    #[test]
    fn 入れ子になった無名関数式は改行を跨いで正しく再インデントされる() {
        // code review発覚・実行確認済みの回帰(2エージェント独立指摘): 内側のExpr::FnExprが
        // 埋め込む複数行の文字列は、外側の再インデント処理から見ると`body_lines`の
        // 1要素の中に改行を内包する形になる——各要素へ`pad`を1回ずつ足すだけだと
        // 2行目以降が余分なindentを受け取れず、TS版の出力(全体を一旦結合してから
        // 改めて改行分割してpadを付け直す)と食い違っていた
        let js = gen_body(
            "fn main() {\n  nums := [1, 2, 3]\n  result := map(nums, fn(n: int) int {\n    doubled := map([n], fn(m: int) int { return m * 2 })\n    return doubled[0]\n  })\n  print(result)\n}",
        );
        assert!(
            js.contains("      return __iarith(m, \"*\", 2,"),
            "内側のExpr::FnExprの本体は外側より2段深くインデントされるべき、got: {js}"
        );
        assert!(js.contains("    })));\n    return __idx(doubled, 0,"), "got: {js}");
    }

    #[test]
    fn preludeのtoint正規表現はテンプレートリテラルのエスケープを評価済みになる() {
        // code review発覚・実行確認済みの回帰: runtime.tsのソース上は(外側のテンプレート
        // リテラルが1段エスケープを解決する前提で)`\\d`と2つ重ねて書かれているが、
        // Rust版は単純な部分文字列抽出しかしておらずソースの`\\d`をそのまま出力していた
        // ため、生成JSの正規表現が`\\d`(バックスラッシュ文字自体を要求する、実質何にも
        // マッチしない)になり`toInt`が常に失敗していた
        let js = gen_js("fn main() {}").unwrap();
        assert!(js.contains("/^[+-]?\\d+$/"), "got prelude toInt regex context: {js}");
        assert!(!js.contains("\\\\d"), "got: {js}");
    }

    #[test]
    fn 複数のdeferはlifo順で関数を抜けるときに実行される() {
        let js = gen_body("fn work() {\n  defer print(\"first\")\n  defer print(\"second\")\n  defer print(\"third\")\n  print(\"body\")\n}\nfn main() {\n  work()\n}");
        assert!(js.contains("const __defers = [];"), "got: {js}");
        // 各defer文の引数(文字列リテラルでも)は一時変数へ捕捉される(TS版と同じ)
        let push_first = js.find("__d0 = \"first\"").unwrap();
        let push_second = js.find("__d1 = \"second\"").unwrap();
        let push_third = js.find("__d2 = \"third\"").unwrap();
        assert!(push_first < push_second && push_second < push_third, "登録順(first, second, third)のはず, got: {js}");
        assert!(js.contains("for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();"), "got: {js}");
    }

    #[test]
    fn deferの引数はdefer文の時点の値で一時変数へ捕捉される() {
        let js = gen_body("fn work() {\n  mut n := 1\n  defer print(n)\n  n = 99\n}\nfn main() {\n  work()\n}");
        assert!(js.contains("const __d0 = n;"), "got: {js}");
        assert!(js.contains("__defers.push(async () => { __print(__d0); });"), "got: {js}");
    }

    #[test]
    fn deferのメソッド呼び出しはレシーバも一時変数へ捕捉される() {
        let js = gen_body(
            "struct Box {\n  label: string\n}\nfn (b: Box) announce() {\n  print(b.label)\n}\nfn work() {\n  mut b := Box{label: \"first\"}\n  defer b.announce()\n  b = Box{label: \"second\"}\n}\nfn main() {\n  work()\n}",
        );
        assert!(js.contains("const __d0 = b;"), "got: {js}");
        assert!(js.contains("__defers.push(async () => { (await __m_Box_announce(__d0)); });"), "got: {js}");
    }

    #[test]
    fn 組み込み関数もパッケージ修飾関数もdeferできる() {
        let js = gen_body("fn main() {\n  ch := chan<int>(1)\n  defer close(ch)\n}");
        assert!(js.contains("const __d0 = ch;"), "got: {js}");
        assert!(js.contains("__defers.push(async () => { __d0.close(); });"), "got: {js}");

        let js2 = gen_modules_body(&[
            ("main", "main.mesh", "import \"mathutil\"\nfn main() {\n  defer mathutil.add(1, 2)\n}"),
            ("mathutil", "mathutil/ops.mesh", OPS_MESH),
        ]);
        assert!(js2.contains("__defers.push(async () => { (await mathutil$add(__d0, __d1)); });"), "got: {js2}");
    }

    #[test]
    fn deferでない式は明確なerrになる() {
        let err = gen_js("fn main() {\n  defer 1 + 1\n}").unwrap_err();
        assert!(err.contains("'defer' must be followed by a function or method call"), "got: {err}");
    }

    #[test]
    fn deferした呼び出しの影武者call式は元のcall式自身の位置情報を使う() {
        // code review発覚・実行確認済みの回帰: 影武者call式にdefer文自体の位置を
        // 使っていたため、defer先の組み込み呼び出しが埋め込むパニック位置情報が
        // (TS版と違い)`defer`キーワードの位置を指してしまっていた
        let js = gen_body("fn main() {\n  x := 1.5\n  defer round(x)\n}");
        assert!(js.contains("\"t.mesh:3:9\""), "元のround(...)呼び出し自身の位置(3行9列目)を指すべき, got: {js}");
        assert!(!js.contains("\"t.mesh:3:2\""), "defer文自体の位置(3行2列目)を指してはいけない, got: {js}");
    }

    #[test]
    fn 無名関数式の中のdeferは外側の関数を汚さず独立したdefers配列を持つ() {
        let js = gen_body("fn f() {\n  print(1)\n}\nfn main() {\n  g := fn() {\n    defer f()\n  }\n  g()\n  print(2)\n}");
        let defers_count = js.matches("const __defers = [];").count();
        assert_eq!(defers_count, 1, "got: {js}"); // 無名関数の中の1回だけ(外側のmainはdeferを使っていない)
        assert!(js.contains("(await g());\n  __print(2);"), "got: {js}"); // mainの残りの文はtry/finallyで包まれない(2段インデントのまま)
    }

    #[test]
    fn spawnとdeferを併用するとfinally内でspawn待ちの後にdeferが実行される() {
        let js = gen_body("fn f() {\n  print(1)\n}\nfn main() {\n  defer print(\"cleanup\")\n  spawn f()\n}");
        let wait_pos = js.find("await Promise.all(__waitStack.pop());").unwrap();
        let defer_pos = js.find("for (let __i = __defers.length - 1;").unwrap();
        assert!(wait_pos < defer_pos, "spawn待ちがdefer実行より先のはず, got: {js}");
    }

    // ---- milestone 12: struct literalのフィールド検証 ----

    #[test]
    fn struct_litのtypoしたフィールド名は明確なerrになる() {
        // PR #17以来の既知の穴: 以前はフィールド名が一切照合されず、typoが黙って
        // 素通りしていた(`nmae`のようなtypoが実行時までエラーにならなかった)
        let err = gen_js("struct User {\n  name: string\n}\nfn main() {\n  u := User{nmae: \"a\"}\n  print(u)\n}").unwrap_err();
        assert!(err.contains("no field 'nmae'"), "got: {err}");
    }

    #[test]
    fn struct_litの欠落フィールドは明確なerrになる() {
        let err = gen_js("struct User {\n  name: string\n  age: int\n}\nfn main() {\n  u := User{name: \"a\"}\n  print(u)\n}").unwrap_err();
        assert!(err.contains("missing field(s) in User: age"), "got: {err}");
    }

    #[test]
    fn struct_litの型不一致は明確なerrになる() {
        let err = gen_js("struct User {\n  age: int\n}\nfn main() {\n  u := User{age: \"old\"}\n  print(u)\n}").unwrap_err();
        assert!(err.contains("cannot use"), "got: {err}");
    }

    #[test]
    fn struct_litの重複フィールドは明確なerrになる() {
        let err = gen_js("struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\", name: \"b\"}\n  print(u)\n}").unwrap_err();
        assert!(err.contains("duplicate field 'name'"), "got: {err}");
    }

    #[test]
    fn struct_litはint型フィールドにfloatリテラルを渡すとerrになるがintはfloatフィールドへ渡せる() {
        assert!(gen_js("struct P {\n  x: int\n}\nfn main() {\n  p := P{x: 1.5}\n  print(p)\n}").is_err());
        let js = gen_body("struct P {\n  x: float\n}\nfn main() {\n  p := P{x: 1}\n  print(p)\n}");
        assert!(js.contains("({ x: 1 })"), "got: {js}");
    }

    #[test]
    fn 判別可能unionは正しいタグ値で構築でき間違ったタグ値はerrになる() {
        let src_ok = "type Resp = { kind: \"ok\", value: int } | { kind: \"err\" }\nfn main() {\n  r := Resp{kind: \"ok\", value: 1}\n  print(r)\n}";
        let js = gen_body(src_ok);
        assert!(js.contains("({ kind: \"ok\", value: 1 })"), "got: {js}");

        // 以前(milestone 11以前)は静かに素通りしていた穴: タグ値がどのメンバーとも
        // 一致しないstruct literalが黙って構築できてしまっていた
        let err = gen_js(
            "type Resp = { kind: \"ok\", value: int } | { kind: \"err\" }\nfn main() {\n  r := Resp{kind: \"unknown\"}\n  print(r)\n}",
        )
        .unwrap_err();
        assert!(err.contains("no member of 'Resp' has kind"), "got: {err}");

        let err_missing_tag = gen_js(
            "type Resp = { kind: \"ok\", value: int } | { kind: \"err\" }\nfn main() {\n  r := Resp{value: 1}\n  print(r)\n}",
        )
        .unwrap_err();
        assert!(err_missing_tag.contains("needs its tag field 'kind'"), "got: {err_missing_tag}");
    }

    #[test]
    fn error_type_unionの各メンバーは判別可能unionのdisambiguation経由で正しくerrtagされる() {
        // milestone 8の"all"ヒューリスティックに代わり、milestone 12はタグdisambiguationで
        // 特定した具体的なメンバー自身のis_error_typeを見る、より正確な経路を通る
        let src = "error type DbError = { kind: \"notFound\", table: string } | { kind: \"timeout\" }\nfn main() {\n  e := DbError{kind: \"notFound\", table: \"users\"}\n  print(e)\n}";
        let js = gen_body(src);
        assert!(js.contains("__errTag({ kind: \"notFound\", table: \"users\" })"), "got: {js}");
    }

    // ---- milestone 13: 算術演算子の妥当性検査(is_numericのUnion/ANY問題) ----

    #[test]
    fn 未絞り込みのchan受信結果への算術はerrになる() {
        // 以前は静かに素通りしてJSの浮動小数点`/`を生成していた(本来Meshの切り捨て
        // 除算`__idiv`が必要な`int`のはずが、`int | closed`という未絞り込みのunion型の
        // ままだったため、is_numericのUnion非対応でチェックをすり抜けていた)
        let err = gen_js(
            "fn main() {\n  ch := chan<int>(1)\n  ch <- 7\n  x := <-ch\n  y := x / 2\n  print(y)\n}",
        )
        .unwrap_err();
        assert!(err.contains("invalid operation"), "got: {err}");
    }

    #[test]
    fn 未絞り込みのmap読み取り結果への算術はerrになる() {
        let err = gen_js(
            "fn main() {\n  m := map<string, int>{\"a\": 1}\n  x := m[\"a\"]\n  y := x + 1\n  print(y)\n}",
        )
        .unwrap_err();
        assert!(err.contains("invalid operation"), "got: {err}");
    }

    #[test]
    fn bool同士の引き算のような無効な算術はerrになる() {
        let err = gen_js("fn main() {\n  y := true - false\n  print(y)\n}").unwrap_err();
        assert!(err.contains("invalid operation"), "got: {err}");
    }

    #[test]
    fn is_closedで絞り込んだ後のchan受信結果への算術は今まで通りidivを経由する() {
        let js = gen_body(
            "fn main() {\n  ch := chan<int>(1)\n  ch <- 7\n  v := <-ch\n  if v is closed {\n    return\n  }\n  y := v / 2\n  print(y)\n}",
        );
        assert!(js.contains("__idiv(v, 2,"), "got: {js}");
    }

    #[test]
    fn orで絞り込んだ後のmap読み取り結果への算術は今まで通りiarithを経由する() {
        let js = gen_body(
            "fn main() {\n  m := map<string, int>{\"a\": 1}\n  x := m[\"a\"] or 0\n  y := x + 1\n  print(y)\n}",
        );
        assert!(js.contains("__iarith(x, \"+\", 1,"), "got: {js}");
    }

    #[test]
    fn any型が絡む算術は相手がstruct等の非数値型でも常に許可される() {
        // 未宣言の識別子はANYへフォールバックする(infer_exprの既存挙動)。TS版と同じ
        // 「is_numeric分岐の外側のANY安全弁」がここで効いていることの確認
        // (無ければ`ANY + struct`が誤ってErrになってしまう)
        let js = gen_body("struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  print(undeclaredVar + u)\n}");
        assert!(js.contains("(undeclaredVar + u)"), "got: {js}");
    }

    #[test]
    fn 文字列と非文字列のplusはstr変換のヒント付きerrになる() {
        // code review指摘で発覚した移植漏れ(TS版checkArithOpの唯一のhintメッセージ)。
        // リテラル型(`"count: "`のような`:=`で束縛した文字列リテラル)はTS版でも
        // ヒント対象外(typeEqualsがstring本体とは一致しない)——実際にTS版で確認済みの
        // 挙動のため、素のstring型と分かる関数引数を使う
        let err = gen_js("fn greet(name: string) string {\n  return \"hi \" + name + 5\n}\nfn main() {\n  print(greet(\"bob\"))\n}").unwrap_err();
        assert!(err.contains("hint: use str() to convert values to string"), "got: {err}");
    }

    #[test]
    fn 未絞り込みのunion型への単項マイナスはerrになる() {
        // git historyレビュー指摘・実行確認済み: unary `-` はcheck_arith_opと同じ
        // invalid-operation診断を共有するのに以前は検査が無く、`-x`(xがint | none)が
        // 静かに素通りしていた
        let err = gen_js("fn main() {\n  m := map<string, int>{\"a\": 1}\n  x := m[\"a\"]\n  y := -x\n  print(y)\n}").unwrap_err();
        assert!(err.contains("unary '-' requires int or float"), "got: {err}");
    }

    #[test]
    fn bool配列の要素インクリメントはerrになる() {
        // 以前はJSの暗黙のbool→number変換で意味不明な値を静かに書き込んでいた
        let err = gen_js("fn main() {\n  bools := [true, false]\n  bools[0]++\n  print(bools)\n}").unwrap_err();
        assert!(err.contains("'++' requires int or float"), "got: {err}");
    }

    #[test]
    fn 未絞り込みのunion型へのincdecはerrになる() {
        let err = gen_js(
            "fn main() {\n  ch := chan<int>(1)\n  ch <- 7\n  x := <-ch\n  x++\n  print(x)\n}",
        )
        .unwrap_err();
        assert!(err.contains("'++' requires int or float"), "got: {err}");
    }

    #[test]
    fn 絞り込んだ後の単項マイナスとincdecは今まで通り動く() {
        let js = gen_body(
            "fn main() {\n  m := map<string, int>{\"a\": 1}\n  x := m[\"a\"] or 0\n  y := -x\n  mut z := 1\n  z++\n  print(y)\n  print(z)\n}",
        );
        assert!(js.contains("(-x)"), "got: {js}");
        assert!(js.contains("z++;"), "got: {js}");
    }

    // ---- milestone 14: 比較/論理/等価演算子の妥当性検査 ----

    #[test]
    fn 非bool_operandでの論理演算子はerrになる() {
        let err = gen_js("fn main() {\n  x := 1\n  if x && true {\n    print(\"yes\")\n  }\n}").unwrap_err();
        assert!(err.contains("'&&' requires bool operands, got int"), "got: {err}");
    }

    #[test]
    fn リテラルnoneとの等価比較はerrになる() {
        // P1: `x == none`は`is none`に一本化する言語ルールそのもの
        let err = gen_js("fn main() {\n  x := 1\n  if x == none {\n    print(\"yes\")\n  }\n}").unwrap_err();
        assert!(err.contains("use 'is none'"), "got: {err}");
        let err2 = gen_js("fn main() {\n  x := 1\n  if none != x {\n    print(\"yes\")\n  }\n}").unwrap_err();
        assert!(err2.contains("use 'is none'"), "got: {err2}");
    }

    #[test]
    fn 比較不能な型同士の等価比較や順序比較はerrになる() {
        let err = gen_js(
            "struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  if u == 5 {\n    print(\"yes\")\n  }\n}",
        )
        .unwrap_err();
        assert!(err.contains("cannot compare User with int"), "got: {err}");

        let err2 = gen_js(
            "struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  v := User{name: \"b\"}\n  if u < v {\n    print(\"yes\")\n  }\n}",
        )
        .unwrap_err();
        assert!(err2.contains("cannot compare User with User"), "got: {err2}");
    }

    #[test]
    fn 正常な論理_等価_順序比較は今まで通りコンパイルできる() {
        let js = gen_body(
            "fn main() {\n  x := 3\n  ok := x > 1 && x < 10\n  same := x == 3\n  print(ok)\n  print(same)\n}",
        );
        assert!(js.contains("((x > 1) && (x < 10))"), "got: {js}");
        assert!(js.contains("(x === 3)"), "got: {js}");
    }

    #[test]
    fn 非bool_operandでの単項notはerrになる() {
        // code review発覚・実行確認済みの回帰: `&&`/`||`のnot-bool検査実装時に
        // 同じ診断を共有する兄弟演算子`!`を見落としていた(PR #28のunary`-`/`++`/`--`と
        // 全く同じ構図)
        let err = gen_js("fn main() {\n  x := 5\n  if !x {\n    print(\"yes\")\n  }\n}").unwrap_err();
        assert!(err.contains("'!' requires bool, got int"), "got: {err}");
    }

    #[test]
    fn 論理演算子の左辺のis式によるnarrowingは右辺のコード生成にも反映される() {
        // code review発覚・実行確認済みの回帰: `x is int && x > 0`のような複合条件は、
        // 右辺の型検査だけでなくcodegenが右辺を実際に生成する際にも左辺のnarrowingを
        // 反映しないと、右辺の中でさらに算術/比較する式が誤ってincomparable-types等に
        // なる(gen_ifの単純な`if x is T {...}`と同じnarrowing技法を&&/||でも使う)
        let js = gen_body(
            "fn f() int | error {\n  return 1\n}\nfn main() {\n  x := f()\n  if x is int && x > 0 {\n    print(\"positive\")\n  }\n}",
        );
        assert!(js.contains("if ((Number.isInteger(x) && (x > 0)))"), "got: {js}");

        // De Morgan: ||はelse側(絞り込みの否定)の事実を右辺の検査/生成に使う
        let js2 = gen_body(
            "fn main() {\n  m := map<string, int>{\"a\": 1}\n  v := m[\"a\"]\n  if v is none || v > 0 {\n    print(\"ok\")\n  }\n}",
        );
        assert!(js2.contains("(v > 0)"), "got: {js2}");
    }

    // ---- milestone 15: 読み/書き共通のstructフィールドアクセス検証 ----

    #[test]
    fn 代入先のtypoしたフィールド名は明確なerrになる() {
        // PR #17以来の既知の限界(代入先のフィールド名は宣言済みfieldsと一切照合されず、
        // typoがJSの新規プロパティとして黙って書き込まれ、実際のフィールドは変わらない
        // ままだった)を解消
        let err = gen_js(
            "struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  u.nmae = \"b\"\n  print(u.name)\n}",
        )
        .unwrap_err();
        assert!(err.contains("User has no field 'nmae' (fields: name)"), "got: {err}");
    }

    #[test]
    fn 読み取り側のtypoしたフィールド名も同じunknown_fieldメッセージになる() {
        // 以前は「'nmae' is a method — call it, it cannot be referenced as a value」という
        // 誤ったメッセージだった(method_tableを実際には確認せず、未検出のフィールドを
        // 常にメソッド名の値参照だと決め打っていたため)
        let err = gen_js("struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  print(u.nmae)\n}").unwrap_err();
        assert!(err.contains("User has no field 'nmae' (fields: name)"), "got: {err}");
    }

    #[test]
    fn メソッド名を呼び出さず値として参照するとmethod_not_calledになる() {
        let err = gen_js(
            "struct User {\n  name: string\n}\nfn (u: User) describe() string {\n  return u.name\n}\nfn main() {\n  u := User{name: \"a\"}\n  f := u.describe\n  print(f)\n}",
        )
        .unwrap_err();
        assert!(err.contains("'describe' is a method — call it like describe(...)"), "got: {err}");
    }

    #[test]
    fn 正常なフィールドの読み書きは今まで通りコンパイルできる() {
        let js = gen_body(
            "struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  u.name = \"b\"\n  print(u.name)\n}",
        );
        assert!(js.contains("u.name = \"b\";"), "got: {js}");
        assert!(js.contains("__print(u.name);"), "got: {js}");
    }

    #[test]
    fn json_valueの不透明な再帰位置への書き込みは今まで通りコンパイルできる() {
        // git historyレビュー発覚・実行確認済みの回帰: json.Valueの自己参照する再帰位置
        // (`obj.entries`の値等、milestone 9で意図的に空フィールドの不透明な殻として
        // 表現している)への書き込みが、この milestone の新しい検証で誤って
        // unknown-fieldになっていた——2階層以上の入れ子destructureはchecker側の型推論の
        // 精度が落ちるだけでrun時テストは動く、というmilestone 9の意図的なスコープ縮小の
        // 範囲内なので、書き込みも今まで通り通す
        let js = gen_body(
            "import \"mesh/json\"\nfn main() {\n  v := json.parse(\"1\") or _ => json.Value{kind: \"null\"}\n  if v is { kind: \"obj\" } {\n    for k, val := range v.entries {\n      if val is { kind: \"str\" } {\n        val.s = \"patched\"\n        print(val.s)\n      }\n    }\n  }\n}",
        );
        assert!(js.contains("val.s = \"patched\""), "got: {js}");
    }
}
