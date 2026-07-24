// フルchecker(milestone 22・第一歩): 診断(位置+コード+メッセージ)を積むchecker。
// 既存のchecker.rs(最小リゾルバ)とは別物——あちらは診断を出さずcodegenの型解決だけを
// 担う「フェーズ2」としてそのまま残し、このフルcheckerはcodegenの前段に置く新設の
// 「フェーズ1」という位置づけ(docs/handoff.md「次のフェーズ: フルchecker移植」節で
// 合意した設計)。TS版`src/checker/`のcontext.ts(スコープ/宣言の基盤)+
// expressions.ts(識別子解決)+statements.ts(変数宣言/代入検査)のごく一部に相当する。
//
// **スコープ(意図的な段階的拡張)**: 元のcodegen移植のmilestone 1と同じ「スカラーの
// Mesh」(struct/map/配列/channel/並行処理/import/ジェネリクスは対象外)。診断コードも
// 「変数宣言・型不一致・未定義名」のごく少数(diagnostic_codes.rs参照)だけを実装する。
// milestone 23でトップレベル宣言(fn/const)自体の名前衝突検査を追加(下記
// `check_program`参照)。main関数の形検査(missing-main等)・演算子の妥当性検査
// (invalid-operation等)は引き続き対象外——アーキテクチャが正しいと分かった時点で、
// 機能ごとに広げていく方針(既存21マイルストーンと同じ進め方)。

use crate::ast::{Block, ConstDecl, ElseClause, Expr, FnDecl, IfStmt, InterpSegment, Program, Stmt, TypeNode};
use crate::diagnostic_codes::{Diagnostic, DiagnosticCode};
use crate::token::Pos;
use crate::types::{self, ANY, BOOL, ERROR, FLOAT, INT, NONE, STRING, VOID, Type};
use std::collections::HashMap;

// JS化したときに意味を持ってしまう名前(TS版`checker/context.ts`のRESERVEDをそのまま移植)
const RESERVED: &[&str] = &[
    "await", "async", "function", "const", "let", "var", "class", "new", "this", "typeof", "instanceof", "in", "of", "yield", "delete", "void", "switch", "case", "default", "do", "while",
    "with", "export", "import", "extends", "super", "null", "undefined", "try", "catch", "finally", "throw", "eval", "arguments",
];

struct Binding {
    ty: Type,
    mutable: bool,
}

pub struct FullCheckerCtx {
    // scopes[0]がトップレベル(fn/const名の置き場)、それ以降が通常の関数ローカルスコープ。
    // TS版`checker/modules.ts`のcheckPackageもトップレベルのfn/constを普通の
    // declareBindingでscopes[0]へ登録しており(milestone 22時点では別集合
    // `top_level_names`で代用していたが、milestone 23でこの統一設計へ寄せた)、
    // ローカル変数と全く同じdeclare()を通ることで予約語・組み込み名衝突・重複宣言・
    // shadowingの検査が自動的に効くようになる
    scopes: Vec<HashMap<String, Binding>>,
    // 今検査中の関数の戻り値型のスタック(無名関数はmilestone 22の対象外なので実際には
    // 深さ1までしか積まれないが、TS版`ctx.retStack`と同じ形にしておく)
    ret_stack: Vec<Type>,
    diagnostics: Vec<Diagnostic>,
}

impl FullCheckerCtx {
    fn new() -> Self {
        FullCheckerCtx { scopes: vec![HashMap::new()], ret_stack: Vec::new(), diagnostics: Vec::new() }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn error(&mut self, pos: Pos, code: DiagnosticCode, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic { pos, code, message: message.into() });
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    // TS版`checker/context.ts`の`declareBinding`相当。ブランク識別子("_")は宣言しない
    fn declare(&mut self, name: &str, ty: Type, pos: Pos, mutable: bool) {
        if name == "_" {
            return;
        }
        if RESERVED.contains(&name) {
            self.error(pos, DiagnosticCode::ReservedWord, format!("'{name}' is a reserved word and cannot be used as a name"));
            return;
        }
        if crate::checker::is_builtin(name) {
            self.error(pos, DiagnosticCode::BuiltinRedeclared, format!("'{name}' is a builtin function and cannot be redeclared"));
            return;
        }
        if self.scopes.last().expect("scopes is never empty").contains_key(name) {
            self.error(pos, DiagnosticCode::AlreadyDeclared, format!("'{name}' is already declared in this scope"));
            return;
        }
        // シャドーイング禁止(TS版と同じ2026-07-17決定): 外側スコープ(トップレベルの
        // fn/const名を含む——TS版コメント「外側スコープ(関数名を含む)」と同じ扱い。
        // トップレベル名もscopes[0]にいるのでlookup()が自然に見つける)に同名があれば、
        // 更新し忘れて`:=`してしまっただけの疑いが強いバグとして拒否する
        if self.lookup(name).is_some() {
            self.error(pos, DiagnosticCode::Shadowing, format!("'{name}' shadows an outer binding — use '=' to update it, or pick a different name"));
            return;
        }
        self.scopes.last_mut().expect("scopes is never empty").insert(name.to_string(), Binding { ty, mutable });
    }
}

// 型注釈をmilestone 22のスコープ(スカラーのみ)で解決する。struct/union/配列/map/channel/
// 関数型やpkg修飾型はこの一歩の対象外なのでANYへフォールバックする(診断はしない——
// 未対応の構文をあたかも誤りであるかのように報告しないため)
fn resolve_scalar_type(node: &TypeNode) -> Type {
    match node {
        TypeNode::Name { name, pkg: None, .. } => match name.as_str() {
            "int" => INT,
            "float" => FLOAT,
            "string" => STRING,
            "bool" => BOOL,
            "void" => VOID,
            "error" => ERROR,
            "none" => NONE,
            _ => ANY,
        },
        _ => ANY,
    }
}

// milestone 22のスコープ(スカラーのMesh)に含まれる式だけ型を推論しつつ未定義名を検査する。
// スコープ外の式(struct/array/map/channel/spawn/match/is/...)は中まで再帰せずANYへ
// フォールバックする——それらの内部にある未定義名の見落としは、対応する構文が
// milestone 22の対象に入るタイミングで解消する
fn infer_expr(ctx: &mut FullCheckerCtx, expr: &Expr) -> Type {
    match expr {
        Expr::Int { .. } => INT,
        Expr::Float { .. } => FLOAT,
        Expr::String { .. } => STRING,
        Expr::Bool { .. } => BOOL,
        Expr::None { .. } => NONE,
        Expr::Interp { segments, .. } => {
            for seg in segments {
                if let InterpSegment::Expr { expr } = seg {
                    infer_expr(ctx, expr);
                }
            }
            STRING
        }
        Expr::Ident { name, pos } => {
            if let Some(b) = ctx.lookup(name) {
                // トップレベルのfn/const名もscopes[0]にBindingとして入っている
                // (check_programが本体検査より前に登録する)ので、ここは組み込み関数と
                // 通常のローカル変数/トップレベル名を区別せず同じ経路で解決できる
                b.ty.clone()
            } else if crate::checker::is_builtin(name) {
                ANY
            } else {
                ctx.error(*pos, DiagnosticCode::UndefinedName, format!("'{name}' is not defined"));
                ANY
            }
        }
        Expr::Binary { op, left, right, .. } => {
            let left_ty = infer_expr(ctx, left);
            infer_expr(ctx, right);
            // 比較・論理演算子は被演算子の型によらず結果は常にbool(妥当性検査自体は
            // milestone 22の対象外だが、結果の型は演算子だけで決まるので取り違えると
            // `x: bool = a > b`のような正しいコードがtype-mismatchの誤検知になる)。
            // それ以外(算術演算子)は左辺の型をそのまま伝播する——int/floatの昇格規則等の
            // 妥当性検査はmilestone 22の対象外
            use crate::token::TokenType::*;
            match op {
                EqEq | NotEq | Lt | Gt | Le | Ge | AndAnd | OrOr => BOOL,
                _ => left_ty,
            }
        }
        Expr::Unary { operand, .. } => infer_expr(ctx, operand),
        Expr::Call { callee, args, .. } => {
            infer_expr(ctx, callee);
            for a in args {
                infer_expr(ctx, a);
            }
            // 引数の数・型の照合(argument-count等)はmilestone 22の対象外
            ANY
        }
        // struct/array/map/channel/match/is/spawn/select/prop/orElse/無名関数など:
        // milestone 22の対象外なので中へは踏み込まない
        _ => ANY,
    }
}

fn check_block(ctx: &mut FullCheckerCtx, block: &Block) {
    ctx.push_scope();
    for stmt in &block.stmts {
        check_stmt(ctx, stmt);
    }
    ctx.pop_scope();
}

fn check_stmt(ctx: &mut FullCheckerCtx, stmt: &Stmt) {
    match stmt {
        // names/valuesは1:1対応が前提(Go風多重代入はF-9で廃止済み。パーサが数を揃える)
        Stmt::ShortVarDecl { names, values, mutable, pos } => {
            for (name, value) in names.iter().zip(values.iter()) {
                let ty = infer_expr(ctx, value);
                ctx.declare(name, ty, *pos, *mutable);
            }
        }
        Stmt::TypedVarDecl { name, type_node, value, mutable, pos } => {
            let declared = resolve_scalar_type(type_node);
            let value_ty = infer_expr(ctx, value);
            if !types::assignable(&value_ty, &declared) {
                ctx.error(
                    *pos,
                    DiagnosticCode::TypeMismatch,
                    format!("cannot assign a value of type '{}' to '{name}' of type '{}'", types::type_to_string(&value_ty), types::type_to_string(&declared)),
                );
            }
            ctx.declare(name, declared, *pos, *mutable);
        }
        Stmt::Assign { targets, values, .. } => {
            for (target, value) in targets.iter().zip(values.iter()) {
                let value_ty = infer_expr(ctx, value);
                check_assign_target(ctx, target, &value_ty);
            }
        }
        Stmt::IncDec { target, pos, .. } => {
            if let Expr::Ident { name, .. } = target {
                match ctx.lookup(name) {
                    None => {
                        infer_expr(ctx, target);
                    }
                    Some(b) if !b.mutable => {
                        ctx.error(*pos, DiagnosticCode::ImmutableAssignment, format!("'{name}' was declared without 'mut' and cannot be reassigned"));
                    }
                    _ => {}
                }
            } else {
                infer_expr(ctx, target); // index/member代入はmilestone 22対象外——式だけ検査
            }
        }
        Stmt::ExprStmt { expr, .. } => {
            infer_expr(ctx, expr);
        }
        Stmt::Return { value, pos } => {
            // 戻り値なしのreturn(値が要る関数で足りない場合はmissing-return-value)は
            // この一歩で実装した7種に含めていないので診断しない——値がある場合の
            // type-mismatchだけ、7種の同じ診断コードとしてここでも検査する
            if let Some(v) = value {
                let value_ty = infer_expr(ctx, v);
                let expected = ctx.ret_stack.last().cloned().unwrap_or(VOID);
                if !types::assignable(&value_ty, &expected) {
                    ctx.error(
                        *pos,
                        DiagnosticCode::TypeMismatch,
                        format!("cannot return a value of type '{}' as '{}'", types::type_to_string(&value_ty), types::type_to_string(&expected)),
                    );
                }
            }
        }
        Stmt::If(if_stmt) => check_if(ctx, if_stmt),
        Stmt::For { init, cond, post, body, .. } => {
            // ヘッダのinit変数(例: `for i := 0; ...`)用の外側スコープを1つ作り、
            // bodyはcheck_block経由でその内側にネストした別スコープを持つ(TS版
            // `checker/statements.ts`の`for`ケースと同じ2段構成——pushScope(ヘッダ)の
            // 後、bodyはcheckBlockが自分でpushScopeする)。ここを1段のスコープに
            // 潰すと、body内で`i := ...`のようにヘッダ変数と同名の宣言をしたとき
            // 「同じスコープ」と誤認してalready-declaredになってしまう
            // (正しくは外側スコープの再利用としてshadowing——実装時に見落とし、
            // git履歴・TS版との突き合わせで発覚した)
            ctx.push_scope();
            if let Some(init) = init {
                check_for_init(ctx, init);
            }
            if let Some(cond) = cond {
                infer_expr(ctx, cond);
            }
            if let Some(post) = post {
                check_stmt(ctx, post);
            }
            check_block(ctx, body);
            ctx.pop_scope();
        }
        Stmt::RangeFor { names, subject, body, pos } => {
            // for文と同じ理由でヘッダ(range変数)とbodyは別スコープにする
            infer_expr(ctx, subject);
            ctx.push_scope();
            for n in names {
                ctx.declare(n, ANY, *pos, false);
            }
            check_block(ctx, body);
            ctx.pop_scope();
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Wait { body, .. } => check_block(ctx, body),
        Stmt::Send { channel, value, .. } => {
            infer_expr(ctx, channel);
            infer_expr(ctx, value);
        }
        Stmt::DeferStmt { call, .. } => {
            infer_expr(ctx, call);
        }
    }
}

fn check_assign_target(ctx: &mut FullCheckerCtx, target: &Expr, value_ty: &Type) {
    let Expr::Ident { name, pos } = target else {
        infer_expr(ctx, target); // index/member代入はmilestone 22対象外——式だけ検査
        return;
    };
    match ctx.lookup(name) {
        None => {
            infer_expr(ctx, target); // 共通経路でundefined-nameを報告
        }
        Some(b) if !b.mutable => {
            ctx.error(*pos, DiagnosticCode::ImmutableAssignment, format!("'{name}' was declared without 'mut' and cannot be reassigned"));
        }
        Some(b) if !types::assignable(value_ty, &b.ty) => {
            let declared_ty_str = types::type_to_string(&b.ty);
            ctx.error(*pos, DiagnosticCode::TypeMismatch, format!("cannot assign a value of type '{}' to '{name}' of type '{declared_ty_str}'", types::type_to_string(value_ty)));
        }
        _ => {}
    }
}

// C風forのヘッダ変数は暗黙に可変(TS版`checker/statements.ts`の`for`ケース、B-4決定:
// デフォルト不変の唯一の構造的例外)。`i++`という後置文が書けるようにするための特例で、
// ここだけAST上の`mutable`フラグ(通常`mut`を書かない限りfalse)を無視してtrue扱いにする
fn check_for_init(ctx: &mut FullCheckerCtx, init: &Stmt) {
    if let Stmt::ShortVarDecl { names, values, pos, .. } = init {
        for (name, value) in names.iter().zip(values.iter()) {
            let ty = infer_expr(ctx, value);
            ctx.declare(name, ty, *pos, true);
        }
    } else {
        check_stmt(ctx, init);
    }
}

fn check_if(ctx: &mut FullCheckerCtx, if_stmt: &IfStmt) {
    infer_expr(ctx, &if_stmt.cond);
    check_block(ctx, &if_stmt.then);
    match if_stmt.else_.as_deref() {
        Some(ElseClause::If(nested)) => check_if(ctx, nested),
        Some(ElseClause::Block(b)) => check_block(ctx, b),
        None => {}
    }
}

fn check_fn(ctx: &mut FullCheckerCtx, f: &FnDecl) {
    // パラメータ用の外側スコープ+本体用のネストしたスコープ(check_block)という
    // 2段構成(TS版`checker/functions.ts`と同じ——pushScope〈パラメータ〉の後、
    // 本体はcheckBlockが自分でpushScopeする)。1段に潰すと、本体でパラメータ名を
    // 再宣言したとき「同じスコープ」と誤認してalready-declaredになってしまう
    // (正しくは外側スコープの再利用としてshadowing)
    ctx.push_scope();
    for p in &f.params {
        ctx.declare(&p.name, resolve_scalar_type(&p.type_node), p.pos, false);
    }
    ctx.ret_stack.push(f.ret.as_ref().map(resolve_scalar_type).unwrap_or(VOID));
    check_block(ctx, &f.body);
    ctx.ret_stack.pop();
    ctx.pop_scope();
}

// トップレベル定数(F-9c)。値の式はローカル変数宣言と同じくtype-mismatch/undefined-name
// の対象(見落とすと定数の初期値だけすり抜けてしまう)。最後にTS版`checker/modules.ts`と
// 同じ「型注釈があればそちら、無ければ値から推論」の優先順位でscopes[0]へdeclareする——
// これによりmilestone 23で追加した名前衝突検査(reserved-word/builtin-redeclared/
// already-declared/shadowing)がローカル変数と同じdeclare()経由で自動的に効く
fn check_top_level_const(ctx: &mut FullCheckerCtx, c: &ConstDecl) {
    let value_ty = infer_expr(ctx, &c.value);
    let final_ty = match &c.type_node {
        Some(type_node) => {
            let declared = resolve_scalar_type(type_node);
            if !types::assignable(&value_ty, &declared) {
                ctx.error(
                    c.pos,
                    DiagnosticCode::TypeMismatch,
                    format!("cannot assign a value of type '{}' to '{}' of type '{}'", types::type_to_string(&value_ty), c.name, types::type_to_string(&declared)),
                );
            }
            declared
        }
        None => value_ty,
    };
    ctx.declare(&c.name, final_ty, c.pos, false);
}

// このProgram(単一ファイル。milestone 22はimport/パッケージ対象外)を検査し、
// 見つかった診断を返す。空なら「(このスコープの範囲で)問題なし」の意味
pub fn check_program(program: &Program) -> Vec<Diagnostic> {
    let mut ctx = FullCheckerCtx::new();
    // TS版`checker/modules.ts`のcheckPackageと同じ順序: 先に全関数の名前をscopes[0]へ
    // 登録してから(前方参照・相互再帰を許すため——本体はまだ検査しない)、トップレベル
    // 定数を検査+登録し、最後に関数本体を検査する。シグネチャ全体(引数/戻り値の型)の
    // 照合はmilestone 22と同じくこの一歩の対象外なので、関数の型はANYで登録する
    // (「名前として存在する」ことだけをここで表現する)
    for f in &program.fns {
        ctx.declare(&f.name, ANY, f.pos, false);
    }
    for c in &program.consts {
        check_top_level_const(&mut ctx, c);
    }
    for f in &program.fns {
        check_fn(&mut ctx, f);
    }
    ctx.diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn check(src: &str) -> Vec<Diagnostic> {
        let program = parse(src).expect("test source must parse");
        check_program(&program)
    }

    #[test]
    fn 正しいスカラープログラムは診断を出さない() {
        let diags = check(
            "fn helper() int { return 42 }\n\
             fn main() {\n\
                 mut total := helper()\n\
                 total = total + 1\n\
                 if total > 0 {\n\
                     x := \"ok\"\n\
                     print(x)\n\
                 }\n\
             }\n",
        );
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 未定義の変数参照はundefined_nameを報告する() {
        let diags = check("fn main() {\n    print(missing)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn mutなしの変数への再代入はimmutable_assignmentを報告する() {
        let diags = check("fn main() {\n    total := 1\n    total = 2\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ImmutableAssignment);
    }

    #[test]
    fn 同じスコープでの再宣言はalready_declaredを報告する() {
        let diags = check("fn main() {\n    x := 1\n    x := 2\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::AlreadyDeclared);
    }

    #[test]
    fn 外側スコープの名前を再利用するとshadowingを報告する() {
        let diags = check("fn main() {\n    x := 1\n    if true {\n        x := 2\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn 型注釈と値の不一致はtype_mismatchを報告する() {
        let diags = check("fn main() {\n    x: int = \"hi\"\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn 再代入時の型不一致もtype_mismatchを報告する() {
        let diags = check("fn main() {\n    mut x := 1\n    x = \"hi\"\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn 予約語での宣言はreserved_wordを報告する() {
        let diags = check("fn main() {\n    await := 1\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ReservedWord);
    }

    #[test]
    fn 組み込み関数名での宣言はbuiltin_redeclaredを報告する() {
        let diags = check("fn main() {\n    print := 1\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinRedeclared);
    }

    #[test]
    fn intはfloatへ暗黙に広げられ型不一致にならない() {
        let diags = check("fn main() {\n    x: float = 1\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn トップレベル関数名を再宣言するとshadowingになる() {
        let diags = check("fn helper() int { return 1 }\nfn main() {\n    helper := 5\n    print(helper)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn 重複したトップレベル関数宣言はalready_declaredになる() {
        let diags = check("fn helper() int { return 1 }\nfn helper() int { return 2 }\nfn main() {\n    print(helper())\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::AlreadyDeclared);
    }

    #[test]
    fn 組み込み関数名のトップレベル関数宣言はbuiltin_redeclaredになる() {
        let diags = check("fn print() {\n}\nfn main() {\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinRedeclared);
    }

    #[test]
    fn 予約語のトップレベル関数宣言はreserved_wordになる() {
        let diags = check("fn await() {\n}\nfn main() {\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ReservedWord);
    }

    #[test]
    fn トップレベル定数とトップレベル関数の名前衝突もalready_declaredになる() {
        let diags = check("helper := 1\nfn helper() int { return 2 }\nfn main() {\n    print(helper)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::AlreadyDeclared);
    }

    #[test]
    fn 戻り値の型不一致はtype_mismatchを報告する() {
        let diags = check("fn helper() int {\n    return \"oops\"\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn トップレベル定数の型不一致もtype_mismatchを報告する() {
        let diags = check("x: int = \"oops\"\nfn main() {\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn トップレベル定数内の未定義名もundefined_nameを報告する() {
        let diags = check("x := missing\nfn main() {\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn 比較演算子の結果はboolになり型不一致を誤検知しない() {
        // 回帰テスト: Binaryの結果型を「左辺の型をそのまま伝播」で済ませると、
        // `a > b`(int同士の比較)がintのまま返ってしまい、boolへの代入が
        // type-mismatchの誤検知になっていた
        let diags = check("fn main() {\n    ok: bool = 1 > 0\n    print(ok)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 論理演算子の結果もboolになり型不一致を誤検知しない() {
        let diags = check("fn main() {\n    mut ok := true\n    ok = 1 > 0 && 2 > 1\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn トップレベル関数呼び出しはundefined_nameを誤検知しない() {
        // 回帰テスト: トップレベル関数名をscopeに登録し忘れると、他の関数を呼ぶだけの
        // 正しいコードがundefined-nameになってしまう
        let diags = check("fn helper() int { return 1 }\nfn main() {\n    x := helper()\n    print(x)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 関数本体でパラメータ名を再宣言するとshadowingになる() {
        // 回帰テスト: パラメータのスコープと本体のスコープを1段に潰すと、本体側の
        // 再宣言が(shadowingではなく)already-declaredに誤判定されていた
        let diags = check("fn f(x: int) {\n    x := 5\n    print(x)\n}\nfn main() {\n    f(1)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn forボディでヘッダのinit変数を再宣言するとshadowingになる() {
        let diags = check("fn main() {\n    for i := 0; i < 3; i++ {\n        i := 5\n        print(i)\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn range_forボディでヘッダの変数を再宣言するとshadowingになる() {
        let diags = check("fn main() {\n    for i := range 3 {\n        i := 5\n        print(i)\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn forヘッダのinit変数はbody内から参照できる() {
        let diags = check("fn main() {\n    for i := 0; i < 3; i++ {\n        print(i)\n    }\n}\n");
        assert_eq!(diags, vec![]);
    }
}
