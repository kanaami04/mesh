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

use crate::ast::{Expr, TypeDecl, TypeNode};
use crate::token::TokenType;
use crate::types::{self, ANY, BOOL, ERROR, FLOAT, INT, NONE, STRING, VOID, Type};
use std::collections::{HashMap, HashSet};

// 組み込み関数。TS版`checker/context.ts`のBUILTINSをそのまま移植(特殊な検査は
// このリゾルバの対象外なので、名前の集合だけが必要)
pub const BUILTINS: &[&str] = &[
    "print", "len", "push", "str", "error", "sleep", "delete", "contains", "indexOf", "get", "keys", "values", "sort", "split", "join", "trim",
    "upper", "lower", "toInt", "toFloat", "round", "floor", "ceil", "filter", "map", "reduce", "close",
];

pub fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

// TS版`CheckerCtx`のうち、milestone 2(struct/メソッド。パッケージはまだ無し)で使う部分だけを
// 持つ。スコープスタック(narrowingは対象外)・トップレベル関数のシグネチャ表・struct型表・
// メソッド表のみ
pub struct CheckerCtx {
    scopes: Vec<HashMap<String, Type>>,
    fn_decls: HashMap<String, Type>,
    // 名前→解決済みのstruct型(resolve_struct_declsが埋める)。TS版のresolvedAliasesに相当するが、
    // knot-tying(共有可変状態)ではなく固定点反復で埋めるため、単純な所有権ベースのmapでよい
    // (ファイル冒頭のコメント参照)
    struct_types: HashMap<String, Type>,
    // struct名→メソッド名→関数型(レシーバを第1引数として含む、TS版methodTableと同じ形)
    method_table: HashMap<String, HashMap<String, Type>>,
}

impl Default for CheckerCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckerCtx {
    pub fn new() -> Self {
        CheckerCtx { scopes: vec![HashMap::new()], fn_decls: HashMap::new(), struct_types: HashMap::new(), method_table: HashMap::new() }
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

    pub fn declare_struct(&mut self, name: &str, ty: Type) {
        self.struct_types.insert(name.to_string(), ty);
    }

    pub fn lookup_struct(&self, name: &str) -> Option<&Type> {
        self.struct_types.get(name)
    }

    pub fn declare_method(&mut self, struct_name: &str, method_name: &str, ty: Type) {
        self.method_table.entry(struct_name.to_string()).or_default().insert(method_name.to_string(), ty);
    }

    pub fn lookup_method(&self, struct_name: &str, method_name: &str) -> Option<&Type> {
        self.method_table.get(struct_name)?.get(method_name)
    }
}

// 型注釈(構文)を内部表現の型へ変換。TS版`checker/types-resolve.ts`のresolveTypeのうち、
// milestone 2で必要な部分を移植。ユーザー定義のtype alias解決(knot-tying。循環検出込み)は
// 判別可能union/自己参照型を移植する段階まで先送り——今は`ctx.struct_types`(milestone 2で
// resolve_struct_declsが埋める)を引き、無ければ名前だけを覚えた空フィールドのstruct型として
// 素通しする(未宣言の型名・判別可能unionのtype alias等のフォールバック)
pub fn resolve_type_node(ctx: &CheckerCtx, node: &TypeNode) -> Type {
    match node {
        TypeNode::Union { members, .. } => types::union_of(members.iter().map(|m| resolve_type_node(ctx, m)).collect()),
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
            _ => ctx.lookup_struct(name).cloned().unwrap_or_else(|| Type::Struct { name: name.clone(), fields: vec![], is_error_type: false }),
        },
        TypeNode::Array { elem, .. } => Type::Array(Box::new(resolve_type_node(ctx, elem))),
        TypeNode::Chan { elem, .. } => Type::Chan(Box::new(resolve_type_node(ctx, elem))),
        TypeNode::MapType { key, value, .. } => {
            Type::Map { key: Box::new(resolve_type_node(ctx, key)), value: Box::new(resolve_type_node(ctx, value)) }
        }
        TypeNode::FnType { params, ret, .. } => Type::Fn {
            params: params.iter().map(|p| resolve_type_node(ctx, p)).collect(),
            ret: Box::new(ret.as_deref().map(|r| resolve_type_node(ctx, r)).unwrap_or(VOID)),
        },
        TypeNode::StructType { fields, .. } => Type::Struct {
            name: types::ANONYMOUS_STRUCT_NAME.to_string(),
            fields: fields.iter().map(|f| types::StructField { name: f.name.clone(), type_: resolve_type_node(ctx, &f.type_node) }).collect(),
            is_error_type: false,
        },
    }
}

pub fn resolve_return_type(ctx: &CheckerCtx, ret: &Option<TypeNode>) -> Type {
    ret.as_ref().map(|r| resolve_type_node(ctx, r)).unwrap_or(VOID)
}

// トップレベルのstruct宣言をすべて`ctx.struct_types`へ解決する。TS版はASTを直接書き換える
// knot-tyingで自己参照型を表現するが、Rustの所有権ベースの木ではそのパターンに向かない
// (types.rs冒頭のコメント参照)。代わりに**固定点反復**で解決する: 現時点のレジストリを使って
// 全struct宣言のfieldsを繰り返し再解決し、`types.len()`回のパスで非循環(DAG)なら宣言順に
// 関係なく必ず収束する。ただし循環(自己参照含む)は固定点反復では「クラッシュはしないが
// 深さが毎パス線形に伸びる中途半端な入れ子」になってしまい、「自己参照は未対応」という
// 前提を静かに裏切ってしまうため、固定点反復の前に生のTypeNode参照関係だけを見た軽量な
// DFSサイクル検出を挟み、循環があれば明確なErrを返す(codegenの「まだ対応していません」と
// 同じ精神——診断ではなく、対応していない構造を正直に伝える)
pub fn resolve_struct_decls(ctx: &mut CheckerCtx, types: &[TypeDecl]) -> Result<(), String> {
    // code review(milestone 3で発覚): 以前は`!t.is_error`も条件に含めていたため、
    // `error struct X {...}`宣言がここで丸ごと無視され、is_error_typeタグが一切効かない
    // バグになっていた。error structもここで解決し(下のstruct構築コードが既に
    // `is_error_type: decl.is_error`を渡しているので、それ以外の変更は不要)、
    // json structだけを引き続き対象外にする(decode<X>自動生成はモジュールmilestoneまで先送り)
    let struct_decls: Vec<&TypeDecl> = types.iter().filter(|t| !t.is_json && matches!(t.node, TypeNode::StructType { .. })).collect();
    let names: HashSet<&str> = struct_decls.iter().map(|t| t.name.as_str()).collect();

    if let Some(cycle_name) = find_struct_cycle(&struct_decls, &names) {
        return Err(format!("checker: self-referential/cyclic struct definitions are not yet supported (found via '{cycle_name}')"));
    }

    // 固定点反復: 依存先が(宣言順に関係なく)先に解決されているかどうかに関わらず、
    // 現在のレジストリの中身で全宣言を素朴に再解決するのをN回繰り返す。非循環である
    // ことは上のサイクル検出で保証済みなので、依存の深さはtypes.len()を超えない
    for _ in 0..struct_decls.len().max(1) {
        for decl in &struct_decls {
            let TypeNode::StructType { fields, .. } = &decl.node else { continue };
            let resolved_fields =
                fields.iter().map(|f| types::StructField { name: f.name.clone(), type_: resolve_type_node(ctx, &f.type_node) }).collect();
            ctx.declare_struct(&decl.name, Type::Struct { name: decl.name.clone(), fields: resolved_fields, is_error_type: decl.is_error });
        }
    }
    Ok(())
}

// struct宣言同士の直接参照関係(fieldsに現れる型名)を有向グラフとして辿り、循環を検出する。
// Array/Chan/MapType/Union/FnTypeの中も再帰的に見る(例: `children: Node[]`も
// `Node`への依存として数える——配列越しでも固定点反復の収束が壊れる点は同じため)
fn find_struct_cycle(struct_decls: &[&TypeDecl], names: &HashSet<&str>) -> Option<String> {
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for decl in struct_decls {
        let TypeNode::StructType { fields, .. } = &decl.node else { continue };
        let mut referenced = Vec::new();
        for f in fields {
            collect_referenced_names(&f.type_node, &mut referenced);
        }
        deps.insert(decl.name.clone(), referenced.into_iter().filter(|n| names.contains(n.as_str())).collect());
    }

    let mut visiting: HashSet<String> = HashSet::new();
    let mut done: HashSet<String> = HashSet::new();
    for decl in struct_decls {
        if visit_for_cycle(&decl.name, &deps, &mut visiting, &mut done) {
            return Some(decl.name.clone());
        }
    }
    None
}

fn visit_for_cycle(name: &str, deps: &HashMap<String, Vec<String>>, visiting: &mut HashSet<String>, done: &mut HashSet<String>) -> bool {
    if done.contains(name) {
        return false;
    }
    if !visiting.insert(name.to_string()) {
        return true; // 現在たどっている経路上に再び現れた = 循環
    }
    if let Some(refs) = deps.get(name) {
        for r in refs {
            if visit_for_cycle(r, deps, visiting, done) {
                return true;
            }
        }
    }
    visiting.remove(name);
    done.insert(name.to_string());
    false
}

fn collect_referenced_names(node: &TypeNode, out: &mut Vec<String>) {
    match node {
        TypeNode::Name { name, .. } => out.push(name.clone()),
        TypeNode::Literal { .. } => {}
        TypeNode::Union { members, .. } => members.iter().for_each(|m| collect_referenced_names(m, out)),
        TypeNode::Array { elem, .. } | TypeNode::Chan { elem, .. } => collect_referenced_names(elem, out),
        TypeNode::MapType { key, value, .. } => {
            collect_referenced_names(key, out);
            collect_referenced_names(value, out);
        }
        TypeNode::FnType { params, ret, .. } => {
            params.iter().for_each(|p| collect_referenced_names(p, out));
            if let Some(r) = ret {
                collect_referenced_names(r, out);
            }
        }
        TypeNode::StructType { fields, .. } => fields.iter().for_each(|f| collect_referenced_names(&f.type_node, out)),
    }
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
        // パッケージ修飾(pkg: Some)はパッケージ/モジュールのmilestoneまで対象外——名前だけで
        // struct_typesを引く(見つからなければ従来通り殻)
        Expr::StructLit { name, .. } => {
            ctx.lookup_struct(name).cloned().unwrap_or_else(|| Type::Struct { name: name.clone(), fields: vec![], is_error_type: false })
        }
        // フィールドアクセス。targetがstruct型でnameが宣言済みフィールドならその型を返す。
        // メソッド名(フィールドではない名前)はここでは解決しない——裸のメンバー値として
        // メソッドを参照する式はcodegen側でも対象外(TS版と同じくcall式側だけで判別する)
        Expr::Member { target, name, .. } => match infer_expr(ctx, target) {
            Type::Struct { fields, .. } => fields.into_iter().find(|f| &f.name == name).map(|f| f.type_).unwrap_or(ANY),
            _ => ANY,
        },
        // `?`/`or`はどちらも「失敗メンバーを取り除いた残り」が結果の型になる(TS版と同じ式。
        // contextやright/bindingの中身は結果型に影響しない)
        Expr::Prop { operand, .. } => types::union_without(infer_expr(ctx, operand), is_failure_type),
        Expr::OrElse { left, .. } => types::union_without(infer_expr(ctx, left), is_failure_type),
        // M3未対応の式(map/channel/並行処理等)はANYへ最善努力でフォールバックする。
        // codegen側がこれらの構文自体を「まだ対応していません」と明確なエラーにするので、
        // ここで型を誤魔化しても実害は無い
        _ => ANY,
    }
}

// 「失敗」メンバーか(none/errorに加えて、error type/error structでタグ付けされたstructも
// 含める)。TS版`checker/types-resolve.ts`のisFailureTypeを移植——types.rsのis_failureは
// none/errorのみを見るプリミティブな判定なので、structのタグまで見る拡張はここに置く
pub fn is_failure_type(t: &Type) -> bool {
    types::is_failure(t) || matches!(t, Type::Struct { is_error_type: true, .. })
}

// `f() or e => fallback`のeの型。TS版expressions.tsのorElse検査を忠実に移植:
// **unionでない被演算子は無条件でANYになる**(TS版の実際の挙動——bareの失敗型はそもそも
// 「or-never-fails」等の診断で弾かれる想定のため、union以外のケースをわざわざ賢く
// 扱う実装にはなっていない。診断を出さないRust版でこの経路に来た場合も同じ挙動にする)
pub fn or_binding_type(t: &Type) -> Type {
    match t {
        Type::Union { members, .. } => {
            let failures: Vec<Type> = members.iter().filter(|m| is_failure_type(m)).cloned().collect();
            if failures.is_empty() { ANY } else { types::union_of(failures) }
        }
        _ => ANY,
    }
}

// codegen側だけで使う安全ガード用: 構造化error(error type/error structでタグ付けされた
// struct)がtの中に(unionの中も含めて再帰的に)含まれるか。TS版の対応する診断
// (prop-context-structured-error)はunion内のケースしか見ないが、こちらは意図的に
// それより広く——bare(union化されていない)構造化errorも拾う。理由: ランタイムの
// `__propCtx`は`null`/`instanceof Error`しか特別扱いせず、`__ERR`タグ付きの構造化errorは
// 素通りして「成功扱い」になってしまう(runtime.ts参照)。TS版はこの組み合わせ自体を
// 型検査の時点で弾くので実害が無いが、診断を出さないこのリゾルバではここで拾わないと
// 実行時に静かに壊れた挙動になる
pub fn has_structured_failure(t: &Type) -> bool {
    match t {
        Type::Union { members, .. } => members.iter().any(has_structured_failure),
        Type::Struct { is_error_type: true, .. } => true,
        _ => false,
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
    // メソッド呼び出し: recv.method(args)。TS版calls.tsと同じ「フィールドが勝つ」順序——
    // targetがstruct型でnameが宣言済みフィールドでなければメソッドとして解決する
    if let Expr::Member { target, name, .. } = callee
        && let Type::Struct { fields, name: struct_name, .. } = infer_expr(ctx, target)
        && !fields.iter().any(|f| &f.name == name)
        && let Some(Type::Fn { ret, .. }) = ctx.lookup_method(&struct_name, name)
    {
        return (**ret).clone();
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
        let ctx = CheckerCtx::new();
        let node = TypeNode::Name { name: "int".into(), pkg: None, pos: pos() };
        assert!(matches!(resolve_type_node(&ctx, &node), Type::Prim(_)));
        assert!(types::type_equals(&resolve_type_node(&ctx, &node), &INT));
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

    use crate::ast::StructFieldNode;

    fn name_type(n: &str) -> TypeNode {
        TypeNode::Name { name: n.to_string(), pkg: None, pos: pos() }
    }

    fn struct_decl(name: &str, fields: &[(&str, TypeNode)]) -> TypeDecl {
        TypeDecl {
            name: name.to_string(),
            node: TypeNode::StructType {
                fields: fields.iter().map(|(fname, ft)| StructFieldNode { name: fname.to_string(), type_node: ft.clone(), pos: pos() }).collect(),
                pos: pos(),
            },
            exported: false,
            is_error: false,
            is_json: false,
            pos: pos(),
        }
    }

    #[test]
    fn resolve_struct_declsは前方参照でも解決できる() {
        // LineがPointより先に宣言されているが、固定点反復により正しく解決できる
        let types = vec![
            struct_decl("Line", &[("start", name_type("Point")), ("end", name_type("Point"))]),
            struct_decl("Point", &[("x", name_type("int")), ("y", name_type("int"))]),
        ];
        let mut ctx = CheckerCtx::new();
        resolve_struct_decls(&mut ctx, &types).unwrap();
        let Some(Type::Struct { fields, .. }) = ctx.lookup_struct("Line").cloned() else { panic!("expected struct") };
        let Type::Struct { fields: point_fields, name, .. } = &fields[0].type_ else { panic!("expected resolved Point field") };
        assert_eq!(name, "Point");
        assert_eq!(point_fields.len(), 2);
    }

    #[test]
    fn resolve_struct_declsは相互循環structを検出してerrを返す() {
        let types = vec![struct_decl("A", &[("b", name_type("B"))]), struct_decl("B", &[("a", name_type("A"))])];
        let mut ctx = CheckerCtx::new();
        assert!(resolve_struct_decls(&mut ctx, &types).is_err());
    }

    #[test]
    fn resolve_struct_declsは自己参照structも検出してerrを返す() {
        let types = vec![struct_decl("Node", &[("next", name_type("Node"))])];
        let mut ctx = CheckerCtx::new();
        assert!(resolve_struct_decls(&mut ctx, &types).is_err());
    }

    #[test]
    fn infer_exprはstruct_litとフィールドアクセスの型を解決する() {
        let types = vec![struct_decl("User", &[("name", name_type("string")), ("age", name_type("int"))])];
        let mut ctx = CheckerCtx::new();
        resolve_struct_decls(&mut ctx, &types).unwrap();
        let lit = Expr::StructLit { name: "User".into(), pkg: None, fields: vec![], pos: pos() };
        let lit_ty = infer_expr(&ctx, &lit);
        assert!(matches!(&lit_ty, Type::Struct { name, .. } if name == "User"));
        let member = Expr::Member { target: Box::new(lit), name: "age".into(), pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &member), &INT));
    }

    #[test]
    fn infer_callはメソッドの戻り値型を引き_同名フィールドがあれば勝つ() {
        let types = vec![struct_decl("User", &[("name", name_type("string"))])];
        let mut ctx = CheckerCtx::new();
        resolve_struct_decls(&mut ctx, &types).unwrap();
        let user_ty = ctx.lookup_struct("User").unwrap().clone();
        ctx.declare_method("User", "describe", Type::Fn { params: vec![user_ty], ret: Box::new(STRING) });
        let recv = Expr::StructLit { name: "User".into(), pkg: None, fields: vec![], pos: pos() };
        let call = Expr::Member { target: Box::new(recv), name: "describe".into(), pos: pos() };
        assert!(types::type_equals(&infer_call(&ctx, &call), &STRING));
    }

    fn error_struct_decl(name: &str, fields: &[(&str, TypeNode)]) -> TypeDecl {
        let mut decl = struct_decl(name, fields);
        decl.is_error = true;
        decl
    }

    #[test]
    fn is_failure_typeはnone_error_タグ付きstructでtrue() {
        let tagged = Type::Struct { name: "NotFound".into(), fields: vec![], is_error_type: true };
        let plain = Type::Struct { name: "User".into(), fields: vec![], is_error_type: false };
        assert!(is_failure_type(&NONE));
        assert!(is_failure_type(&ERROR));
        assert!(is_failure_type(&tagged));
        assert!(!is_failure_type(&plain));
        assert!(!is_failure_type(&INT));
    }

    #[test]
    fn resolve_struct_declsはerror_structをis_error_typeとして解決する() {
        // code review(milestone 3で発覚): 以前はerror struct宣言自体が無視されていたバグ
        let types = vec![error_struct_decl("NotFound", &[("message", name_type("string"))])];
        let mut ctx = CheckerCtx::new();
        resolve_struct_decls(&mut ctx, &types).unwrap();
        let Some(Type::Struct { is_error_type, .. }) = ctx.lookup_struct("NotFound") else { panic!("expected struct") };
        assert!(*is_error_type);
    }

    #[test]
    fn infer_exprのprop_orelseはerror_struct込みのunionでも成功メンバーだけを返す() {
        let types = vec![error_struct_decl("NotFound", &[])];
        let mut ctx = CheckerCtx::new();
        resolve_struct_decls(&mut ctx, &types).unwrap();
        let not_found = ctx.lookup_struct("NotFound").unwrap().clone();

        let plain_union = types::union_of(vec![INT, ERROR]);
        let struct_union = types::union_of(vec![INT, not_found.clone()]);
        let make_ident = |ty: Type| {
            let mut c = CheckerCtx::new();
            c.declare("x", ty);
            (c, Expr::Ident { name: "x".into(), pos: pos() })
        };

        let (c1, x1) = make_ident(plain_union);
        let prop = Expr::Prop { operand: Box::new(x1.clone()), context: None, pos: pos() };
        assert!(types::type_equals(&infer_expr(&c1, &prop), &INT));
        let or_else = Expr::OrElse { left: Box::new(x1), right: Box::new(int_lit("0")), binding: None, pos: pos() };
        assert!(types::type_equals(&infer_expr(&c1, &or_else), &INT));

        let (c2, x2) = make_ident(struct_union);
        let prop2 = Expr::Prop { operand: Box::new(x2), context: None, pos: pos() };
        assert!(types::type_equals(&infer_expr(&c2, &prop2), &INT));
    }

    #[test]
    fn or_binding_typeはunion内の失敗メンバーを返しunion以外は常にany() {
        let with_error = types::union_of(vec![INT, ERROR]);
        assert!(types::type_equals(&or_binding_type(&with_error), &ERROR));
        let no_failure = types::union_of(vec![INT, STRING]);
        assert!(matches!(or_binding_type(&no_failure), Type::Any));
        // TS版の実際の挙動を忠実に踏襲: unionでない被演算子は常にANY(bareのERRORでも)
        assert!(matches!(or_binding_type(&ERROR), Type::Any));
    }

    #[test]
    fn has_structured_failureはstructのerrorタグを再帰的に検出する() {
        let tagged = Type::Struct { name: "NotFound".into(), fields: vec![], is_error_type: true };
        assert!(has_structured_failure(&tagged));
        assert!(has_structured_failure(&types::union_of(vec![INT, tagged.clone()])));
        // union_ofは平坦化するため、genuinely入れ子のUnionを直接組んで再帰を検証する
        let inner = Type::Union { members: vec![INT, tagged], discriminant_tag: None };
        let nested = Type::Union { members: vec![STRING, inner], discriminant_tag: None };
        assert!(has_structured_failure(&nested));
        assert!(!has_structured_failure(&types::union_of(vec![INT, ERROR])));
        assert!(!has_structured_failure(&types::union_of(vec![INT, NONE])));
    }
}
