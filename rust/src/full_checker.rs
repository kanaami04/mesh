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
// milestone 23でトップレベル宣言(fn/const)自体の名前衝突検査(下記`check_program`参照)、
// milestone 24でmain関数の形検査(missing-main/invalid-main-signature)、
// milestone 25で演算子の妥当性検査(invalid-operation/incomparable-types/not-bool/
// use-is-none/division-by-zero——下記`infer_binary`/`check_arith`/`infer_unary`参照)、
// milestone 26でユーザー定義関数呼び出しの引数個数・型検査(argument-count——下記
// `fn_signature`/`Expr::Call`参照)、milestone 27で組み込み関数の個数・スカラー引数型検査
// (argument-count/builtin-arg-type——下記`infer_builtin_call`参照)を追加。
// struct/フィールド関連の診断・配列/map/channel等コレクションの要素型検査・
// run/buildへのゲート統合は引き続き対象外
// ——アーキテクチャが正しいと分かった時点で、機能ごとに広げていく方針(既存21マイルストーンと
// 同じ進め方)。

use crate::ast::{Block, ConstDecl, ElseClause, Expr, FnDecl, IfStmt, InterpSegment, Program, Stmt, TypeNode};
use crate::diagnostic_codes::{Diagnostic, DiagnosticCode};
use crate::token::{Pos, TokenType};
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
        // 文字列リテラルは(STRINGではなく)リテラル型。TS版`checkExprSingle`・
        // codegen側checker.rsの`infer_expr`と同じ——`1 < "a"`の診断で相手の型が
        // `string`ではなく`"a"`と表示される等、TS版とのメッセージ一致に効く。
        // mut宣言時の型はShortVarDeclでwiden_literalする(TS版statements.tsと同じ)
        Expr::String { value, .. } => Type::Literal(value.clone()),
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
        Expr::Binary { op, left, right, pos } => infer_binary(ctx, *op, left, right, *pos),
        Expr::Unary { op, operand, pos } => infer_unary(ctx, *op, operand, *pos),
        Expr::Call { callee, args, pos } => {
            // milestone 27: 組み込み関数呼び出し(`print`/`len`/...)を先にintercept。
            // 名前が組み込みならユーザーはそれをshadowできない(declare()が拒否する)ため、
            // 裸のIdentが組み込み名と一致すれば必ず組み込み呼び出し
            if let Expr::Ident { name, .. } = &**callee
                && crate::checker::is_builtin(name)
            {
                return infer_builtin_call(ctx, name, args, *pos);
            }
            let callee_ty = infer_expr(ctx, callee);
            let arg_tys: Vec<Type> = args.iter().map(|a| infer_expr(ctx, a)).collect();
            // milestone 26: calleeがユーザー定義関数(Type::Fn。check_programが
            // 非ジェネリック自由関数をシグネチャ付きで登録)なら個数・各引数の型を照合する
            // (TS版`checkArgsAgainst`)。ローカル変数に束縛した関数値の呼び出し
            // (`f := add; f(1,2)`)もcalleeがType::Fnとして伝播するので同じく照合される
            // (TS版`checkCallOfValue`も同じ経路)。pkg修飾呼び出し・メソッド呼び出し・
            // ジェネリック関数はcalleeがANYになるため対象外——従来どおりANYを返す
            if let Type::Fn { params, ret } = &callee_ty {
                if arg_tys.len() != params.len() {
                    ctx.error(*pos, DiagnosticCode::ArgumentCount, format!("expected {} argument(s), got {}", params.len(), arg_tys.len()));
                }
                // 個数が違っても重なる範囲は型照合する(TS版と同じ min(args, params))。
                // paramがANY(union/struct/未対応型)なら常にassignableなので誤検知しない
                for (i, (at, pt)) in arg_tys.iter().zip(params.iter()).enumerate() {
                    if !types::assignable(at, pt) {
                        ctx.error(args[i].pos(), DiagnosticCode::TypeMismatch, format!("argument {}: cannot use {} as {}", i + 1, types::type_to_string(at), types::type_to_string(pt)));
                    }
                }
                (**ret).clone()
            } else {
                ANY
            }
        }
        // struct/array/map/channel/match/is/spawn/select/prop/orElse/無名関数など:
        // milestone 22の対象外なので中へは踏み込まない
        _ => ANY,
    }
}

// 二項演算の妥当性検査+結果型の推論(milestone 25)。TS版`inferBinary`+`checkArithOp`
// (src/checker/expressions.ts)の移植。codegen側のmilestone 13/14で同じロジックを既に
// `checker.rs`へ移植済みだが、あちらは診断を出さず`Result<_, String>`で即失敗する設計
// (最小リゾルバ)。こちらは診断(コード+メッセージ+位置)を積んで走査を続ける。
// **narrowingは不要**: TS版は`&&`/`||`の右辺検査で左辺`x is T`の絞り込みを適用する
// (F-6)が、絞り込みの対象になるのはunion型だけで、full_checkerのスカラースコープでは
// union型は`resolve_scalar_type`でANYに潰れる——ANYはnot-bool/incomparable-types等の
// 全チェックの安全弁を素通りするので、絞り込みの有無で結果が変わらない
// (codegen milestone 14がスクラッチctxで対処した回帰は、そもそもここでは起きない)
fn infer_binary(ctx: &mut FullCheckerCtx, op: TokenType, left: &Expr, right: &Expr, pos: Pos) -> Type {
    // グロブ`use TokenType::*`は`TokenType::Type`(typeキーワード)を取り込んで
    // `Type`enumを隠すため、必要なバリアントだけ明示的にインポートする
    use TokenType::{AndAnd, EqEq, Ge, Gt, Le, Lt, NotEq, OrOr};
    // &&/||: 左右それぞれがbool(またはANY)であること。位置は全体ではなく各オペランド自身
    if op == AndAnd || op == OrOr {
        let lt = infer_expr(ctx, left);
        if !types::type_equals(&lt, &BOOL) && !matches!(lt, Type::Any) {
            ctx.error(left.pos(), DiagnosticCode::NotBool, format!("'{op}' requires bool operands, got {}", types::type_to_string(&lt)));
        }
        let rt = infer_expr(ctx, right);
        if !types::type_equals(&rt, &BOOL) && !matches!(rt, Type::Any) {
            ctx.error(right.pos(), DiagnosticCode::NotBool, format!("'{op}' requires bool operands, got {}", types::type_to_string(&rt)));
        }
        return BOOL;
    }

    let lt = infer_expr(ctx, left);
    let rt = infer_expr(ctx, right);

    // ==/!=: noneとの比較は narrowing の効く`is none`へ一本化する(P1)。それ以外は双方向assignable
    if op == EqEq || op == NotEq {
        if matches!(left, Expr::None { .. }) || matches!(right, Expr::None { .. }) {
            // TS版はfix(`==`→`is`)も付けるが、Diagnosticにfixフィールドがまだ無い
            // (milestone 22スコープ外)ため、コード・メッセージ・位置のみ一致させる
            ctx.error(pos, DiagnosticCode::UseIsNone, "use 'is none' to test for none (== does not narrow the type)");
            return BOOL;
        }
        if !types::assignable(&lt, &rt) && !types::assignable(&rt, &lt) {
            ctx.error(pos, DiagnosticCode::IncomparableTypes, format!("cannot compare {} with {}", types::type_to_string(&lt), types::type_to_string(&rt)));
        }
        return BOOL;
    }

    // < <= > >=: 両方numeric・両方stringy・どちらかANYのいずれか
    if matches!(op, Lt | Le | Gt | Ge) {
        let ok = (types::is_numeric(&lt) && types::is_numeric(&rt))
            || (types::is_stringy(&lt) && types::is_stringy(&rt))
            || matches!(lt, Type::Any)
            || matches!(rt, Type::Any);
        if !ok {
            ctx.error(pos, DiagnosticCode::IncomparableTypes, format!("cannot compare {} with {}", types::type_to_string(&lt), types::type_to_string(&rt)));
        }
        return BOOL;
    }

    // 算術演算(+ - * / %)
    check_arith(ctx, op, &lt, right, &rt, pos)
}

// 算術演算(+ - * / %)の妥当性検査+結果型。TS版`checkArithOp`の移植。
// right_exprはリテラル0除算検査に使う実AST(型だけでは "0" という値まで分からない)。
// int/floatの分類フラグ(intDiv/intMod/intArith)はcodegenの関心事なので、
// 診断だけを担うfull_checkerでは結果型のみ返す
fn check_arith(ctx: &mut FullCheckerCtx, op: TokenType, left: &Type, right_expr: &Expr, right: &Type, pos: Pos) -> Type {
    use TokenType::{Percent, Plus, Slash};
    if op == Plus && types::is_stringy(left) && types::is_stringy(right) {
        return STRING;
    }
    if types::is_numeric(left) && types::is_numeric(right) {
        let is_int = types::type_equals(left, &INT) && types::type_equals(right, &INT);
        // リテラルの0で割る/剰余するのは実行するまでもなくバグ。コンパイル時に弾く
        if is_int
            && matches!(op, Slash | Percent)
            && let Expr::Int { value, pos: rpos } = right_expr
            && value == "0"
        {
            let word = if op == Slash { "division" } else { "modulo" };
            ctx.error(*rpos, DiagnosticCode::DivisionByZero, format!("integer {word} by zero"));
        }
        // 「どちらかANY」の安全弁はis_numeric分岐の中と外の2箇所にある——is_numeric(ANY)は
        // 常にtrueなので、`ANY op 構造体`のような片方だけANYの組み合わせは最初の分岐
        // (両方numeric)を満たさず外側で拾われる(TS版checkArithOpと同じ2段構え)
        if matches!(left, Type::Any) || matches!(right, Type::Any) {
            return ANY;
        }
        return if is_int { INT } else { FLOAT };
    }
    if matches!(left, Type::Any) || matches!(right, Type::Any) {
        return ANY;
    }
    // 不正な演算(型不一致・未絞り込みのunion型への算術等)。TS版と同じく op=='+' で
    // 片側がstring本体のときだけ「str()で変換を」のヒントを添える(TS本体のこの診断で唯一のhint)
    let hint = if op == Plus && (types::type_equals(left, &STRING) || types::type_equals(right, &STRING)) {
        " (hint: use str() to convert values to string)"
    } else {
        ""
    };
    ctx.error(pos, DiagnosticCode::InvalidOperation, format!("invalid operation: {} {op} {}{hint}", types::type_to_string(left), types::type_to_string(right)));
    ANY
}

// 単項演算子(! / -)の妥当性検査(milestone 25)。TS版`case "unary"`の移植——
// `!`はnot-bool・単項`-`はinvalid-operationで、算術二項演算子と同じ`invalid-operation`を共有する
fn infer_unary(ctx: &mut FullCheckerCtx, op: TokenType, operand: &Expr, pos: Pos) -> Type {
    let t = infer_expr(ctx, operand);
    if op == TokenType::Bang {
        if !types::type_equals(&t, &BOOL) && !matches!(t, Type::Any) {
            ctx.error(pos, DiagnosticCode::NotBool, format!("'!' requires bool, got {}", types::type_to_string(&t)));
        }
        return BOOL;
    }
    if !types::is_numeric(&t) {
        ctx.error(pos, DiagnosticCode::InvalidOperation, format!("unary '-' requires int or float, got {}", types::type_to_string(&t)));
    }
    t
}

// arity照合(TS版`expectArity`)。個数が違えばargument-countを積んでfalseを返す。
// メッセージはTS版の組み込み用フォーマット(`name() expects N argument(s), got M`)
fn expect_arity(ctx: &mut FullCheckerCtx, name: &str, got: usize, want: usize, pos: Pos) -> bool {
    if got != want {
        ctx.error(pos, DiagnosticCode::ArgumentCount, format!("{name}() expects {want} argument(s), got {got}"));
        return false;
    }
    true
}

// 「引数がANYでなければ builtin-arg-type を積む」——コレクション系組み込みの引数検査の
// 縮退形。full_checkerのスカラースコープでは配列/map/channelはANYへ潰れるので、TS版の
// `arr.kind === "array"` ガードは「ANYなら素通り」に縮退する。実際のコレクションを渡す
// 典型ケース(型がANY)は無診断、スカラーを誤って渡した場合だけ発火(TS版と一致)。
// **注**: 関数型はmilestone 26以降ANYではなくType::Fnとして追跡される——`push(add, 4)`の
// ように関数値をコレクション組み込みへ渡すと`push() requires an array, got fn(...)`が出る
// (これもTS版と一致——TS版も`arr.kind !== "any"`なら同じエラーを出す)
fn require_kind(ctx: &mut FullCheckerCtx, arg_ty: &Type, arg_pos: Pos, msg: String) {
    if !matches!(arg_ty, Type::Any) {
        ctx.error(arg_pos, DiagnosticCode::BuiltinArgType, msg);
    }
}

// 組み込み関数呼び出しの検査(milestone 27)。TS版`inferBuiltinCall`
// (src/checker/builtins.ts)の移植。
// **スカラースコープでの縮退**: 配列/map/channelはfull_checkerでは常にANYなので、
// TS版がコレクション種別(`arr.kind === "array"`等)でガードする「要素型・添字型・callback署名」の
// 検査(type-mismatch/invalid-index-type/callback-signature-mismatch)は実際には到達せず、
// コレクションをモデル化する将来のmilestoneで拾う。この一歩で実際に効くのは
// (1)全組み込みのarity(argument-count)、(2)スカラー引数の型検査(builtin-arg-type)——
// `len(5)`・`contains(5, x)`・`round(3)`のようにスカラーを渡した場合のエラー、
// (3)スカラー戻り値型(str→string, len→int, round→int 等)。配列を返す組み込み
// (keys/values/sort/split/filter/map/indexOf/get/toInt 等)はANYを返す(配列型は未モデル化)。
// ただしreduceの戻り値だけはaccumulator型でスカラーになりうるためcallbackから計算する
// (下記reduceアーム参照)。**関数型はANYではなくType::Fnとして追跡される**ので、
// コレクション組み込みへ関数値を渡すとrequire_kindが正しく発火する(TS版と一致)。
fn infer_builtin_call(ctx: &mut FullCheckerCtx, name: &str, args: &[Expr], pos: Pos) -> Type {
    // 引数はarityに関わらず全て推論する(未定義名検査のため)
    let at: Vec<Type> = args.iter().map(|a| infer_expr(ctx, a)).collect();
    let n = at.len();
    let ts = |t: &Type| types::type_to_string(t);
    match name {
        "print" => VOID, // 可変長・任意型
        "str" => {
            expect_arity(ctx, name, n, 1, pos);
            STRING
        }
        "len" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::is_stringy(&at[0]) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("len() requires string, array or map, got {}", ts(&at[0])));
            }
            INT
        }
        "push" => {
            if expect_arity(ctx, name, n, 2, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("push() requires an array, got {}", ts(&at[0])));
            }
            VOID
        }
        "error" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::assignable(&at[0], &STRING) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("error() requires a string message, got {}", ts(&at[0])));
            }
            ERROR
        }
        "delete" => {
            if expect_arity(ctx, name, n, 2, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("delete() requires a map, got {}", ts(&at[0])));
            }
            VOID
        }
        "sleep" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::is_numeric(&at[0]) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("sleep() requires milliseconds (int), got {}", ts(&at[0])));
            }
            VOID
        }
        "contains" => {
            if expect_arity(ctx, name, n, 2, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("contains() requires an array, got {}", ts(&at[0])));
            }
            BOOL
        }
        "indexOf" => {
            if expect_arity(ctx, name, n, 2, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("indexOf() requires an array, got {}", ts(&at[0])));
            }
            ANY // 本来 int | none
        }
        "get" => {
            if expect_arity(ctx, name, n, 2, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("get() requires an array, got {}", ts(&at[0])));
            }
            ANY
        }
        "keys" => {
            if expect_arity(ctx, name, n, 1, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("keys() requires a map, got {}", ts(&at[0])));
            }
            ANY // 本来 []K
        }
        "values" => {
            if expect_arity(ctx, name, n, 1, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("values() requires a map, got {}", ts(&at[0])));
            }
            ANY // 本来 []V
        }
        "sort" => {
            if expect_arity(ctx, name, n, 1, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("sort() requires an array, got {}", ts(&at[0])));
            }
            ANY
        }
        "split" => {
            if expect_arity(ctx, name, n, 2, pos) {
                // is_stringyはANYを含まないので明示的にANYを免除(struct/メソッド由来の実stringが
                // full_checkerではANYに潰れるため——免除しないと誤検知。code reviewで発覚)
                if !types::is_stringy(&at[0]) && !matches!(at[0], Type::Any) {
                    ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("split() requires a string, got {}", ts(&at[0])));
                }
                if !types::is_stringy(&at[1]) && !matches!(at[1], Type::Any) {
                    ctx.error(args[1].pos(), DiagnosticCode::BuiltinArgType, format!("split() separator must be a string, got {}", ts(&at[1])));
                }
            }
            ANY // 本来 []string
        }
        "join" => {
            if expect_arity(ctx, name, n, 2, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("join() requires an array, got {}", ts(&at[0])));
                if !types::is_stringy(&at[1]) && !matches!(at[1], Type::Any) {
                    ctx.error(args[1].pos(), DiagnosticCode::BuiltinArgType, format!("join() separator must be a string, got {}", ts(&at[1])));
                }
            }
            STRING
        }
        "trim" | "upper" | "lower" => {
            // is_stringyはANYを含まないので明示的にANYを免除する(len/round等と同じ)——
            // full_checkerではstruct/メソッド/pkg/ジェネリック由来の実stringもANYに潰れるため、
            // 免除しないと`trim(s.name)`(s.nameは実string)を誤検知する(code reviewで発覚)
            if expect_arity(ctx, name, n, 1, pos) && !types::is_stringy(&at[0]) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("{name}() requires a string, got {}", ts(&at[0])));
            }
            STRING
        }
        "toInt" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::is_stringy(&at[0]) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("toInt() requires a string, got {}", ts(&at[0])));
            }
            ANY // 本来 int | error
        }
        "toFloat" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::type_equals(&at[0], &INT) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("toFloat() requires an int, got {}", ts(&at[0])));
            }
            FLOAT
        }
        "round" | "floor" | "ceil" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::type_equals(&at[0], &FLOAT) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("{name}() requires a float, got {}", ts(&at[0])));
            }
            INT
        }
        "close" => {
            if expect_arity(ctx, name, n, 1, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("close() requires a channel, got {}", ts(&at[0])));
            }
            VOID
        }
        "filter" => {
            if expect_arity(ctx, name, n, 2, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("filter() requires an array, got {}", ts(&at[0])));
            }
            ANY
        }
        "map" => {
            if expect_arity(ctx, name, n, 2, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("map() requires an array, got {}", ts(&at[0])));
            }
            ANY
        }
        "reduce" => {
            if expect_arity(ctx, name, n, 3, pos) {
                require_kind(ctx, &at[0], args[0].pos(), format!("reduce() requires an array, got {}", ts(&at[0])));
            }
            // reduceの戻り値はaccumulator型でスカラーになりうる(map/filter等の配列戻り値と違い
            // モデル化できる)。Type::Fnはmilestone 26で追跡済みなので、TS版と同じく
            // callbackの第1引数型(f.params[0])を返す——fnでなければ初期値(args[2])の型、
            // それも無ければANY(TS版 `f.kind==="fn" && f.params.length===2 ? f.params[0] : args[2] ?? ANY`)。
            // これにより `round(reduce(xs, add, 0))`(reduce→int)のような後続検査がTS版と一致する
            match at.get(1) {
                Some(Type::Fn { params, .. }) if params.len() == 2 => params[0].clone(),
                _ => at.get(2).cloned().unwrap_or(ANY),
            }
        }
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
                // mut宣言はリテラル型を広げる(`mut s := "a"`のsはstring型——後で
                // `s = "b"`と別リテラルを代入できるように)。TS版statements.tsの
                // `stmt.mutable ? widenLiteral(t) : t`と同じ
                let ty = if *mutable { types::widen_literal(ty) } else { ty };
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
        Stmt::IncDec { target, op, pos } => {
            // TS版`case "incDec"`と同じ順序: まず対象の型を推論(identなら未定義名も検査)し、
            // int/floatでなければinvalid-operation(算術二項演算子と共有する診断)、
            // そのあとidentなら可変性を検査する
            let t = infer_expr(ctx, target);
            if !types::is_numeric(&t) {
                ctx.error(*pos, DiagnosticCode::InvalidOperation, format!("'{op}' requires int or float, got {}", types::type_to_string(&t)));
            }
            if let Expr::Ident { name, .. } = target {
                // 借用を跨がないよう可変性をboolへ落としてからctx.errorを呼ぶ
                let immutable = matches!(ctx.lookup(name), Some(b) if !b.mutable);
                if immutable {
                    ctx.error(*pos, DiagnosticCode::ImmutableAssignment, format!("'{name}' was declared without 'mut' and cannot be reassigned"));
                }
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
            // forヘッダ変数は(:=で書いても)常にmutable扱い——codegenも常にletで出し、
            // postで書き換わる。ShortVarDeclのmut分岐と同じくリテラル型を広げないと、
            // milestone 25でExpr::StringをType::Literalにした結果、
            // `for s := "a"; s != "z"; s = "b"` のsがLiteral("a")のままになり、
            // 比較(incomparable-types)・再代入(type-mismatch)が誤検知になる
            // (code reviewで発見した回帰)
            let ty = types::widen_literal(infer_expr(ctx, value));
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

// 関数のシグネチャ型(Type::Fn)。milestone 26でcheck_programがトップレベル関数を
// この型で登録するようになり、呼び出し側の個数・型照合(argument-count/type-mismatch)が
// 効くようになった。params/retはスカラースコープで解決する(union/struct/配列/pkg修飾型は
// resolve_scalar_typeでANYへ潰れる——個数照合は常に効き、型照合はスカラーのみ効く)。
// TS版`checkPackage`もトップレベル関数を通常のdeclareBindingでFn型として登録する
fn fn_signature(f: &FnDecl) -> Type {
    let params = f.params.iter().map(|p| resolve_scalar_type(&p.type_node)).collect();
    let ret = f.ret.as_ref().map(resolve_scalar_type).unwrap_or(VOID);
    Type::Fn { params, ret: Box::new(ret) }
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
    // 定数を検査+登録し、最後に関数本体を検査する。
    // milestone 26以降、非ジェネリックの自由関数は`fn_signature`でシグネチャ付き
    // (`Type::Fn`)で登録し、呼び出し側の個数・型照合(argument-count/type-mismatch)を
    // 効かせる。**ジェネリック関数(`type_params`有り)だけは従来どおりANYで登録**——
    // TS版はジェネリック呼び出しを別経路(`inferGenericCall`、型パラメータ推論つき)で
    // 扱い、引数不足なら`generic-inference-failed`を出す。full_checkerはこの推論を
    // 未実装なので、`Type::Fn`で登録するとTS版と違って`argument-count`を出してしまう
    // (診断コードのTS非互換)。ANY登録で呼び出しを丸ごと対象外にしておく。
    // `program.fns`にはstructのメソッド(`f.receiver.is_some()`)も自由関数と同じ配列で
    // 混在している——TS版`checkPackage`はレシーバ付きなら`declareMethod`で別の
    // `methodTable`へ登録し、`scopes[0]`(自由関数と同じ名前空間)には絶対に入れない
    // (`src/checker/functions.ts`のコメント「グローバルscopeには置かない」参照。
    // メソッドの名前空間は自由関数と完全分離——異なるstructが同名メソッドを持てる)。
    // structはmilestone 22/23とも対象外なので、メソッドは名前登録・本体検査どちらも
    // 単純にスキップする(誤って自由関数と同じ扱いにすると、別々のstructの同名メソッドが
    // already-declaredの誤検知になる)
    for f in &program.fns {
        if f.receiver.is_none() {
            let ty = if f.type_params.is_empty() { fn_signature(f) } else { ANY };
            ctx.declare(&f.name, ty, f.pos, false);
        }
    }
    for c in &program.consts {
        check_top_level_const(&mut ctx, c);
    }
    // エントリポイント検査(TS版`checker/modules.ts`のrequireMain分岐)。TS版では
    // `requireMain = pkg === "main" && !testMode`だが、full_checkerは単一ファイル
    // (=mainパッケージ)専用でテストモードも未対応なので、ここでは常に要求する
    // (依存先パッケージ・`mesh test`はスコープ外——足すときにフラグ化する)。
    // レシーバ無しの`fn main`を探し、無ければ1:1(ファイル先頭)を指してmissing-main、
    // あれば引数ありor戻り値ありでinvalid-main-signature(エントリポイントは
    // 引数を取らず何も返さない)
    match program.fns.iter().find(|f| f.name == "main" && f.receiver.is_none()) {
        None => ctx.error(
            Pos { line: 1, col: 1 },
            DiagnosticCode::MissingMain,
            "missing 'fn main()' — Mesh programs start from main",
        ),
        Some(main) => {
            if !main.params.is_empty() || main.ret.is_some() {
                ctx.error(
                    main.pos,
                    DiagnosticCode::InvalidMainSignature,
                    "'fn main()' must take no parameters and return nothing",
                );
            }
        }
    }
    for f in &program.fns {
        if f.receiver.is_none() {
            check_fn(&mut ctx, f);
        }
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

    // codegen経路と同じくjson structのデコーダを合成してから検査する(mesh checkのrun_checkが
    // やるのと同じ前処理)。json struct由来の合成関数を検査対象に含めたいテスト用
    fn check_with_json(src: &str) -> Vec<Diagnostic> {
        let mut program = parse(src).expect("test source must parse");
        crate::json_decode::synthesize_json_decoders(&mut program).expect("synthesis must succeed");
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
    fn 異なるstructの同名メソッドはalready_declaredの誤検知にならない() {
        // 回帰テスト: program.fnsには自由関数とstructのメソッド(receiver付き)が
        // 同じ配列に混在している。メソッドの名前空間は自由関数と完全分離(TS版
        // `checker/functions.ts`の`declareMethod`はscopes[0]に触れない)なので、
        // 異なるstructが同名メソッドを持つのは正当。フィルタし忘れるとここが
        // already-declaredの誤検知になる
        let diags = check(
            "struct Todo {\n    title: string\n}\n\nfn (t: Todo) describe() string {\n    return t.title\n}\n\nstruct User {\n    name: string\n}\n\nfn (u: User) describe() string {\n    return u.name\n}\n\nfn main() {\n}\n",
        );
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn ローカル変数がメソッド名と同じでもshadowingの誤検知にならない() {
        let diags = check(
            "struct Todo {\n    title: string\n}\n\nfn (t: Todo) describe() string {\n    return t.title\n}\n\nfn main() {\n    describe := 5\n    print(describe)\n}\n",
        );
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn トップレベル定数の型が参照側でも正しく伝播する() {
        // milestone 22時点ではトップレベル定数を参照する式は常にANYへフォールバック
        // していたが、milestone 23でconstをctx.declare()経由で本物のBindingとして
        // 登録するようになった副次効果で、参照側でも実際の型と照合されるようになった
        let diags = check("limit: int = 10\nfn main() {\n    x: string = limit\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn 戻り値の型不一致はtype_mismatchを報告する() {
        // main無しだとmissing-mainが上乗せされるため、type-mismatchを切り出せるよう
        // 空のmainを添える(TS版でもmain無し単一ファイルはmissing-mainを併発する)
        let diags = check("fn helper() int {\n    return \"oops\"\n}\nfn main() {}\n");
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

    #[test]
    fn main無しはmissing_mainを報告する() {
        let diags = check("fn helper() int { return 1 }\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::MissingMain);
        // ファイル先頭(1:1)を指す
        assert_eq!(diags[0].pos, Pos { line: 1, col: 1 });
    }

    #[test]
    fn mainに引数があるとinvalid_main_signatureを報告する() {
        let diags = check("fn main(x: int) {\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidMainSignature);
    }

    #[test]
    fn mainに戻り値があるとinvalid_main_signatureを報告する() {
        let diags = check("fn main() int {\n    return 0\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidMainSignature);
    }

    #[test]
    fn 正しい空のmainはエントリポイント診断を出さない() {
        let diags = check("fn main() {}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn レシーバ付きのmainはエントリポイントとみなされずmissing_mainになる() {
        // メソッドの名前空間は自由関数と完全分離——`fn (r: R) main()`は
        // エントリポイントの`main`ではない(TS版も`!f.receiver`で除外する)。
        // structはfull_checkerのスコープ外なのでメソッド本体は検査されないが、
        // 名前だけは「自由関数のmainが無い」と判定されmissing-mainになるべき
        let diags = check("fn (r: R) main() {}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::MissingMain);
    }

    // ---- milestone 25: 演算子の妥当性検査 ----

    #[test]
    fn 型不一致な算術はinvalid_operationを報告する() {
        let diags = check("fn main() {\n    x := true - false\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "invalid operation: bool - bool");
    }

    #[test]
    fn 文字列連結でない加算のstrヒント() {
        // op=='+' かつ片側がstring**本体**のときだけTS版はstr()ヒントを添える。
        // 文字列リテラル("a")はリテラル型でtype_equals(_, STRING)がfalseなのでヒント無し
        // (TS版も同じ挙動——mut宣言でwidenされたstring型のときだけヒントが出る)。
        // ここではmut変数(string型)を使ってヒントが出る側を検証する
        let diags = check("fn main() {\n    mut m := \"b\"\n    x := 1 + m\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "invalid operation: int + string (hint: use str() to convert values to string)");
    }

    #[test]
    fn 加算の右辺が文字列リテラルのときはヒントを出さない() {
        // 上のテストの対(リテラルはstring本体ではないのでヒント無し、TS版と一致)
        let diags = check("fn main() {\n    x := 1 + \"a\"\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "invalid operation: int + \"a\"");
    }

    #[test]
    fn 比較不能な型の比較はincomparable_typesを報告する() {
        // 文字列リテラルの型は`string`ではなく`"a"`と表示される(TS版と一致)
        let diags = check("fn main() {\n    b := 1 < \"a\"\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::IncomparableTypes);
        assert_eq!(diags[0].message, "cannot compare int with \"a\"");
    }

    #[test]
    fn 比較不能な型の等価比較もincomparable_typesを報告する() {
        let diags = check("fn main() {\n    b := 1 == \"a\"\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::IncomparableTypes);
    }

    #[test]
    fn noneとの等価比較はuse_is_noneを報告する() {
        let diags = check("fn main() {\n    if 1 == none {\n        print(\"x\")\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UseIsNone);
    }

    #[test]
    fn bool以外への論理演算子はnot_boolを報告する() {
        let diags = check("fn main() {\n    b := 1 && true\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::NotBool);
        assert_eq!(diags[0].message, "'&&' requires bool operands, got int");
    }

    #[test]
    fn 単項否定のbool以外はnot_boolを報告する() {
        let diags = check("fn main() {\n    b := !5\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::NotBool);
        assert_eq!(diags[0].message, "'!' requires bool, got int");
    }

    #[test]
    fn 単項マイナスのnumeric以外はinvalid_operationを報告する() {
        let diags = check("fn main() {\n    x := -\"a\"\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "unary '-' requires int or float, got \"a\"");
    }

    #[test]
    fn numeric以外のインクリメントはinvalid_operationを報告する() {
        let diags = check("fn main() {\n    mut s := \"a\"\n    s++\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "'++' requires int or float, got string");
    }

    #[test]
    fn リテラルのゼロ除算はdivision_by_zeroを報告する() {
        let diags = check("fn main() {\n    x := 1 / 0\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::DivisionByZero);
        assert_eq!(diags[0].message, "integer division by zero");
    }

    #[test]
    fn リテラルのゼロ剰余もdivision_by_zeroを報告する() {
        let diags = check("fn main() {\n    x := 1 % 0\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::DivisionByZero);
        assert_eq!(diags[0].message, "integer modulo by zero");
    }

    #[test]
    fn forヘッダの文字列変数はwidenされ誤検知しない() {
        // 回帰(code reviewで発見): milestone 25でExpr::StringをType::Literalにした際、
        // ShortVarDeclはmut時にwiden_literalしたがcheck_for_init(常にmutableなforヘッダ
        // 変数)を見落としていた。widenしないとsがLiteral("a")のままになり、`s != "z"`が
        // incomparable-types、`s = "b"`がtype-mismatchの誤検知になる(TS版は無診断)
        let diags = check("fn main() {\n    for s := \"a\"; s != \"z\"; s = \"b\" {\n        print(s)\n    }\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 正当な演算は診断を出さない() {
        // 算術・文字列連結・浮動小数点・論理混在・比較・単項・mut文字列再代入・inc/decを一通り
        let diags = check(
            "fn main() {\n\
            \x20   a := 1 + 2 * 3\n\
            \x20   b := \"x\" + \"y\"\n\
            \x20   c := 1.5 / 2.0\n\
            \x20   d := true && false || !true\n\
            \x20   e := a < 10\n\
            \x20   f := b < \"z\"\n\
            \x20   mut s := \"hi\"\n\
            \x20   s = \"bye\"\n\
            \x20   mut n := 0\n\
            \x20   n++\n\
            \x20   n = -n\n\
            \x20   print(\"${a} ${c} ${d} ${e} ${f} ${s} ${n}\")\n\
            }\n",
        );
        assert_eq!(diags, vec![]);
    }

    // ---- milestone 26: argument-count(ユーザー定義関数の引数個数・型) ----

    #[test]
    fn 引数不足はargument_countを報告する() {
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\nfn main() {\n    x := add(1)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
        assert_eq!(diags[0].message, "expected 2 argument(s), got 1");
    }

    #[test]
    fn 引数過多もargument_countを報告する() {
        let diags = check("fn greet(name: string) {\n    print(name)\n}\nfn main() {\n    greet(\"a\", \"b\")\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
        assert_eq!(diags[0].message, "expected 1 argument(s), got 2");
    }

    #[test]
    fn 引数の型不一致はtype_mismatchを報告する() {
        let diags = check("fn sq(n: int) int {\n    return n * n\n}\nfn main() {\n    x := sq(\"hi\")\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "argument 1: cannot use \"hi\" as int");
    }

    #[test]
    fn 正しい引数の呼び出しは診断を出さない() {
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\nfn main() {\n    print(\"${add(2, 3)}\")\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 前方参照の呼び出しでも個数検査が効く() {
        // 本体検査より前に全関数を登録するので、後で定義する関数の呼び出しも照合できる
        let diags = check("fn main() {\n    later(1, 2)\n}\nfn later(a: int) {\n    print(a)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
    }

    #[test]
    fn 関数の戻り値型が呼び出し側に伝播する() {
        // addはintを返すのでstringへの代入はtype-mismatch(呼び出し結果がANYに潰れない)
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\nfn main() {\n    x: string = add(1, 2)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn 組み込み関数の呼び出しはargument_count対象外で誤検知しない() {
        // milestone 27で組み込みはintercept経由で検査するようになったが、printは可変長
        // なのでarity検査せず、複数引数でも誤検知しない(milestone 26時点では「calleeがANY」
        // が理由だったが、milestone 27でinfer_builtin_callがprintをVOIDとして素通しする形に変わった)
        let diags = check("fn main() {\n    print(1, 2, 3)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn ジェネリック関数の呼び出しはargument_count対象外で誤検知しない() {
        // ジェネリック関数はANY登録なので個数検査されない——TS版は引数不足時に
        // generic-inference-failedを出すが、full_checkerは型パラメータ推論が未実装のため
        // 意図的に対象外(argument-countをTS非互換に出してしまうのを避ける)
        let diags = check("fn identity<T>(x: T) T {\n    return x\n}\nfn main() {\n    identity(1, 2, 3)\n    print(1)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn ローカル変数に束縛した関数値の呼び出しも個数検査される() {
        // `f := add` で f の型は Type::Fn として伝播するので、f(1) は argument-count
        // (TS版 checkCallOfValue も同じ経路——値呼び出しは対象外ではない)
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\nfn main() {\n    f := add\n    x := f(1)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
    }

    // ---- milestone 27: 組み込み関数の検査 ----

    #[test]
    fn 組み込み関数の個数不一致はargument_countを報告する() {
        let diags = check("fn main() {\n    x := str(1, 2)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
        assert_eq!(diags[0].message, "str() expects 1 argument(s), got 2");
    }

    #[test]
    fn lenへの非文字列スカラーはbuiltin_arg_typeを報告する() {
        let diags = check("fn main() {\n    x := len(5)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "len() requires string, array or map, got int");
    }

    #[test]
    fn roundへの非floatはbuiltin_arg_typeを報告する() {
        let diags = check("fn main() {\n    x := round(3)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "round() requires a float, got int");
    }

    #[test]
    fn コレクション組み込みにスカラーを渡すとbuiltin_arg_typeを報告する() {
        // contains(5, x): 第1引数が配列でなく具体スカラー→"requires an array"(TS版と一致)
        let diags = check("fn main() {\n    b := contains(5, 3)\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "contains() requires an array, got int");
    }

    #[test]
    fn 正しい組み込み呼び出しは診断を出さない() {
        // 文字列組み込み・可変長print・スカラー戻り値の合成
        let diags = check("fn main() {\n    s := trim(\"  hi  \")\n    u := upper(s)\n    parts := split(u, \",\")\n    print(len(s), u, parts)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 配列map引数の組み込みは誤検知しない() {
        // 配列/mapはスカラースコープでANYに潰れるため、push/contains/lenは無診断
        // (要素型検査はコレクションをモデル化する将来のmilestone)
        let diags = check("fn main() {\n    mut xs: int[] = [1, 2, 3]\n    push(xs, 4)\n    b := contains(xs, 2)\n    n := len(xs)\n    print(\"${b} ${n}\")\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 組み込み引数の未定義名も検出する() {
        // arityが合っていても引数式は推論されるので未定義名を拾う
        let diags = check("fn main() {\n    x := str(missing)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn any由来の文字列を文字列組み込みへ渡しても誤検知しない() {
        // 回帰(code reviewで発覚): ジェネリック関数の結果(ANYに潰れる)は実際にはstringでも
        // full_checkerではANY。trim/toInt/split/joinの文字列引数検査がANYを免除していなかったため、
        // `trim(s)`(sはANY由来の実string)を誤ってbuiltin-arg-typeにしていた。TS版は無診断。
        let diags = check("fn identity<T>(x: T) T {\n    return x\n}\nfn main() {\n    s := identity(\"hi\")\n    t := trim(s)\n    parts := split(s, \",\")\n    n := toInt(s)\n    print(\"${t} ${parts} ${n}\")\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn reduceの戻り値はaccumulator型になり後続の型検査に効く() {
        // 回帰(code reviewで発覚): reduceは常にANYを返していたが、accumulator型は
        // callback(Type::Fn、milestone 26で追跡済み)から計算できるスカラー。
        // reduce(xs, add, 0)→int を round に渡すとTS版と同じくbuiltin-arg-typeが出る
        let diags = check("fn add(acc: int, x: int) int {\n    return acc + x\n}\nfn main() {\n    mut xs: int[] = [1, 2, 3]\n    total := reduce(xs, add, 0)\n    y := round(total)\n    print(y)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "round() requires a float, got int");
    }

    // ---- milestone 28: mesh checkでもjson structデコーダを合成する ----

    #[test]
    fn json構造体の合成デコーダ呼び出しは合成後undefined_nameにならない() {
        // 生成デコーダは json.field 等を呼ぶので mesh/json のimportが要る(合成の前提)
        let src = "import \"mesh/json\"\njson struct User { name: string }\nfn main() {\n    u := decodeUser(5)\n    print(u)\n}\n";
        // 合成しない生の検査では decodeUser が未定義扱い(バグの再現)
        let raw = check(src);
        assert!(raw.iter().any(|d| d.code == DiagnosticCode::UndefinedName), "合成前はundefined-nameになるはず");
        // 合成後(run_check/codegenと同じ前処理)は decodeUser が登録され誤検知しない
        let synthed = check_with_json(src);
        assert_eq!(synthed, vec![]);
    }
}
