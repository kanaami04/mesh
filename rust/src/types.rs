// checker(最小リゾルバ)が内部で使う「型」の表現。TS版(src/types.ts、246行)からの移植。
//
// 2026-07-17の背骨決定(union路線)により、不在は`T | none`、失敗は`T | error`のunion型で
// 表現する。汎用のnullは存在しない。
//
// **Rust版の意図的な簡略化(自己参照型は現時点で非対応)**: TS版はstructのfields/unionの
// membersを「後から埋める」knot-tying(オブジェクト参照の共有グラフ)で自己参照型
// (`struct Node { left: Node, right: Node }`・`json.Value`の配列/map要素等)を表現できる。
// Rust版は`Box<Type>`による所有権ベースの木構造にしているため、値が自分自身を含む
// 真の循環をそもそも構築できない(Rustの所有権モデルでは不可能)。struct/自己参照型を
// 移植する段階(checker milestone 2予定)で、`Rc<RefCell<Type>>`かarena+インデックス方式への
// 作り直しが必要になる。今回のマイルストーン(struct無し)ではこの問題は発生しないため、
// 先送りにしている。これに伴い、TS版`typeEquals`にある「比較中のペアを覚えておく」
// 循環ガード(`seen`)も今は省略している(Box木は循環し得ないため不要)

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
    // (「全メンバーに存在しリテラル型で値が互いに異なる」フィールド名)
    Union { members: Vec<Type>, discriminant_tag: Option<String> },
    // ジェネリック関数(F-1後半)の宣言側でだけ現れる抽象型パラメータ
    TypeParam(String),
    // struct User { name: string }。v1の同一性判定は名前ベース(無名{...}型式が入るときに
    // 構造的比較へ拡張する)。is_error_type(F-2後半): `error type X = ...`/`error struct X {...}`で
    // 宣言されたメンバーに立つ。`?`/`or`はnone/組み込みerrorに加えてこれも失敗として伝播対象にする
    Struct { name: String, fields: Vec<StructField>, is_error_type: bool },
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub type_: Type,
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

pub fn type_to_string(t: &Type) -> String {
    match t {
        Type::Prim(p) => p.as_str().to_string(),
        Type::Any => "any".to_string(),
        Type::None => "none".to_string(),
        Type::Closed => "closed".to_string(),
        Type::Literal(value) => format!("{value:?}"), // JSON.stringify相当(ダブルクォート引用)
        Type::Array(elem) => format!("{}[]", type_to_string(elem)),
        Type::Chan(elem) => format!("chan<{}>", type_to_string(elem)),
        Type::Map { key, value } => format!("map<{}, {}>", type_to_string(key), type_to_string(value)),
        Type::Fn { params, ret } => {
            let params_str = params.iter().map(type_to_string).collect::<Vec<_>>().join(", ");
            format!("fn({params_str}) {}", type_to_string(ret))
        }
        Type::Union { members, .. } => members.iter().map(type_to_string).collect::<Vec<_>>().join(" | "),
        Type::Struct { name, fields, .. } => {
            // 無名struct(判別可能unionのメンバー)は名前ではなく形を表示する
            if name != ANONYMOUS_STRUCT_NAME {
                return name.clone();
            }
            let fields_str = fields.iter().map(|f| format!("{}: {}", f.name, type_to_string(&f.type_))).collect::<Vec<_>>().join(", ");
            format!("{{ {fields_str} }}")
        }
        Type::TypeParam(name) => name.clone(),
    }
}

// Box木は循環し得ないため、TS版の「比較中のペアを覚えておく」循環ガードは不要
// (ファイル冒頭のコメント参照)
pub fn type_equals(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Prim(pa), Type::Prim(pb)) => pa == pb,
        (Type::Any, Type::Any) | (Type::None, Type::None) | (Type::Closed, Type::Closed) => true,
        (Type::TypeParam(na), Type::TypeParam(nb)) => na == nb,
        (Type::Literal(va), Type::Literal(vb)) => va == vb,
        (Type::Array(ea), Type::Array(eb)) => type_equals(ea, eb),
        (Type::Chan(ea), Type::Chan(eb)) => type_equals(ea, eb),
        (Type::Map { key: ka, value: va }, Type::Map { key: kb, value: vb }) => type_equals(ka, kb) && type_equals(va, vb),
        (Type::Fn { params: pa, ret: ra }, Type::Fn { params: pb, ret: rb }) => {
            pa.len() == pb.len() && type_equals(ra, rb) && pa.iter().zip(pb.iter()).all(|(x, y)| type_equals(x, y))
        }
        (Type::Union { members: ma, .. }, Type::Union { members: mb, .. }) => {
            ma.len() == mb.len() && ma.iter().all(|m| mb.iter().any(|n| type_equals(m, n)))
        }
        (Type::Struct { name: na, fields: fa, .. }, Type::Struct { name: nb, fields: fb, .. }) => {
            // 名前的型付け(F-3決定): 名前付きstruct同士は名前で判定する(形が同じでも
            // MetersとDollarsは別の型)。無名{...}型式が絡むときだけ構造的に比較する
            let a_anon = na == ANONYMOUS_STRUCT_NAME;
            let b_anon = nb == ANONYMOUS_STRUCT_NAME;
            if !a_anon && !b_anon {
                return na == nb;
            }
            fa.len() == fb.len()
                && fa.iter().all(|field_a| fb.iter().any(|field_b| field_b.name == field_a.name && type_equals(&field_a.type_, &field_b.type_)))
        }
        _ => false,
    }
}

// メンバーの並びからunion型を作る(平坦化・重複除去・1個なら素の型に)
pub fn union_of(members: Vec<Type>) -> Type {
    let mut flat: Vec<Type> = Vec::new();
    for m in members {
        let candidates = match m {
            Type::Union { members, .. } => members,
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
    Type::Union { members: flat, discriminant_tag: None }
}

// unionから条件に合うメンバーを取り除く(narrowingの中核)
pub fn union_without(t: Type, remove: impl Fn(&Type) -> bool) -> Type {
    match t {
        Type::Union { members, .. } => union_of(members.into_iter().filter(|m| !remove(m)).collect()),
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
    if let Type::Union { members: to_members, .. } = to {
        if let Type::Union { members: from_members, .. } = from {
            return from_members.iter().all(|m| assignable(m, to));
        }
        return to_members.iter().any(|m| assignable(from, m));
    }
    // unionからは「全メンバーが入れられる場合のみ」(=事実上、絞り込みが必要)
    if let Type::Union { members: from_members, .. } = from {
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
// 再帰。structのフィールドは常に宣言済みの具体型なので再帰不要)
pub fn contains_any(t: &Type) -> bool {
    match t {
        Type::Any => true,
        Type::Array(elem) | Type::Chan(elem) => contains_any(elem),
        Type::Map { key, value } => contains_any(key) || contains_any(value),
        Type::Union { members, .. } => members.iter().any(contains_any),
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
        let meters = Type::Struct { name: "Meters".into(), fields: vec![StructField { name: "value".into(), type_: INT }], is_error_type: false };
        let dollars = Type::Struct { name: "Dollars".into(), fields: vec![StructField { name: "value".into(), type_: INT }], is_error_type: false };
        // 形が同じでもMetersとDollarsは別の型(単位型の取り違えを防ぐ)
        assert!(!type_equals(&meters, &dollars));
        let meters2 = Type::Struct { name: "Meters".into(), fields: vec![], is_error_type: false };
        assert!(type_equals(&meters, &meters2)); // 名前が同じなら形は見ない
    }

    #[test]
    fn 無名structは構造的に比較する() {
        let a = Type::Struct {
            name: ANONYMOUS_STRUCT_NAME.into(),
            fields: vec![StructField { name: "kind".into(), type_: Type::Literal("ok".into()) }],
            is_error_type: false,
        };
        let b = Type::Struct {
            name: ANONYMOUS_STRUCT_NAME.into(),
            fields: vec![StructField { name: "kind".into(), type_: Type::Literal("ok".into()) }],
            is_error_type: false,
        };
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
        let Type::Union { members, .. } = &t else { panic!("expected union, got {t:?}") };
        assert_eq!(members.len(), 2);

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
}
