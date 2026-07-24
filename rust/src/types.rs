// checker(最小リゾルバ)が内部で使う「型」の表現。TS版(src/types.ts、246行)からの移植。
//
// 2026-07-17の背骨決定(union路線)により、不在は`T | none`、失敗は`T | error`のunion型で
// 表現する。汎用のnullは存在しない。
//
// **自己参照型のサポート(milestone 19)**: TS版はstructのfields/unionのmembersを
// 「後から埋める」knot-tying(オブジェクト参照の共有グラフ)で自己参照型
// (`struct Node { next: Node | none }`・自己参照する判別可能union`examples/tree.mesh`・
// `json.Value`の配列/map要素等)を表現する。Rust版は`Type::Struct.fields`/
// `Type::Union`の再帰しうる部分だけを`Rc<OnceCell<_>>`で包むことで同じ効果を得る:
// 宣言解決時に空の`Rc<OnceCell<_>>`を先にレジストリへ登録してから中身を解決し、
// 自分自身への参照は同じ(まだ空かもしれない)Rcのcloneとして返す(`checker::
// resolve_named_type`参照)。`OnceCell`を選ぶ理由は「一度だけ書き込み、以降は
// 読み取り専用」という実際の使われ方に対して`RefCell`より意図が正確で、実行時
// borrowパニックのリスクも無いため(`.fields.push`等のin-place変更は元々存在しない)。
// `Rc<T>`は`T`が`Clone`でなくても常に`Clone`(ポインタのcloneのみ)なので、`Type`
// 全体の`#[derive(Debug, Clone)]`はそのまま機能する。
//
// 自己参照が可能になったことで、TS版`typeEquals`/`typeToString`にある「比較中/
// 表示中の値を覚えておく」循環ガード(`seen`)が必須になる(以前はBox木は循環し
// 得ないため省略していた)。`Rc::ptr_eq`によるidentity比較でTS版と同じ形で移植する
// (`type_equals`は無名struct同士の比較分岐だけ、`type_to_string`は無名struct分岐・
// union分岐の両方——単一値のトラバーサルなので自己参照unionが自分自身へ構造体
// フィールド越しに直接re-entrantしうるため)。

use std::cell::OnceCell;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrimKind {
    Int,
    Float,
    String,
    Bool,
    Void,
    Error,
}

impl PrimKind {
    fn as_str(&self) -> &'static str {
        match self {
            PrimKind::Int => "int",
            PrimKind::Float => "float",
            PrimKind::String => "string",
            PrimKind::Bool => "bool",
            PrimKind::Void => "void",
            PrimKind::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Type {
    Prim(PrimKind),
    Any, // H-1: ユーザーは書けないが、空配列/mapリテラルの要素型が文脈から決まるまでの
    // 一時的なプレースホルダとしてcheckerが内部的に使う(containsAny参照)
    None,   // 「不在」を表す単位型。T | none の形でだけ現れる
    Closed, // channelがcloseされたことを表す単位型。<-ch は常に T | closed を返す
    Literal(String), // 文字列リテラル型: "active"。stringの部分型
    Array(Box<Type>),
    Chan(Box<Type>),
    Map { key: Box<Type>, value: Box<Type> }, // map<string, int>。読みはV | noneを返す
    Fn { params: Vec<Type>, ret: Box<Type> },
    // F-7: discriminant_tagは2個以上のstruct memberを持つ判別可能unionだけが持つ
    // (「全メンバーに存在しリテラル型で値が互いに異なる」フィールド名)。
    // membersと共に`UnionBody`としてknot-tying可能な形で共有する(milestone 19)
    Union { body: Rc<OnceCell<UnionBody>> },
    // ジェネリック関数(F-1後半)の宣言側でだけ現れる抽象型パラメータ
    TypeParam(String),
    // struct User { name: string }。v1の同一性判定は名前ベース(無名{...}型式が入るときに
    // 構造的比較へ拡張する)。is_error_type(F-2後半): `error type X = ...`/`error struct X {...}`で
    // 宣言されたメンバーに立つ。`?`/`or`はnone/組み込みerrorに加えてこれも失敗として伝播対象にする。
    // fieldsはknot-tying可能な形で共有する(milestone 19、struct宣言解決参照)
    Struct { name: String, fields: Rc<OnceCell<Vec<StructField>>>, is_error_type: bool },
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub type_: Type,
}

#[derive(Debug, Clone)]
pub struct UnionBody {
    pub members: Vec<Type>,
    pub discriminant_tag: Option<String>,
}

pub const INT: Type = Type::Prim(PrimKind::Int);
pub const FLOAT: Type = Type::Prim(PrimKind::Float);
pub const STRING: Type = Type::Prim(PrimKind::String);
pub const BOOL: Type = Type::Prim(PrimKind::Bool);
pub const VOID: Type = Type::Prim(PrimKind::Void);
pub const ERROR: Type = Type::Prim(PrimKind::Error);
pub const ANY: Type = Type::Any;
pub const NONE: Type = Type::None;
pub const CLOSED: Type = Type::Closed;

// 無名structの表示名(TS版の"(anonymous)"に相当)。structの名前がこの文字列のとき、
// typeToStringは名前ではなく形を展開して表示する
pub const ANONYMOUS_STRUCT_NAME: &str = "(anonymous)";

// 既に解決済みの値からstruct型を作る(非knot-tying経路)。宣言時点で自己参照する
// かもしれないstruct(`checker::resolve_named_type`)は使わず、代わりに空の
// `Rc<OnceCell::new()>`を直接登録してから後で`.set()`する
pub fn struct_ty(name: impl Into<String>, fields: Vec<StructField>, is_error_type: bool) -> Type {
    Type::Struct { name: name.into(), fields: Rc::new(OnceCell::from(fields)), is_error_type }
}

// 既に解決済みの値からunion型を作る(非knot-tying経路、struct_ty参照)
pub fn union_ty(members: Vec<Type>, discriminant_tag: Option<String>) -> Type {
    Type::Union { body: Rc::new(OnceCell::from(UnionBody { members, discriminant_tag })) }
}

pub fn type_to_string(t: &Type) -> String {
    let mut seen: Vec<*const ()> = Vec::new();
    type_to_string_impl(t, &mut seen)
}

// seen: 表示中のunion/無名struct(名前を持たず展開するしかない種類)を覚えておく「知恵の輪」
// ガード(TS版typeToStringの移植)。名前付きstructは名前だけ返して再帰しないので元々安全だが、
// 無名structが配列やmap越しに自分自身(を含むunion)を参照する自己参照判別可能unionは、
// 名前で止められず無限に展開し続けてスタックオーバーフローする。既にスタック上にある
// 型(Rcの指すアドレスで識別)へ戻ってきたらそれ以上展開せず"..."で打ち切る
fn type_to_string_impl(t: &Type, seen: &mut Vec<*const ()>) -> String {
    match t {
        Type::Prim(p) => p.as_str().to_string(),
        Type::Any => "any".to_string(),
        Type::None => "none".to_string(),
        Type::Closed => "closed".to_string(),
        Type::Literal(value) => format!("{value:?}"), // JSON.stringify相当(ダブルクォート引用)
        Type::Array(elem) => format!("{}[]", type_to_string_impl(elem, seen)),
        Type::Chan(elem) => format!("chan<{}>", type_to_string_impl(elem, seen)),
        Type::Map { key, value } => format!("map<{}, {}>", type_to_string_impl(key, seen), type_to_string_impl(value, seen)),
        Type::Fn { params, ret } => {
            let params_str = params.iter().map(|p| type_to_string_impl(p, seen)).collect::<Vec<_>>().join(", ");
            format!("fn({params_str}) {}", type_to_string_impl(ret, seen))
        }
        Type::Union { body } => {
            let ptr = Rc::as_ptr(body) as *const ();
            if seen.contains(&ptr) {
                return "...".to_string();
            }
            seen.push(ptr);
            let ub = body.get().expect("union body resolved before display");
            let s = ub.members.iter().map(|m| type_to_string_impl(m, seen)).collect::<Vec<_>>().join(" | ");
            seen.pop();
            s
        }
        Type::Struct { name, fields, .. } => {
            // 無名struct(判別可能unionのメンバー)は名前ではなく形を表示する
            if name != ANONYMOUS_STRUCT_NAME {
                return name.clone();
            }
            let ptr = Rc::as_ptr(fields) as *const ();
            if seen.contains(&ptr) {
                return "...".to_string();
            }
            seen.push(ptr);
            let fields_v = fields.get().expect("struct fields resolved before display");
            let fields_str = fields_v.iter().map(|f| format!("{}: {}", f.name, type_to_string_impl(&f.type_, seen))).collect::<Vec<_>>().join(", ");
            seen.pop();
            format!("{{ {fields_str} }}")
        }
        Type::TypeParam(name) => name.clone(),
    }
}

// struct fieldsのknot-tying用cell(Rc::ptr_eqで識別する)。type_equalsのseenガードが
// 比較中の無名struct同士のペアを覚えておくのに使う
type StructFieldsCell = Rc<OnceCell<Vec<StructField>>>;

pub fn type_equals(a: &Type, b: &Type) -> bool {
    let mut seen: Vec<(StructFieldsCell, StructFieldsCell)> = Vec::new();
    type_equals_impl(a, b, &mut seen)
}

// seen: 比較中の無名struct同士のペアを覚えておく「知恵の輪」ガード(TS版typeEqualsの移植)。
// union同士の比較は必ず無名struct(のフィールド)を経由してしか循環し得ないため、
// ここ(無名struct同士の比較分岐)にだけガードがあれば十分——union分岐自体には要らない
fn type_equals_impl(a: &Type, b: &Type, seen: &mut Vec<(StructFieldsCell, StructFieldsCell)>) -> bool {
    match (a, b) {
        (Type::Prim(pa), Type::Prim(pb)) => pa == pb,
        (Type::Any, Type::Any) | (Type::None, Type::None) | (Type::Closed, Type::Closed) => true,
        (Type::TypeParam(na), Type::TypeParam(nb)) => na == nb,
        (Type::Literal(va), Type::Literal(vb)) => va == vb,
        (Type::Array(ea), Type::Array(eb)) => type_equals_impl(ea, eb, seen),
        (Type::Chan(ea), Type::Chan(eb)) => type_equals_impl(ea, eb, seen),
        (Type::Map { key: ka, value: va }, Type::Map { key: kb, value: vb }) => type_equals_impl(ka, kb, seen) && type_equals_impl(va, vb, seen),
        (Type::Fn { params: pa, ret: ra }, Type::Fn { params: pb, ret: rb }) => {
            pa.len() == pb.len() && type_equals_impl(ra, rb, seen) && pa.iter().zip(pb.iter()).all(|(x, y)| type_equals_impl(x, y, seen))
        }
        (Type::Union { body: ba }, Type::Union { body: bb }) => {
            let uba = ba.get().expect("union body resolved before comparison");
            let ubb = bb.get().expect("union body resolved before comparison");
            uba.members.len() == ubb.members.len() && uba.members.iter().all(|m| ubb.members.iter().any(|n| type_equals_impl(m, n, seen)))
        }
        (Type::Struct { name: na, fields: fa, .. }, Type::Struct { name: nb, fields: fb, .. }) => {
            // 名前的型付け(F-3決定): 名前付きstruct同士は名前で判定する(形が同じでも
            // MetersとDollarsは別の型)。無名{...}型式が絡むときだけ構造的に比較する
            let a_anon = na == ANONYMOUS_STRUCT_NAME;
            let b_anon = nb == ANONYMOUS_STRUCT_NAME;
            if !a_anon && !b_anon {
                return na == nb;
            }
            // 同じRc(=同一のstruct宣言、自己参照で戻ってきた場合含む)なら比較するまでもなく等しい
            if Rc::ptr_eq(fa, fb) {
                return true;
            }
            if seen.iter().any(|(sa, sb)| Rc::ptr_eq(sa, fa) && Rc::ptr_eq(sb, fb)) {
                return true;
            }
            let fa_v = fa.get().expect("struct fields resolved before comparison");
            let fb_v = fb.get().expect("struct fields resolved before comparison");
            if fa_v.len() != fb_v.len() {
                return false;
            }
            seen.push((fa.clone(), fb.clone()));
            let result = fa_v.iter().all(|field_a| fb_v.iter().any(|field_b| field_b.name == field_a.name && type_equals_impl(&field_a.type_, &field_b.type_, seen)));
            seen.pop();
            result
        }
        _ => false,
    }
}

// メンバーの並びからunion型を作る(平坦化・重複除去・1個なら素の型に)。
// 自己参照するunion(まだ解決中でbodyが未setのplaceholder)がここへ渡ってくることは
// 無い設計——渡ってくる前に呼び出し元(checker::resolve_named_type)が「裸union循環」
// として検出してErrにしているはずなので、来てしまったら`.expect()`で早期に気付く
pub fn union_of(members: Vec<Type>) -> Type {
    let mut flat: Vec<Type> = Vec::new();
    for m in members {
        let candidates = match m {
            Type::Union { body } => {
                body.get()
                    .expect("union_of: nested union body read before resolution — checker bug, should have been caught by the naked-cycle check")
                    .members
                    .clone()
            }
            other => vec![other],
        };
        for c in candidates {
            if matches!(c, Type::Any) {
                return ANY;
            }
            if !flat.iter().any(|f| type_equals(f, &c)) {
                flat.push(c);
            }
        }
    }
    if flat.is_empty() {
        return VOID;
    }
    if flat.len() == 1 {
        return flat.into_iter().next().unwrap();
    }
    union_ty(flat, None)
}

// unionから条件に合うメンバーを取り除く(narrowingの中核)
pub fn union_without(t: Type, remove: impl Fn(&Type) -> bool) -> Type {
    match t {
        Type::Union { body } => {
            let members = body.get().expect("union body resolved before narrowing").members.clone();
            union_of(members.into_iter().filter(|m| !remove(m)).collect())
        }
        other => {
            if remove(&other) {
                VOID
            } else {
                other
            }
        }
    }
}

// 「失敗」を表すメンバーか(noneまたはerror)。?/orの伝播対象はこれだけ
// (closedは「入力が終わった」であって「このコードが失敗した」ではないので含めない)
pub fn is_failure(t: &Type) -> bool {
    matches!(t, Type::None) || matches!(t, Type::Prim(PrimKind::Error))
}

// fromの値をtoの場所に入れてよいか
pub fn assignable(from: &Type, to: &Type) -> bool {
    if matches!(from, Type::Any) || matches!(to, Type::Any) {
        return true;
    }
    // unionへは「どれかのメンバーに入れられればよい」
    if let Type::Union { body: to_body } = to {
        let to_members = &to_body.get().expect("union body resolved before assignability check").members;
        if let Type::Union { body: from_body } = from {
            let from_members = &from_body.get().expect("union body resolved before assignability check").members;
            return from_members.iter().all(|m| assignable(m, to));
        }
        return to_members.iter().any(|m| assignable(from, m));
    }
    // unionからは「全メンバーが入れられる場合のみ」(=事実上、絞り込みが必要)
    if let Type::Union { body: from_body } = from {
        let from_members = &from_body.get().expect("union body resolved before assignability check").members;
        return from_members.iter().all(|m| assignable(m, to));
    }
    // 配列: 要素がany側なら互換(空配列[] = any[]を型付き配列へ入れる等)。
    // 具体型同士はtype_equals(下のフォールバック)なのでint[]をstring[]には入れられない
    if let (Type::Array(from_elem), Type::Array(to_elem)) = (from, to) {
        if matches!(**from_elem, Type::Any) || matches!(**to_elem, Type::Any) {
            return true;
        }
        return type_equals(from_elem, to_elem);
    }
    // intはfloatに暗黙で広げられる(逆は不可)
    if matches!(from, Type::Prim(PrimKind::Int)) && matches!(to, Type::Prim(PrimKind::Float)) {
        return true;
    }
    // リテラル型"active"はstringの部分型(逆は不可: stringを"active"には入れられない)
    if matches!(from, Type::Literal(_)) && matches!(to, Type::Prim(PrimKind::String)) {
        return true;
    }
    type_equals(from, to)
}

// tのどこかにanyが含まれるか(配列/channel/mapのkey・value/union/関数の引数・戻り値の中まで
// 再帰。structのフィールドは常に宣言済みの具体型なので再帰不要——再帰しないぶん、
// 自己参照するstruct/unionが絡んでも無限再帰にならない)
pub fn contains_any(t: &Type) -> bool {
    match t {
        Type::Any => true,
        Type::Array(elem) | Type::Chan(elem) => contains_any(elem),
        Type::Map { key, value } => contains_any(key) || contains_any(value),
        Type::Union { body } => body.get().map(|ub| ub.members.iter().any(contains_any)).unwrap_or(false),
        Type::Fn { params, ret } => params.iter().any(contains_any) || contains_any(ret),
        _ => false,
    }
}

pub fn is_numeric(t: &Type) -> bool {
    matches!(t, Type::Any) || matches!(t, Type::Prim(PrimKind::Int) | Type::Prim(PrimKind::Float))
}

// stringとして扱えるか(string本体またはリテラル型)
pub fn is_stringy(t: &Type) -> bool {
    matches!(t, Type::Literal(_) | Type::Prim(PrimKind::String))
}

// リテラル型をstringに広げる(mut宣言・配列要素の推論で使う)
pub fn widen_literal(t: Type) -> Type {
    if matches!(t, Type::Literal(_)) {
        STRING
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 同じprim型は等しい_違うprim型は等しくない() {
        assert!(type_equals(&INT, &INT));
        assert!(!type_equals(&INT, &FLOAT));
    }

    #[test]
    fn 名前付きstructは名前だけで判定する() {
        let meters = struct_ty("Meters", vec![StructField { name: "value".into(), type_: INT }], false);
        let dollars = struct_ty("Dollars", vec![StructField { name: "value".into(), type_: INT }], false);
        // 形が同じでもMetersとDollarsは別の型(単位型の取り違えを防ぐ)
        assert!(!type_equals(&meters, &dollars));
        let meters2 = struct_ty("Meters", vec![], false);
        assert!(type_equals(&meters, &meters2)); // 名前が同じなら形は見ない
    }

    #[test]
    fn 無名structは構造的に比較する() {
        let a = struct_ty(ANONYMOUS_STRUCT_NAME, vec![StructField { name: "kind".into(), type_: Type::Literal("ok".into()) }], false);
        let b = struct_ty(ANONYMOUS_STRUCT_NAME, vec![StructField { name: "kind".into(), type_: Type::Literal("ok".into()) }], false);
        assert!(type_equals(&a, &b));
    }

    #[test]
    fn assignable_intはfloatへ暗黙で広がるが逆は不可() {
        assert!(assignable(&INT, &FLOAT));
        assert!(!assignable(&FLOAT, &INT));
    }

    #[test]
    fn assignable_リテラル型はstringの部分型() {
        assert!(assignable(&Type::Literal("active".into()), &STRING));
        assert!(!assignable(&STRING, &Type::Literal("active".into())));
    }

    #[test]
    fn assignable_anyはどちらの向きにも入る() {
        assert!(assignable(&ANY, &INT));
        assert!(assignable(&INT, &ANY));
    }

    #[test]
    fn union_ofは重複を除去し1個ならunionにしない() {
        let t = union_of(vec![INT, INT, STRING]);
        let Type::Union { body } = &t else { panic!("expected union, got {}", type_to_string(&t)) };
        assert_eq!(body.get().unwrap().members.len(), 2);

        let single = union_of(vec![INT]);
        assert!(matches!(single, Type::Prim(PrimKind::Int)));
    }

    #[test]
    fn is_failureはnoneとerrorだけtrue() {
        assert!(is_failure(&NONE));
        assert!(is_failure(&ERROR));
        assert!(!is_failure(&CLOSED));
        assert!(!is_failure(&INT));
    }

    #[test]
    fn is_numeric_is_stringy() {
        assert!(is_numeric(&INT));
        assert!(is_numeric(&FLOAT));
        assert!(!is_numeric(&STRING));
        assert!(is_stringy(&STRING));
        assert!(is_stringy(&Type::Literal("x".into())));
        assert!(!is_stringy(&INT));
    }

    #[test]
    fn widen_literalはstringに広げる() {
        assert!(matches!(widen_literal(Type::Literal("x".into())), Type::Prim(PrimKind::String)));
        assert!(matches!(widen_literal(INT), Type::Prim(PrimKind::Int)));
    }

    #[test]
    fn type_to_stringの表示() {
        assert_eq!(type_to_string(&INT), "int");
        assert_eq!(type_to_string(&Type::Array(Box::new(INT))), "int[]");
        assert_eq!(type_to_string(&Type::Map { key: Box::new(STRING), value: Box::new(INT) }), "map<string, int>");
        let u = union_of(vec![INT, NONE]);
        assert_eq!(type_to_string(&u), "int | none");
    }

    #[test]
    fn 自己参照するstructはtype_equalsが無限再帰せず自分自身と等しい() {
        // struct Node { next: Node | none } 相当。knot-tying: 空のcellを先に登録してから
        // 中身(自分自身への参照を含むunion)をセットする(resolve_named_typeの縮図)
        let cell: Rc<OnceCell<Vec<StructField>>> = Rc::new(OnceCell::new());
        let node = Type::Struct { name: "Node".into(), fields: Rc::clone(&cell), is_error_type: false };
        let next_ty = union_of(vec![node.clone(), NONE]);
        cell.set(vec![StructField { name: "next".into(), type_: next_ty }]).unwrap();

        assert!(type_equals(&node, &node));
        // 名前付きstruct同士は名前だけで判定するため、フィールドを再帰する前に短絡することも
        // 確認(別名の構造的に同じstructとは等しくない)
        let other_cell: Rc<OnceCell<Vec<StructField>>> = Rc::new(OnceCell::new());
        let other = Type::Struct { name: "Other".into(), fields: Rc::clone(&other_cell), is_error_type: false };
        let other_next = union_of(vec![other.clone(), NONE]);
        other_cell.set(vec![StructField { name: "next".into(), type_: other_next }]).unwrap();
        assert!(!type_equals(&node, &other));
    }

    #[test]
    fn 自己参照する無名structのunionはtype_equals_type_to_stringが無限再帰しない() {
        // examples/tree.meshの判別可能union相当: 無名structが自分自身を含むunionを
        // フィールド越しに参照する。無名なので比較・表示の両方が構造再帰する
        let union_cell: Rc<OnceCell<UnionBody>> = Rc::new(OnceCell::new());
        let tree = Type::Union { body: Rc::clone(&union_cell) };
        let leaf = struct_ty(ANONYMOUS_STRUCT_NAME, vec![StructField { name: "kind".into(), type_: Type::Literal("leaf".into()) }], false);
        let node = struct_ty(
            ANONYMOUS_STRUCT_NAME,
            vec![
                StructField { name: "kind".into(), type_: Type::Literal("node".into()) },
                StructField { name: "left".into(), type_: tree.clone() },
                StructField { name: "right".into(), type_: tree.clone() },
            ],
            false,
        );
        union_cell.set(UnionBody { members: vec![leaf, node], discriminant_tag: Some("kind".into()) }).unwrap();

        assert!(type_equals(&tree, &tree)); // 無限再帰せず終了することの確認そのものがテスト
        let displayed = type_to_string(&tree); // これも無限再帰せず終了すればよい("..."を含むはず)
        assert!(displayed.contains("..."), "got: {displayed}");
    }
}
