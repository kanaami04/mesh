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

use crate::ast::{Block, ConstDecl, ElseClause, Expr, FnDecl, IfStmt, Program, Receiver, Stmt, TypeNode};
use crate::checker::{self, CheckerCtx};
use crate::token::{Pos, TokenType};
use crate::types::Type;

// src/runtime.ts自体はTSファイル(`export const PRELUDE = \`...\`;`というテンプレートリテラル
// 宣言でランタイムJSを包んでいる)なので、include_str!でファイル全体を埋め込むと
// TSの宣言部分(`export const PRELUDE = `や末尾の`;`)まで生成JSに混ざって壊れる。
// バッククォートの中身だけを取り出す(ファイル全体でバッククォートはこの2箇所にしか
// 現れない — ランタイム本体は文字列連結のみで書かれ、テンプレートリテラルを使っていない)
const RUNTIME_TS: &str = include_str!("../../src/runtime.ts");

fn prelude() -> &'static str {
    let start = RUNTIME_TS.find('`').expect("runtime.ts should wrap PRELUDE in a template literal") + 1;
    let end = RUNTIME_TS.rfind('`').expect("runtime.ts should wrap PRELUDE in a template literal");
    &RUNTIME_TS[start..end]
}

pub type CodegenResult<T> = Result<T, String>;

pub fn generate(program: &Program, file: &str) -> CodegenResult<String> {
    Codegen::new(file).generate_all(program)
}

struct Codegen {
    out: Vec<String>,
    indent: usize,
    file: String,
    ctx: CheckerCtx,
}

impl Codegen {
    fn new(file: &str) -> Self {
        Codegen { out: Vec::new(), indent: 0, file: file.to_string(), ctx: CheckerCtx::new() }
    }

    fn emit(&mut self, line: impl Into<String>) {
        self.out.push("  ".repeat(self.indent) + &line.into());
    }

    // パニックメッセージに埋め込む位置情報: "main.mesh:3:8"
    fn at(&self, pos: Pos) -> String {
        format!("{:?}", format!("{}:{}:{}", self.file, pos.line, pos.col))
    }

    fn generate_all(&mut self, program: &Program) -> CodegenResult<String> {
        if !program.imports.is_empty() {
            return Err("codegen: import/export are not yet supported (milestone 1 is single-file only)".to_string());
        }
        // struct宣言のみ対応。error/jsonマーカー付き・判別可能union等の非struct型宣言は
        // まだ対象外(次のmilestone以降)
        for t in &program.types {
            if t.is_error || t.is_json {
                return Err(format!("codegen: error/json struct declarations are not yet supported (type '{}' at {}:{})", t.name, t.pos.line, t.pos.col));
            }
            if !matches!(t.node, TypeNode::StructType { .. }) {
                return Err(format!("codegen: only plain struct declarations are supported so far (type '{}' at {}:{})", t.name, t.pos.line, t.pos.col));
            }
        }
        checker::resolve_struct_decls(&mut self.ctx, &program.types)?;

        self.out.push(prelude().trim_end().to_string());
        self.out.push(String::new());

        // 呼び出し先の戻り値型を解決できるよう、先に全トップレベル関数・メソッドのシグネチャを
        // 登録してから本体を出力する(前方参照——後で宣言される関数/メソッドを先に呼ぶ場合に
        // 対応するため)
        for fn_decl in &program.fns {
            if !fn_decl.type_params.is_empty() {
                return Err(format!("codegen: generic functions are not yet supported (fn '{}' at {}:{})", fn_decl.name, fn_decl.pos.line, fn_decl.pos.col));
            }
            let params = fn_decl.params.iter().map(|p| checker::resolve_type_node(&self.ctx, &p.type_node)).collect();
            let ret = Box::new(checker::resolve_return_type(&self.ctx, &fn_decl.ret));
            match &fn_decl.receiver {
                Some(recv) => {
                    let struct_name = receiver_struct_name(recv)?;
                    // レシーバが未宣言/非struct型(例: `fn (x: int) foo()`)なら殻へ静かに
                    // フォールバックさせず、明確なErrにする(おかしなJS関数名`__m_int_foo`等を
                    // 生成しないため)
                    if self.ctx.lookup_struct(&struct_name).is_none() {
                        return Err(format!(
                            "codegen: receiver type '{struct_name}' is not a declared struct (fn '{}' at {}:{})",
                            fn_decl.name, fn_decl.pos.line, fn_decl.pos.col
                        ));
                    }
                    let mut all_params = vec![checker::resolve_type_node(&self.ctx, &recv.type_node)];
                    all_params.extend(params);
                    self.ctx.declare_method(&struct_name, &fn_decl.name, Type::Fn { params: all_params, ret });
                }
                None => self.ctx.declare_fn(&fn_decl.name, Type::Fn { params, ret }),
            }
        }

        for c in &program.consts {
            self.gen_const_decl(c)?;
        }
        for fn_decl in &program.fns {
            self.gen_fn_decl(fn_decl)?;
            self.out.push(String::new());
        }

        self.out.push("main().catch(__panic);".to_string());
        Ok(self.out.join("\n") + "\n")
    }

    fn gen_const_decl(&mut self, c: &ConstDecl) -> CodegenResult<()> {
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
            Some(recv) => method_js_name(&receiver_struct_name(recv)?, &fn_decl.name),
            None => fn_decl.name.clone(),
        };
        self.emit(format!("async function {js_name}({params}) {{"));
        self.ctx.push_scope();
        if let Some(recv) = &fn_decl.receiver {
            self.ctx.declare(&recv.name, checker::resolve_type_node(&self.ctx, &recv.type_node));
        }
        for p in &fn_decl.params {
            self.ctx.declare(&p.name, checker::resolve_type_node(&self.ctx, &p.type_node));
        }
        self.indent += 1;
        for stmt in &fn_decl.body.stmts {
            self.gen_stmt(stmt)?;
        }
        self.indent -= 1;
        self.ctx.pop_scope();
        self.emit("}");
        Ok(())
    }

    fn gen_block(&mut self, block: &Block) -> CodegenResult<()> {
        self.ctx.push_scope();
        self.indent += 1;
        for stmt in &block.stmts {
            self.gen_stmt(stmt)?;
        }
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
            Stmt::IncDec { target, op, .. } => {
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
            Stmt::Wait { pos, .. } => Err(format!("codegen: 'wait' is not yet supported ({}:{})", pos.line, pos.col)),
            Stmt::Send { pos, .. } => Err(format!("codegen: channel send is not yet supported ({}:{})", pos.line, pos.col)),
            Stmt::RangeFor { pos, .. } => Err(format!("codegen: range-for is not yet supported ({}:{})", pos.line, pos.col)),
            Stmt::DeferStmt { pos, .. } => Err(format!("codegen: 'defer' is not yet supported ({}:{})", pos.line, pos.col)),
        }
    }

    fn gen_if(&mut self, if_stmt: &IfStmt) -> CodegenResult<()> {
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
            Stmt::IncDec { target, op, .. } => {
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
        let info = checker::infer_binary(&self.ctx, op, target, value_expr);
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
                if !matches!(checker::infer_expr(&self.ctx, target), Type::Struct { .. }) {
                    return Err(format!("codegen: package/member access is not yet supported ({}:{})", pos.line, pos.col));
                }
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
            Expr::Binary { op, left, right, pos } => {
                let info = checker::infer_binary(&self.ctx, *op, left, right);
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
            Expr::Unary { op, operand, .. } => Ok(format!("({op}{})", self.gen_expr(operand)?)),
            Expr::Call { .. } => self.gen_call(expr),
            Expr::Is { pos, .. } => Err(format!("codegen: 'is' is not yet supported ({}:{})", pos.line, pos.col)),
            Expr::Match { pos, .. } => Err(format!("codegen: 'match' is not yet supported ({}:{})", pos.line, pos.col)),
            // 裸のメンバーアクセス(呼び出しではない)。targetがstruct型かつnameが宣言済み
            // フィールドのときだけ素の`.name`を出す——パッケージ修飾(`math.add`)はまだ
            // 実装が無く「未解決の識別子」としてANYへ落ちるので、ここで弾かないと
            // 実行時ReferenceErrorになるJSを静かに生成してしまう。メソッド名(フィールドでは
            // ない名前)を値として参照する式もTS版と同じく対象外のまま(呼び出し式側でだけ解決)
            Expr::Member { target, name, pos } => {
                let target_ty = checker::infer_expr(&self.ctx, target);
                let Type::Struct { fields, .. } = &target_ty else {
                    return Err(format!("codegen: package/member access is not yet supported ({}:{})", pos.line, pos.col));
                };
                if !fields.iter().any(|f| &f.name == name) {
                    return Err(format!("codegen: '{name}' is a method — call it, it cannot be referenced as a value ({}:{})", pos.line, pos.col));
                }
                Ok(format!("{}.{name}", self.gen_expr(target)?))
            }
            // 生成JSにはstruct名自体は現れない(TS版と同じ、プレーンなobject literal)。
            // error/jsonマーカー付きstructはgenerate_allで弾いてあるので、__errTagは
            // 次のmilestone(error/json)まで出番が無い
            Expr::StructLit { pkg, fields, pos, .. } => {
                if pkg.is_some() {
                    return Err(format!("codegen: package-qualified struct literals are not yet supported ({}:{})", pos.line, pos.col));
                }
                let mut js_fields = Vec::with_capacity(fields.len());
                for f in fields {
                    if f.name == "__proto__" {
                        return Err(format!("codegen: '__proto__' cannot be used as a field name ({}:{})", f.pos.line, f.pos.col));
                    }
                    js_fields.push(format!("{}: {}", f.name, self.gen_expr(&f.value)?));
                }
                Ok(format!("({{ {} }})", js_fields.join(", ")))
            }
            Expr::Recv { pos, .. } => Err(format!("codegen: channel receive is not yet supported ({}:{})", pos.line, pos.col)),
            Expr::Chan { pos, .. } => Err(format!("codegen: channels are not yet supported ({}:{})", pos.line, pos.col)),
            Expr::Spawn { pos, .. } => Err(format!("codegen: spawn/detach are not yet supported ({}:{})", pos.line, pos.col)),
            Expr::Select { pos, .. } => Err(format!("codegen: 'select' is not yet supported ({}:{})", pos.line, pos.col)),
            Expr::Prop { pos, .. } => Err(format!("codegen: '?' propagation is not yet supported ({}:{})", pos.line, pos.col)),
            Expr::OrElse { pos, .. } => Err(format!("codegen: 'or' is not yet supported ({}:{})", pos.line, pos.col)),
            Expr::ArrayLit { pos, .. } => Err(format!("codegen: array literals are not yet supported ({}:{})", pos.line, pos.col)),
            Expr::Index { pos, .. } => Err(format!("codegen: index access is not yet supported ({}:{})", pos.line, pos.col)),
            Expr::MapLit { pos, .. } => Err(format!("codegen: map literals are not yet supported ({}:{})", pos.line, pos.col)),
            Expr::FnExpr { pos, .. } => Err(format!("codegen: anonymous functions are not yet supported ({}:{})", pos.line, pos.col)),
        }
    }

    fn gen_call(&mut self, expr: &Expr) -> CodegenResult<String> {
        let Expr::Call { callee, args, pos } = expr else { unreachable!("caller guarantees Expr::Call") };

        // 組み込み関数はランタイムの同期ヘルパへ直接変換(milestone 1はstruct/mapが無いので、
        // TS版のうちそれらに依存しないものだけを移植——lenの map/配列判別・pushの配列操作等は
        // 次のマイルストーン以降)
        if let Expr::Ident { name, .. } = &**callee
            && checker::is_builtin(name)
        {
            let js_args = args.iter().map(|a| self.gen_expr(a)).collect::<CodegenResult<Vec<_>>>()?;
            return self.gen_builtin_call(name, &js_args, *pos);
        }

        // メソッド呼び出し: recv.method(args) → __m_Struct_method(recv, args)。
        // TS版calls.ts/codegen.tsと同じ「フィールドが勝つ」順序——targetがstruct型で
        // nameが宣言済みフィールドでなければメソッドと判定する
        if let Expr::Member { target, name, .. } = &**callee
            && let Type::Struct { fields, name: struct_name, .. } = checker::infer_expr(&self.ctx, target)
            && !fields.iter().any(|f| &f.name == name)
        {
            if self.ctx.lookup_method(&struct_name, name).is_none() {
                // structではあるがfieldにもmethodにも無い名前——実行時に
                // `undefined is not a function`でクラッシュさせず、ここで明確なErrにする
                return Err(format!("codegen: '{struct_name}' has no method '{name}' ({}:{})", pos.line, pos.col));
            }
            let recv_js = self.gen_expr(target)?;
            let args_js = args.iter().map(|a| self.gen_expr(a)).collect::<CodegenResult<Vec<_>>>()?;
            let js_name = method_js_name(&struct_name, name);
            let all_args = std::iter::once(recv_js).chain(args_js).collect::<Vec<_>>().join(", ");
            return Ok(format!("(await {js_name}({all_args}))"));
        }

        // ユーザー定義関数はすべてasyncなので常にawait
        let callee_js = self.gen_expr(callee)?;
        let args_js = args.iter().map(|a| self.gen_expr(a)).collect::<CodegenResult<Vec<_>>>()?;
        Ok(format!("(await {callee_js}({}))", args_js.join(", ")))
    }

    fn gen_builtin_call(&self, name: &str, args: &[String], pos: Pos) -> CodegenResult<String> {
        // code review指摘: パーサ/checkerのどちらも組み込みの引数個数を検査しないため、
        // 以前は`args[0]`/`args[1]`への直接インデックスが足りない引数でパニックしていた
        // (例: `round()`)。「まだ対応していない構文はErrで返す、パニックさせない」という
        // 設計原則(ast.rsコメント参照)に反するため、個数を先に検査してから分岐する
        let required = match name {
            "print" => 0,
            "str" | "sleep" | "toInt" | "toFloat" | "round" | "floor" | "ceil" | "error" | "trim" | "upper" | "lower" | "sort" | "close" => 1,
            "contains" | "indexOf" | "get" | "split" | "join" | "push" => 2,
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
            // len/delete/keys/values/filter/map/reduceはmap/配列の判別か高階関数呼び出しが要り、
            // milestone 1(配列/map未対応)の範囲外——次のマイルストーンで対応する
            _ => Err(format!("codegen: builtin '{name}' is not yet supported ({}:{})", pos.line, pos.col)),
        }
    }
}

// レシーバの型注釈からstruct名を取り出す。パッケージ修飾(`math.User`)はモジュールの
// milestoneまで対象外、単純な名前(`(u: User)`)のみ受け付ける
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
// ように)。TS版のmethodJsNameを移植(パッケージ修飾structの"."を"$"に変換する部分は
// パッケージ未対応のmilestone 2ではまだ現れないが、そのまま移植しておく)
fn method_js_name(struct_name: &str, method_name: &str) -> String {
    format!("__m_{}_{}", struct_name.replace('.', "$"), method_name)
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
        let err = gen_js("struct User {\n  name: string\n}\nfn main() {\n  u := User{name: \"a\"}\n  u.unknown()\n}").unwrap_err();
        assert!(err.contains("has no method"), "got: {err}");
    }

    #[test]
    fn error_structはまだ未対応として明確なエラーになる() {
        let err = gen_js("error struct Oops {\n  message: string\n}\nfn main() {}").unwrap_err();
        assert!(err.contains("not yet supported"), "got: {err}");
    }

    #[test]
    fn パッケージ修飾structリテラルはまだ未対応として明確なエラーになる() {
        let err = gen_js("fn main() {\n  x := math.Point{x: 1, y: 2}\n}").unwrap_err();
        assert!(err.contains("not yet supported"), "got: {err}");
    }

    #[test]
    fn 多値の短縮変数宣言は未対応として明確なエラーになる() {
        let err = gen_js("fn f() int | error {\n  return 1\n}\nfn main() {\n  v, err := f()\n}").unwrap_err();
        assert!(err.contains("multi-value"), "got: {err}");
    }

    #[test]
    fn 配列リテラルは未対応として明確なエラーになる() {
        let err = gen_js("fn main() {\n  xs := [1, 2, 3]\n}").unwrap_err();
        assert!(err.contains("not yet supported"), "got: {err}");
    }
}
