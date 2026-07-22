// checker(最小リゾルバ)= codegenが必要とする最小限の型情報を解決する。TS版
// `src/checker/`(約2900行)のフェーズ1〜2相当(宣言収集)+フェーズ5の必要最小限
// (式推論、ただし診断は出さない)を移植したもの。フルcheckerの移植ではない。
//
// **設計判断(TS版からの意図的な逸脱)**: TS版はASTノードへ直接`resolvedType`等を書き込み、
// codegenが後から読む「checker→codegen 2パス+共有ミュータブルAST」設計。Rustの`Expr`は
// 不変構造体でこのパターンに向かない(`RefCell`だらけにするか、別のside-tableを持ち回るかの
// 選択になり、どちらも複雑さが増す)。代わりに**resolverとcodegenを1回のトラバーサルに融合**
// する——codegenが式を生成する直前に、その場で`infer_expr`を呼んで必要な型情報だけを得る
// (例: Index式を生成する前にtargetの型を推論してmapか配列か決める)。TS側が2パスに
// 分けていたのは主に「型エラーを1回で全部集めて報告する」ため(診断目的)だが、
// このリゾルバは診断を出さない設計なのでこの制約自体が無く、融合して問題ない
//
// **診断は出さない**: パーサを通った時点で構文的には正しい前提。型不一致等の意味検証は
// このリゾルバの対象外(フルchecker移植の段階で改めて対応する)。未解決の名前・型は
// `Type::Any`へフォールバックし、コンパイラ自体をpanicさせない

use crate::ast::{Expr, TypeNode};
use crate::token::TokenType;
use crate::types::{self, ANY, BOOL, ERROR, FLOAT, INT, NONE, STRING, VOID, Type};
use std::collections::HashMap;

// 組み込み関数。TS版`checker/context.ts`のBUILTINSをそのまま移植(特殊な検査は
// このリゾルバの対象外なので、名前の集合だけが必要)
pub const BUILTINS: &[&str] = &[
    "print", "len", "push", "str", "error", "sleep", "delete", "contains", "indexOf", "get", "keys", "values", "sort", "split", "join", "trim",
    "upper", "lower", "toInt", "toFloat", "round", "floor", "ceil", "filter", "map", "reduce", "close",
];

pub fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

// TS版`CheckerCtx`のうち、M1(struct/パッケージ無し)で使う部分だけを持つ。
// スコープスタック(narrowingは対象外)とトップレベル関数のシグネチャ表のみ
pub struct CheckerCtx {
    scopes: Vec<HashMap<String, Type>>,
    fn_decls: HashMap<String, Type>,
}

impl Default for CheckerCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckerCtx {
    pub fn new() -> Self {
        CheckerCtx { scopes: vec![HashMap::new()], fn_decls: HashMap::new() }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    // ブランク識別子("_")は捨てる用(TS版declareBindingと同じ)。予約語・シャドーイング等の
    // 診断はこのリゾルバの対象外なので、単純にスコープへ積むだけ
    pub fn declare(&mut self, name: &str, ty: Type) {
        if name == "_" {
            return;
        }
        self.scopes.last_mut().expect("scopes is never empty").insert(name.to_string(), ty);
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    pub fn declare_fn(&mut self, name: &str, ty: Type) {
        self.fn_decls.insert(name.to_string(), ty);
    }

    pub fn lookup_fn(&self, name: &str) -> Option<&Type> {
        self.fn_decls.get(name)
    }
}

// 型注釈(構文)を内部表現の型へ変換。TS版`checker/types-resolve.ts`のresolveTypeのうち、
// M1で必要な部分を移植。ユーザー定義のtype alias解決(knot-tying。循環検出込み)は
// struct/自己参照型を移植する段階まで先送り——今は解決できない名前を、名前だけを覚えた
// 空フィールドのstruct型として素通しする(M1はそもそも型注釈にユーザー定義名を使わない
// ため実質未使用のフォールバック)
pub fn resolve_type_node(node: &TypeNode) -> Type {
    match node {
        TypeNode::Union { members, .. } => types::union_of(members.iter().map(resolve_type_node).collect()),
        TypeNode::Literal { value, .. } => Type::Literal(value.clone()),
        TypeNode::Name { name, .. } => match name.as_str() {
            "int" => INT,
            "float" => FLOAT,
            "string" => STRING,
            "bool" => BOOL,
            "void" => VOID,
            "error" => ERROR,
            "none" => NONE,
            "closed" => types::CLOSED,
            _ => Type::Struct { name: name.clone(), fields: vec![], is_error_type: false },
        },
        TypeNode::Array { elem, .. } => Type::Array(Box::new(resolve_type_node(elem))),
        TypeNode::Chan { elem, .. } => Type::Chan(Box::new(resolve_type_node(elem))),
        TypeNode::MapType { key, value, .. } => Type::Map { key: Box::new(resolve_type_node(key)), value: Box::new(resolve_type_node(value)) },
        TypeNode::FnType { params, ret, .. } => Type::Fn {
            params: params.iter().map(resolve_type_node).collect(),
            ret: Box::new(ret.as_deref().map(resolve_type_node).unwrap_or(VOID)),
        },
        TypeNode::StructType { fields, .. } => Type::Struct {
            name: types::ANONYMOUS_STRUCT_NAME.to_string(),
            fields: fields.iter().map(|f| types::StructField { name: f.name.clone(), type_: resolve_type_node(&f.type_node) }).collect(),
            is_error_type: false,
        },
    }
}

pub fn resolve_return_type(ret: &Option<TypeNode>) -> Type {
    ret.as_ref().map(resolve_type_node).unwrap_or(VOID)
}

// 式の型を推論する。TS版`checker/expressions.ts`のinferExprのうち、M1で必要な部分
// (スカラー・ident・二項演算・文字列補間・呼び出し)だけを移植。診断は出さない
// (対応する型が決まらない式には最善努力でANYを返す——異常系として弾くのではなく、
// codegen側の「まだ対応していない構文」チェックに委ねる)
pub fn infer_expr(ctx: &CheckerCtx, expr: &Expr) -> Type {
    match expr {
        Expr::Int { .. } => INT,
        Expr::Float { .. } => FLOAT,
        // 文字列リテラルはリテラル型として推論する("active"は型"active")。
        // stringが必要な場所へは部分型として入る
        Expr::String { value, .. } => Type::Literal(value.clone()),
        Expr::Bool { .. } => BOOL,
        Expr::None { .. } => NONE,
        // 補間される式はどの型でもよい(printと同じ)。結果は常にstring
        Expr::Interp { .. } => STRING,
        Expr::Ident { name, .. } => ctx.lookup(name).cloned().unwrap_or(ANY),
        Expr::Binary { op, left, right, .. } => infer_binary(ctx, *op, left, right).result,
        Expr::Unary { operand, .. } => infer_expr(ctx, operand),
        Expr::Call { callee, .. } => infer_call(ctx, callee),
        // M1未対応の式(struct/map/channel/error伝播等)はANYへ最善努力でフォールバックする。
        // codegen側がこれらの構文自体を「まだ対応していません」と明確なエラーにするので、
        // ここで型を誤魔化しても実害は無い
        _ => ANY,
    }
}

pub struct BinaryInfo {
    pub result: Type,
    pub int_div: bool,  // int同士の除算は切り捨て+ゼロ検査(__idiv)
    pub int_mod: bool,  // int同士の剰余はゼロ検査(__imod)
    pub int_arith: bool, // F-10: int同士の+-*はsafe-integer検査(__iarith)
}

fn no_flags(result: Type) -> BinaryInfo {
    BinaryInfo { result, int_div: false, int_mod: false, int_arith: false }
}

// 二項演算の型推論+算術演算のint/float分類。TS版のinferBinary+checkArithOpを移植
// (narrowing・診断は対象外)。**codegen側もこの関数を直接呼ぶ**——TS版はintDiv/intMod/
// intArithをASTへ書き込んでcodegenが後で読むが、このリゾルバは融合設計なので、
// codegenが二項演算式を生成するその場でこの関数を呼び、resultではなくフラグを見て
// __idiv/__imod/__iarithの要否を決める
pub fn infer_binary(ctx: &CheckerCtx, op: TokenType, left: &Expr, right: &Expr) -> BinaryInfo {
    match op {
        // &&/||や比較演算子の型検査(bool要求・narrowing)は診断専用なので対象外。
        // 結果が常にBOOLであることだけが分かればよく、オペランドの型を推論する必要すら無い
        TokenType::AndAnd | TokenType::OrOr | TokenType::EqEq | TokenType::NotEq | TokenType::Lt | TokenType::Le | TokenType::Gt | TokenType::Ge => {
            no_flags(BOOL)
        }
        TokenType::Plus | TokenType::Minus | TokenType::Star | TokenType::Slash | TokenType::Percent => {
            let lt = infer_expr(ctx, left);
            let rt = infer_expr(ctx, right);
            check_arith_op(op, &lt, &rt)
        }
        _ => no_flags(ANY),
    }
}

// 算術演算(+ - * / %)の型検査。TS版checkArithOpを移植。コンパイル時0除算検出は
// 診断機能なので対象外(実行時のpanicヘルパ〈__idiv/__imod〉が代わりに担う)
fn check_arith_op(op: TokenType, left: &Type, right: &Type) -> BinaryInfo {
    if op == TokenType::Plus && types::is_stringy(left) && types::is_stringy(right) {
        return no_flags(STRING);
    }
    if types::is_numeric(left) && types::is_numeric(right) {
        if matches!(left, Type::Any) || matches!(right, Type::Any) {
            return no_flags(ANY);
        }
        let is_int = types::type_equals(left, &INT) && types::type_equals(right, &INT);
        return BinaryInfo {
            result: if is_int { INT } else { FLOAT },
            int_div: op == TokenType::Slash && is_int,
            int_mod: op == TokenType::Percent && is_int,
            int_arith: is_int && matches!(op, TokenType::Plus | TokenType::Minus | TokenType::Star),
        };
    }
    // 不正な演算(型不一致)——診断は出さないのでANYへフォールバックする
    no_flags(ANY)
}

// 呼び出し式の型推論。自由関数(fn_decls)の戻り値型 → 組み込みの戻り値型の順で引く
// (code review指摘: 組み込みを素通ししてANYへ落としていたため、例えば`round(x) / round(y)`が
// int同士の演算と分からず__idivが呼ばれず浮動小数点演算になっていた——組み込みの戻り値型を
// 引けるようにして修正)。パッケージ修飾・structメソッドの解決は次のマイルストーン以降
// (structが無いのでtargetの型がstructになることも無く、自然にこの経路には来ない)
fn infer_call(ctx: &CheckerCtx, callee: &Expr) -> Type {
    if let Expr::Ident { name, .. } = callee {
        if let Some(Type::Fn { ret, .. }) = ctx.lookup_fn(name) {
            return (**ret).clone();
        }
        if let Some(t) = infer_builtin_call(name) {
            return t;
        }
    }
    ANY
}

// M1のcodegenが実際に生成できる組み込みのうち、引数の型によらず戻り値型が決まるものだけを
// 解決する(TS版`checker/builtins.ts`の`inferBuiltinCall`のうち、引数非依存の部分を移植)。
// get/sort等(引数の配列要素型に依存)はこのリゾルバでは追わずANYのままにする——M1のcodegenは
// 配列そのものをまだ生成できないため、その2つがここに来ることは無く実害が無い
fn infer_builtin_call(name: &str) -> Option<Type> {
    Some(match name {
        "print" | "sleep" | "push" | "close" => VOID,
        "str" | "join" | "trim" | "upper" | "lower" => STRING,
        "toInt" => types::union_of(vec![INT, ERROR]),
        "toFloat" => FLOAT,
        "round" | "floor" | "ceil" => INT,
        "error" => ERROR,
        "contains" => BOOL,
        "indexOf" => types::union_of(vec![INT, NONE]),
        "split" => Type::Array(Box::new(STRING)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Pos;

    fn pos() -> Pos {
        Pos { line: 1, col: 1 }
    }

    fn int_lit(value: &str) -> Expr {
        Expr::Int { value: value.to_string(), pos: pos() }
    }

    #[test]
    fn resolve_type_nodeはプリミティブ名を解決する() {
        let node = TypeNode::Name { name: "int".into(), pkg: None, pos: pos() };
        assert!(matches!(resolve_type_node(&node), Type::Prim(_)));
        assert!(types::type_equals(&resolve_type_node(&node), &INT));
    }

    #[test]
    fn infer_exprはリテラルの型を返す() {
        let ctx = CheckerCtx::new();
        assert!(types::type_equals(&infer_expr(&ctx, &int_lit("1")), &INT));
        assert!(matches!(infer_expr(&ctx, &Expr::String { value: "hi".into(), pos: pos() }), Type::Literal(_)));
    }

    #[test]
    fn infer_exprはスコープに宣言した変数の型を引ける() {
        let mut ctx = CheckerCtx::new();
        ctx.declare("i", INT);
        let ident = Expr::Ident { name: "i".into(), pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &ident), &INT));
    }

    #[test]
    fn 未定義の識別子はanyにフォールバックする() {
        let ctx = CheckerCtx::new();
        let ident = Expr::Ident { name: "undefined_var".into(), pos: pos() };
        assert!(matches!(infer_expr(&ctx, &ident), Type::Any));
    }

    #[test]
    fn int同士の剰余はint_modフラグが立つ() {
        let ctx = CheckerCtx::new();
        let info = infer_binary(&ctx, TokenType::Percent, &int_lit("15"), &int_lit("3"));
        assert!(info.int_mod);
        assert!(!info.int_div);
        assert!(!info.int_arith);
        assert!(types::type_equals(&info.result, &INT));
    }

    #[test]
    fn int同士の加減乗はint_arithフラグが立つ() {
        let ctx = CheckerCtx::new();
        for op in [TokenType::Plus, TokenType::Minus, TokenType::Star] {
            let info = infer_binary(&ctx, op, &int_lit("1"), &int_lit("2"));
            assert!(info.int_arith, "{op:?} should set int_arith");
        }
    }

    #[test]
    fn int同士の除算はint_divフラグが立つ() {
        let ctx = CheckerCtx::new();
        let info = infer_binary(&ctx, TokenType::Slash, &int_lit("7"), &int_lit("2"));
        assert!(info.int_div);
    }

    #[test]
    fn floatが混ざるとint系フラグは立たない() {
        let ctx = CheckerCtx::new();
        let float_lit = Expr::Float { value: "1.5".into(), pos: pos() };
        let info = infer_binary(&ctx, TokenType::Plus, &int_lit("1"), &float_lit);
        assert!(!info.int_arith);
        assert!(types::type_equals(&info.result, &FLOAT));
    }

    #[test]
    fn 比較演算子は常にboolでフラグを立てない() {
        let ctx = CheckerCtx::new();
        let info = infer_binary(&ctx, TokenType::EqEq, &int_lit("1"), &int_lit("2"));
        assert!(types::type_equals(&info.result, &BOOL));
        assert!(!info.int_div && !info.int_mod && !info.int_arith);
    }

    #[test]
    fn 文字列同士の加算はstring型になりフラグは立たない() {
        let ctx = CheckerCtx::new();
        let a = Expr::String { value: "a".into(), pos: pos() };
        let b = Expr::String { value: "b".into(), pos: pos() };
        let info = infer_binary(&ctx, TokenType::Plus, &a, &b);
        assert!(types::type_equals(&info.result, &STRING));
        assert!(!info.int_arith);
    }

    #[test]
    fn infer_callは自由関数の戻り値型を引く() {
        let mut ctx = CheckerCtx::new();
        ctx.declare_fn("add", Type::Fn { params: vec![INT, INT], ret: Box::new(INT) });
        let call_ret = infer_call(&ctx, &Expr::Ident { name: "add".into(), pos: pos() });
        assert!(types::type_equals(&call_ret, &INT));
    }

    #[test]
    fn infer_callは組み込みの戻り値型も引く() {
        let ctx = CheckerCtx::new();
        let round_call = infer_call(&ctx, &Expr::Ident { name: "round".into(), pos: pos() });
        assert!(types::type_equals(&round_call, &INT), "round() should infer as int, got {round_call:?}");
        let to_int_call = infer_call(&ctx, &Expr::Ident { name: "toInt".into(), pos: pos() });
        assert!(types::type_equals(&to_int_call, &types::union_of(vec![INT, ERROR])));
    }
}
