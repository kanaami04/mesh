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

use crate::ast::{Expr, MatchArm, MatchPattern, TypeDecl, TypeNode};
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

// パッケージがexportする名前→型の表。TS版`PackageSymbols`(types/fns/consts)に相当。
// レジストリ(下記)のキーはパッケージ名(importエイリアス)、この中のキーはパッケージ内の
// 素の(pkg修飾されていない)宣言名——他パッケージから`alias.Name`で引くときにこの2段で引く
#[derive(Clone, Default)]
pub struct PackageSymbols {
    pub types: HashMap<String, Type>,
    pub fns: HashMap<String, Type>,
    pub consts: HashMap<String, Type>,
}

// TS版`CheckerCtx`のうち、milestone 2(struct/メソッド。パッケージはまだ無し)で使う部分だけを
// 持つ。スコープスタック(narrowingは対象外)・トップレベル関数のシグネチャ表・struct型表・
// メソッド表のみ。Cloneはselectのアーム推論(infer_expr参照)が使い捨てのスクラッチctxを
// 作るために必要——infer_exprは&CheckerCtx(不変参照)しか受け取らないため
#[derive(Clone)]
pub struct CheckerCtx {
    scopes: Vec<HashMap<String, Type>>,
    // 以下4つは「現在処理中のパッケージ」ぶんだけを持つフラット名前空間——codegen側が
    // パッケージを切り替えるたびbegin_packageでリセットする(milestone 6・複数パッケージ対応)
    fn_decls: HashMap<String, Type>,
    // 名前→解決済みのstruct型(resolve_type_declsが埋める)。TS版のresolvedAliasesに相当するが、
    // knot-tying(共有可変状態)ではなく固定点反復で埋めるため、単純な所有権ベースのmapでよい
    // (ファイル冒頭のコメント参照)。キーは常に素の(pkg修飾されていない)名前——ただし
    // 値のType::Struct.name自体はpkg=="main"以外ならpkg修飾済み(qualify_struct_name参照)
    struct_types: HashMap<String, Type>,
    // 名前→解決済みのunion型(`type X = A | B`、milestone 7・判別可能union対応)。
    // struct_typesと並ぶ姉妹テーブル——resolve_type_declsが同じ固定点反復の中で埋める
    union_types: HashMap<String, Type>,
    pkg: String,                     // 現在処理中パッケージ名("main"かimportエイリアス名)
    import_aliases: HashSet<String>, // 現在処理中パッケージのimportエイリアス集合
    // struct名(既にpkg修飾済み)→メソッド名→関数型。全パッケージ共有——struct名が
    // pkg修飾済みなので衝突しない(TS版sharedMethodsと同じ、begin_packageでもリセットしない)
    method_table: HashMap<String, HashMap<String, Type>>,
    // 処理済みパッケージのexportedシンボル。パッケージ名→PackageSymbols。全パッケージ共有
    // (begin_packageでリセットしない)——パッケージは依存順に処理されるので、あるパッケージを
    // 処理する時点で依存先はここに登録済み
    registry: HashMap<String, PackageSymbols>,
}

impl Default for CheckerCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckerCtx {
    pub fn new() -> Self {
        CheckerCtx {
            scopes: vec![HashMap::new()],
            fn_decls: HashMap::new(),
            struct_types: HashMap::new(),
            union_types: HashMap::new(),
            pkg: "main".to_string(),
            import_aliases: HashSet::new(),
            method_table: HashMap::new(),
            registry: HashMap::new(),
        }
    }

    // 新しいパッケージの処理を始める前に呼ぶ(codegen::generate_packageが全パッケージに
    // ついて——単一パッケージ"main"のみのコンパイルでも1回——必ず呼ぶ。newの初期値
    // 〈pkg="main"、import_aliases/fn_decls/struct_types空〉に対して呼んでも実質no-opなので、
    // 既存の全milestoneの単一ファイル挙動とは完全互換のまま)。fn_decls/struct_typesは
    // パッケージごとのフラット名前空間なのでリセットするが、method_table/registryは
    // 全パッケージ共有なので触らない
    pub fn begin_package(&mut self, pkg: &str, import_aliases: HashSet<String>) {
        self.pkg = pkg.to_string();
        self.import_aliases = import_aliases;
        self.fn_decls.clear();
        self.struct_types.clear();
        self.union_types.clear();
        self.scopes = vec![HashMap::new()];
    }

    pub fn pkg(&self) -> &str {
        &self.pkg
    }

    // targetがローカルスコープでshadowされていない既知のimportエイリアスかどうか
    // (TS版tryPackageMemberと同じ優先順位——ローカル変数が勝つ)
    pub fn is_package_alias(&self, name: &str) -> bool {
        self.lookup(name).is_none() && self.import_aliases.contains(name)
    }

    pub fn register_package(&mut self, pkg: &str, symbols: PackageSymbols) {
        self.registry.insert(pkg.to_string(), symbols);
    }

    pub fn lookup_package_type(&self, pkg: &str, name: &str) -> Option<&Type> {
        self.registry.get(pkg)?.types.get(name)
    }

    pub fn lookup_package_fn(&self, pkg: &str, name: &str) -> Option<&Type> {
        self.registry.get(pkg)?.fns.get(name)
    }

    pub fn lookup_package_const(&self, pkg: &str, name: &str) -> Option<&Type> {
        self.registry.get(pkg)?.consts.get(name)
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

    pub fn declare_union(&mut self, name: &str, ty: Type) {
        self.union_types.insert(name.to_string(), ty);
    }

    pub fn lookup_union(&self, name: &str) -> Option<&Type> {
        self.union_types.get(name)
    }

    pub fn declare_method(&mut self, struct_name: &str, method_name: &str, ty: Type) {
        self.method_table.entry(struct_name.to_string()).or_default().insert(method_name.to_string(), ty);
    }

    pub fn lookup_method(&self, struct_name: &str, method_name: &str) -> Option<&Type> {
        self.method_table.get(struct_name)?.get(method_name)
    }
}

// 型注釈(構文)を内部表現の型へ変換。TS版`checker/types-resolve.ts`のresolveTypeのうち、
// このRust移植で必要な部分を移植。ユーザー定義のtype alias解決(knot-tying。循環検出込み)は
// 自己参照型(milestone 2の自己参照struct・milestone 7の自己参照判別可能union、共に
// 明確なErrで対象外)を除き`ctx.struct_types`/`ctx.union_types`(resolve_type_declsが
// 埋める)を引き、無ければ名前だけを覚えた空フィールドのstruct型として素通しする
// (未宣言の型名のフォールバック)
pub fn resolve_type_node(ctx: &CheckerCtx, node: &TypeNode) -> Type {
    match node {
        TypeNode::Union { members, .. } => types::union_of(members.iter().map(|m| resolve_type_node(ctx, m)).collect()),
        TypeNode::Literal { value, .. } => Type::Literal(value.clone()),
        // pkg修飾された型注釈(`mathutil.Point`)はパッケージのレジストリから引く
        // (milestone 6・複数パッケージ対応)。未import/未exportなら(診断は出さない設計
        // なので)pkg修飾済みの空フィールドstructへフォールバック——codegen側が実際に
        // このstructを使おうとしたところで明確なErrになる。
        // code review指摘: `alias`が実際にこのパッケージのimport_aliasesに含まれるか
        // (is_package_alias)を確認せずにレジストリを直接引いていたため、`import`を
        // 宣言していない他パッケージの型でも(たまたま別経路でロードされてさえいれば)
        // 解決できてしまっていた——「パッケージ間でのstruct循環は構造的に起こり得ない」
        // という前提(circular importが依存グラフの循環検出で必ず弾かれる、という設計)は
        // 依存グラフがimport文だけを見て構築されることに依存しているため、import宣言を
        // 経由しない参照が許されるとこの前提が崩れる。infer_callの自由関数呼び出しは
        // 既に is_package_alias でこれを確認しているので、型/struct literal側も揃える
        TypeNode::Name { name, pkg: Some(alias), .. } => {
            if ctx.is_package_alias(alias) { ctx.lookup_package_type(alias, name).cloned() } else { None }
                .unwrap_or_else(|| Type::Struct { name: format!("{alias}.{name}"), fields: vec![], is_error_type: false })
        }
        TypeNode::Name { name, pkg: None, .. } => match name.as_str() {
            "int" => INT,
            "float" => FLOAT,
            "string" => STRING,
            "bool" => BOOL,
            "void" => VOID,
            "error" => ERROR,
            "none" => NONE,
            "closed" => types::CLOSED,
            // union型alias(`type Status = "active" | "banned"`、milestone 7)はstruct_typesに
            // 無ければunion_typesも試す。どちらにも無ければ従来通り殻structへフォールバック
            _ => ctx
                .lookup_struct(name)
                .or_else(|| ctx.lookup_union(name))
                .cloned()
                .unwrap_or_else(|| Type::Struct { name: name.clone(), fields: vec![], is_error_type: false }),
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

// トップレベルのtype宣言(struct・union型alias)をすべて`ctx.struct_types`/`ctx.union_types`
// へ解決する。TS版はASTを直接書き換えるknot-tyingで自己参照型を表現するが、Rustの
// 所有権ベースの木ではそのパターンに向かない(types.rs冒頭のコメント参照)。代わりに
// **固定点反復**で解決する: 現時点のレジストリを使って全宣言のfields/membersを繰り返し
// 再解決し、`types.len()`回のパスで非循環(DAG)なら宣言順に関係なく必ず収束する。
// ただし循環(自己参照含む)は固定点反復では「クラッシュはしないが深さが毎パス線形に
// 伸びる中途半端な入れ子」になってしまい、「自己参照は未対応」という前提を静かに
// 裏切ってしまうため、固定点反復の前に生のTypeNode参照関係だけを見た軽量なDFSサイクル
// 検出を挟み、循環があれば明確なErrを返す(codegenの「まだ対応していません」と同じ
// 精神——診断ではなく、対応していない構造を正直に伝える)。struct宣言とunion型alias宣言
// (milestone 7・判別可能union対応)は同じ依存グラフの中で扱う——一方が他方を参照しうる
// ため(例: unionのメンバーが名前付きstructを参照する、structのフィールドがunion型
// aliasを参照する)。**自己参照する判別可能union(`examples/tree.mesh`)はこの循環検出で
// 明確なErrになり対象外のまま**——無名structの構造的比較(ANONYMOUS_STRUCT_NAME)では
// 自己参照を安全に表現できないため、milestone 2の自己参照structと同じ理由の意図的な
// スコープ縮小
pub fn resolve_type_decls(ctx: &mut CheckerCtx, types: &[TypeDecl]) -> Result<(), String> {
    // code review(milestone 3で発覚): 以前は`!t.is_error`も条件に含めていたため、
    // `error struct X {...}`宣言がここで丸ごと無視され、is_error_typeタグが一切効かない
    // バグになっていた。error structもここで解決し(下のstruct構築コードが既に
    // `is_error_type: decl.is_error`を渡しているので、それ以外の変更は不要)、
    // json structだけを引き続き対象外にする(decode<X>自動生成はモジュールmilestoneまで先送り)
    let type_decls: Vec<&TypeDecl> =
        types.iter().filter(|t| !t.is_json && matches!(t.node, TypeNode::StructType { .. } | TypeNode::Union { .. })).collect();
    let names: HashSet<&str> = type_decls.iter().map(|t| t.name.as_str()).collect();

    if let Some(cycle_name) = find_type_decl_cycle(&type_decls, &names) {
        return Err(format!("checker: self-referential/cyclic type definitions are not yet supported (found via '{cycle_name}')"));
    }

    // 固定点反復: 依存先が(宣言順に関係なく)先に解決されているかどうかに関わらず、
    // 現在のレジストリの中身で全宣言を素朴に再解決するのをN回繰り返す。非循環である
    // ことは上のサイクル検出で保証済みなので、依存の深さはtypes.len()を超えない。
    // 他パッケージへの参照(pkg修飾された型注釈)はfind_type_decl_cycleが素の名前しか
    // 見ないため対象に含まれず、resolve_type_node経由でregistryから都度解決される
    // (パッケージは依存順に処理されるので、そのregistry参照は既に確定済み——反復不要)
    for _ in 0..type_decls.len().max(1) {
        for decl in &type_decls {
            match &decl.node {
                TypeNode::StructType { fields, .. } => {
                    let resolved_fields =
                        fields.iter().map(|f| types::StructField { name: f.name.clone(), type_: resolve_type_node(ctx, &f.type_node) }).collect();
                    let name = qualify_struct_name(ctx.pkg(), &decl.name);
                    ctx.declare_struct(&decl.name, Type::Struct { name, fields: resolved_fields, is_error_type: decl.is_error });
                }
                // union型alias(`type Status = "active" | "banned"`等)。discriminant_tagは
                // 計算しない——is/matchのcodegenはASTのTypeNodeから直接テストを組み立てる
                // (TS版genTypeTestと同じ)ため、コード生成には不要(§計画参照)。
                // `error type X = A | B`(milestone 8)ならタグ付けも行う
                TypeNode::Union { members, .. } => {
                    let resolved = resolve_type_node(ctx, &decl.node);
                    let resolved = if decl.is_error { tag_error_union(&decl.name, members, resolved)? } else { resolved };
                    ctx.declare_union(&decl.name, resolved);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

// error type X = A | B(union形式、milestone 8)。TS版`tagErrorMembers`の移植——union宣言の
// メンバーは「このunionのために今まさに作られた無名{...}」だけを許す(既存の名前付き型への
// 参照は、その型が使われる他の場所すべてにis_error_typeが波及してしまうため、TS版でも
// `error-type-aliases-existing`診断で常に拒否される)。通ったメンバーすべてに
// is_error_type: trueを立てる(単体の`error struct`と違い、union形は「全メンバーが
// 等しく失敗を表す」——discriminated unionの各バリアントがそれぞれ別の種類の失敗、
// という設計)。診断を出さない設計なので、TS版の2つの診断(`error-type-must-be-struct`/
// `error-type-aliases-existing`)はまとめて明確なErrにする
fn tag_error_union(name: &str, source_members: &[TypeNode], resolved: Type) -> Result<Type, String> {
    if !source_members.iter().all(|m| matches!(m, TypeNode::StructType { .. })) {
        return Err(format!(
            "checker: error type '{name}' members must be freshly-declared struct shapes ({{ ... }}) \
             — referencing an existing named type as an error type member is not supported"
        ));
    }
    fn tag(t: Type) -> Type {
        match t {
            Type::Struct { name, fields, .. } => Type::Struct { name, fields, is_error_type: true },
            other => other,
        }
    }
    Ok(match resolved {
        Type::Union { members, discriminant_tag } => Type::Union { members: members.into_iter().map(tag).collect(), discriminant_tag },
        other => tag(other),
    })
}

// struct型の内部識別名にパッケージを織り込む(TS版`types-resolve.ts`と同じ、ドット区切り)。
// mainパッケージは無修飾のまま(既存milestone 1〜5の単一ファイル挙動と完全互換)
pub fn qualify_struct_name(pkg: &str, name: &str) -> String {
    if pkg == "main" { name.to_string() } else { format!("{pkg}.{name}") }
}

// struct宣言同士の直接参照関係(fieldsに現れる型名)を有向グラフとして辿り、循環を検出する。
// Array/Chan/MapType/Union/FnTypeの中も再帰的に見る(例: `children: Node[]`も
// `Node`への依存として数える——配列越しでも固定点反復の収束が壊れる点は同じため)
fn find_type_decl_cycle(type_decls: &[&TypeDecl], names: &HashSet<&str>) -> Option<String> {
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for decl in type_decls {
        let mut referenced = Vec::new();
        match &decl.node {
            TypeNode::StructType { fields, .. } => {
                for f in fields {
                    collect_referenced_names(&f.type_node, &mut referenced);
                }
            }
            TypeNode::Union { members, .. } => {
                for m in members {
                    collect_referenced_names(m, &mut referenced);
                }
            }
            _ => {}
        }
        deps.insert(decl.name.clone(), referenced.into_iter().filter(|n| names.contains(n.as_str())).collect());
    }

    let mut visiting: HashSet<String> = HashSet::new();
    let mut done: HashSet<String> = HashSet::new();
    for decl in type_decls {
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

// code review指摘(milestone 6): pkg修飾された参照(`otherpkg.Point`)は他パッケージの
// 型であり、このパッケージ自身の循環検出の対象外——素の名前だけ見て収集すると、
// たまたま同じ素の名前を持つ同一パッケージ内の無関係なstruct(例: ローカルの`Point`)への
// 依存と誤認され、実際には循環が無いのに「self-referential/cyclic struct」という
// 誤ったErrになってしまう(find_struct_cycleの`names`フィルタが素の名前だけで
// 一致判定するため)
fn collect_referenced_names(node: &TypeNode, out: &mut Vec<String>) {
    match node {
        TypeNode::Name { name, pkg: None, .. } => out.push(name.clone()),
        TypeNode::Name { pkg: Some(_), .. } => {}
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
        Expr::Call { callee, args, .. } => infer_call(ctx, callee, args),
        // pkg修飾されたstruct literal(`mathutil.Point{...}`)はパッケージのレジストリから
        // 引く(milestone 6・複数パッケージ対応)。それ以外(pkg: None)は今まで通り
        // 素の名前でstruct_typesを引く(見つからなければ殻)。resolve_type_nodeの
        // 同じ分岐と同じ理由でis_package_aliasも確認する(import宣言を経由しない
        // 参照を許すと依存グラフの循環検出が前提とする「全てのパッケージ間参照は
        // import文に対応する」という不変条件が崩れる)
        Expr::StructLit { name, pkg: Some(alias), .. } => {
            if ctx.is_package_alias(alias) { ctx.lookup_package_type(alias, name).cloned() } else { None }
                .unwrap_or_else(|| Type::Struct { name: format!("{alias}.{name}"), fields: vec![], is_error_type: false })
        }
        // union型aliasの名前でも構築できる(`GetUserResponse{kind: "ok", ...}`、milestone 7)。
        // discriminant一致による厳密なmember disambiguationはしない——union全体を近似型
        // として返す(どのexampleも構築直後の式自体の型を厳密に使わないため、実害は無い。
        // §計画参照)
        Expr::StructLit { name, pkg: None, .. } => ctx
            .lookup_struct(name)
            .or_else(|| ctx.lookup_union(name))
            .cloned()
            .unwrap_or_else(|| Type::Struct { name: name.clone(), fields: vec![], is_error_type: false }),
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
        // 配列リテラル: 型注釈があればそれ、無ければ最初の要素の型をwiden_literalしたもの
        // (TS版expressions.tsの"arrayLit"ケースと同じ)。空リテラルはArray(ANY)——
        // 文脈(型注釈つき変数宣言等)で具体化される想定はTS版と同じ
        Expr::ArrayLit { elems, elem_type, .. } => match elem_type {
            Some(t) => Type::Array(Box::new(resolve_type_node(ctx, t))),
            None => match elems.first() {
                None => Type::Array(Box::new(ANY)),
                Some(first) => Type::Array(Box::new(types::widen_literal(infer_expr(ctx, first)))),
            },
        },
        // mapリテラル: key/valueの型注釈は構文上常に必須(TS版と同じ文法)なので、
        // 要素からの推論は不要
        Expr::MapLit { key, value, .. } => {
            Type::Map { key: Box::new(resolve_type_node(ctx, key)), value: Box::new(resolve_type_node(ctx, value)) }
        }
        // 添字読み: targetがMapなら`V | none`(mapの欠損キーはnoneになる)、Arrayなら
        // elem型そのまま(`a[i]`は範囲外panicの設計——`get()`組み込みだけが`elem | none`)、
        // 文字列ならSTRING、それ以外はANY
        Expr::Index { target, .. } => match infer_expr(ctx, target) {
            Type::Map { value, .. } => types::union_of(vec![*value, NONE]),
            Type::Array(elem) => *elem,
            t if types::is_stringy(&t) => STRING,
            _ => ANY,
        },
        // chan<T>(cap): capacityの値そのものは型に影響しない
        Expr::Chan { elem, .. } => Type::Chan(Box::new(resolve_type_node(ctx, elem))),
        // <-ch は常に T | closed(mapの読みがV | noneになるのと同じ理由——closeされうる
        // ことを型で強制する)。chanでなければ(診断は出さないこのリゾルバでは)ANYへ
        // 最善努力でフォールバック
        Expr::Recv { channel, .. } => match infer_expr(ctx, channel) {
            Type::Chan(elem) => types::union_of(vec![*elem, types::CLOSED]),
            _ => ANY,
        },
        // spawn/detachはcheckerの視点では同一(detachedを見ない——TS版のcheckerと同じ)。
        // 戻り値がvoidの関数なら起動するだけ(受取口なし)、それ以外はchan<戻り値型>
        // (呼び出し先が起動時点で1回だけ結果を送る受信専用チャネルとして扱われる)
        Expr::Spawn { call, .. } => {
            let ret = infer_expr(ctx, call);
            if types::type_equals(&ret, &VOID) { VOID } else { Type::Chan(Box::new(ret)) }
        }
        // selectは各アーム(+default)のunion(TS版のmatch-select.tsと同じ)。TS版はアームごとに
        // スコープをpushして束縛名を宣言してからbodyを推論する。infer_exprは&CheckerCtx
        // (不変参照)しか受け取らず、共有ctxへスコープをpushすることはできないため、
        // アームごとに使い捨てのスクラッチctx(clone)を作って束縛名を正しく宣言してから
        // そのスクラッチ上でbodyを推論する(このスクラッチはこの1回の推論だけに使い、
        // すぐ捨てる——gen_select〈codegen側〉が&mut self.ctxで行うpush_scope/declare/
        // pop_scopeと結果的に同じ効果になる)。
        // code review指摘: 以前は束縛名を宣言せず無条件にinfer_expr(ctx, ...)へ渡していた——
        // 「未解決の参照はANYになるだけで無害」という想定だったが、これは2つの意味で
        // 誤りだった。(1) 束縛名がbody内でそのまま参照される典型的なイディオム
        // (`v := <-ch => v`)では、その参照自体が常にANYになり、union_ofがANYを含む
        // union を丸ごとANYへ潰してしまう——結果としてselect式全体の型が(本来の
        // `T | closed`ではなく)ANYになり、後続コードでのmap/chanへの安全ガード
        // (Union添字ガード・非chanへのrecvガード等、いずれもANYは素通しする設計)を
        // 静かにすり抜けてしまう。(2) 束縛名が外側スコープの型が違う変数をshadowして
        // いる場合、bodyの中の参照が外側の(誤った)型に解決されてしまう(例:
        // `v := 42; ... select { v := <-ch => v }`で`v`が誤ってintと推論され、
        // 後続の算術が実際はstringの値に対して`__iarith`を選び、紛らわしい実行時
        // パニックになる)。どちらも実際に再現確認済み
        Expr::Select { arms, default_arm, .. } => {
            let mut arm_types: Vec<Type> = arms
                .iter()
                .map(|a| {
                    let elem_ty = match infer_expr(ctx, &a.channel) {
                        Type::Chan(elem) => types::union_of(vec![*elem, types::CLOSED]),
                        _ => ANY,
                    };
                    let mut scratch = ctx.clone();
                    scratch.declare(&a.name, elem_ty);
                    infer_expr(&scratch, &a.body)
                })
                .collect();
            if let Some(def) = default_arm {
                arm_types.push(infer_expr(ctx, def));
            }
            if arm_types.is_empty() {
                ANY
            } else {
                let void_count = arm_types.iter().filter(|t| types::type_equals(t, &VOID)).count();
                if void_count == arm_types.len() {
                    VOID
                } else if void_count > 0 {
                    ANY // TS版のmixed-void-arms診断に相当。診断を出さないのでANYへ寛容フォールバック
                } else {
                    types::union_of(arm_types)
                }
            }
        }
        // `is`式は常にbool(TS版と同じ——絞り込みの事実はスコープにだけ影響し、式自体の型は
        // 常にBOOL)
        Expr::Is { .. } => BOOL,
        // matchは各アームのunion(milestone 5のselectと全く同じロジックを再利用)。
        // subjectが裸Identの場合だけ、そのアームのパターン集合で絞り込んだ型を一時的に
        // 同じ名前で再宣言してからbodyを推論する(TS版のnarrowPathと同じ目的——codegen側の
        // gen_matchも同じ理由で同じ絞り込みをスコープに反映する。selectの束縛と違い、
        // matchは新しい名前を導入しない〈既存のsubjectの名前をそのまま絞り込むだけ〉ため、
        // スクラッチctxで同名を上書き宣言する形になる)
        Expr::Match { subject, arms, .. } => {
            let subject_ty = infer_expr(ctx, subject);
            let arm_types: Vec<Type> = arms
                .iter()
                .map(|a| {
                    if let Expr::Ident { name, .. } = &**subject {
                        let narrowed = narrow_for_match_patterns(ctx, &subject_ty, &a.patterns);
                        let mut scratch = ctx.clone();
                        scratch.declare(name, narrowed);
                        infer_expr(&scratch, &a.body)
                    } else {
                        infer_expr(ctx, &a.body)
                    }
                })
                .collect();
            if arm_types.is_empty() {
                ANY
            } else {
                let void_count = arm_types.iter().filter(|t| types::type_equals(t, &VOID)).count();
                if void_count == arm_types.len() {
                    VOID
                } else if void_count > 0 {
                    ANY // TS版のmixed-void-arms診断に相当。診断を出さないのでANYへ寛容フォールバック
                } else {
                    types::union_of(arm_types)
                }
            }
        }
        // M7未対応の式はANYへ最善努力でフォールバックする。codegen側がこれらの構文自体を
        // 「まだ対応していません」と明確なエラーにするので、ここで型を誤魔化しても実害は無い
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

// パターン(`is`の右辺・matchアームの型パターン)がunionのmemberに構造的に一致するか
// (TS版`narrowing.ts`のstructPatternMatchesを移植、milestone 7・判別可能union対応)。
// リテラルパターン(`"active"`)は値の完全一致。裸の型名パターン`error`はプリミティブ
// ERROR型のみに一致させる(named error structはTS版でも`error`パターンとは別物——
// 両者が同じunionに共存する場合、TS版は`impossible-pattern`診断でコンパイルエラーに
// する。codegen側の`gen_type_test`も`error`を`instanceof Error`としてのみテストし
// named error struct〈タグ付きの普通のobject〉には決してマッチしないため、ここで
// is_error_type付きstructまで拾ってしまうとchecker/codegenが食い違い、実行時に
// 静かに誤ったアームへ落ちる)。struct形パターン(部分構造、`{kind: "ok"}`)は対象memberが
// structで、パターンの各fieldについて同名フィールドがあり、型まで一致(リテラル型
// パターンなら値まで、それ以外は`type_equals`で構造まで比較——TS版`structPatternMatches`
// と同じ厳密さ)すれば良い(TS版と同じ「余分なfieldは無視」)
pub fn pattern_matches_member(ctx: &CheckerCtx, member: &Type, pattern: &TypeNode) -> bool {
    match pattern {
        TypeNode::Literal { value, .. } => matches!(member, Type::Literal(v) if v == value),
        TypeNode::Name { name, pkg: None, .. } if name == "error" => types::type_equals(member, &ERROR),
        TypeNode::StructType { fields, .. } => {
            let Type::Struct { fields: member_fields, .. } = member else { return false };
            fields.iter().all(|pf| {
                member_fields.iter().find(|mf| mf.name == pf.name).is_some_and(|mf| match &pf.type_node {
                    TypeNode::Literal { value, .. } => matches!(&mf.type_, Type::Literal(v) if v == value),
                    _ => types::type_equals(&mf.type_, &resolve_type_node(ctx, &pf.type_node)),
                })
            })
        }
        _ => types::type_equals(member, &resolve_type_node(ctx, pattern)),
    }
}

fn match_pattern_matches_member(ctx: &CheckerCtx, member: &Type, pattern: &MatchPattern) -> bool {
    match pattern {
        MatchPattern::Wildcard { .. } => true,
        MatchPattern::Type(node) => pattern_matches_member(ctx, member, node),
    }
}

// matchアーム(カンマ区切りの複数パターン、いずれか一致でアーム全体が一致)を踏まえて
// subject_tyを絞り込む(TS版match-select.tsのnarrowPathと同じ目的——ただしcodegen側は
// `&mut self.ctx`を持つので実際のスコープ宣言はcodegen側で行う。ここは絞り込んだ型を
// 計算するだけ)。subject_tyがUnionでなければ絞り込めないのでそのまま返す。ワイルドカードが
// 含まれていればそのアームは何にでも一致するので絞り込まない。一致するmemberが無ければ
// (診断を出さない設計なので)安全側でsubject_tyそのものへフォールバックする
pub fn narrow_for_match_patterns(ctx: &CheckerCtx, subject_ty: &Type, patterns: &[MatchPattern]) -> Type {
    let Type::Union { members, .. } = subject_ty else { return subject_ty.clone() };
    if patterns.iter().any(|p| matches!(p, MatchPattern::Wildcard { .. })) {
        return subject_ty.clone();
    }
    let matched: Vec<Type> = members.iter().filter(|m| patterns.iter().any(|p| match_pattern_matches_member(ctx, m, p))).cloned().collect();
    if matched.is_empty() { subject_ty.clone() } else { types::union_of(matched) }
}

// `is`式・`if x is T`文用: 単一パターンでの絞り込み。戻り値は(then節での絞り込み型,
// else節での絞り込み型)。subject_tyがUnionでなければ絞り込めないのでどちらもそのまま
pub fn narrow_for_is(ctx: &CheckerCtx, subject_ty: &Type, target: &TypeNode) -> (Type, Type) {
    let Type::Union { members, .. } = subject_ty else { return (subject_ty.clone(), subject_ty.clone()) };
    let (matched, rest): (Vec<Type>, Vec<Type>) = members.iter().cloned().partition(|m| pattern_matches_member(ctx, m, target));
    let then_ty = if matched.is_empty() { subject_ty.clone() } else { types::union_of(matched) };
    let else_ty = if rest.is_empty() { subject_ty.clone() } else { types::union_of(rest) };
    (then_ty, else_ty)
}

// TS版`match-not-exhaustive`診断と同じロジックだが、診断は出さない設計なので
// エラーメッセージは一切出さず、codegenが「最後のアームを無条件elseとして信用してよいか」を
// 内部判断するためだけに使う(milestone 2〜6と同じ「TS本体は診断で到達不能だが、診断を
// 出さないこのリゾルバでは実際に到達しうる」パターン——ここでは"到達しうる"のが
// 非exhaustiveなmatchで、codegen側がそれ用の安全ガードを別途持つ)。
// アーム0個は(subjectの型を問わず)絶対に網羅的ではない——TS版はこれを別の診断
// (`empty-match`)で弾くが、このリゾルバは診断を出さないため、ここで確実にfalseを
// 返し、codegenのpanicフォールバックだけで空のアーム本体〈構文的に壊れたJS〉を
// 防ぐ。subject_tyが確実にUnion以外だと分かる場合(struct/int/string等)も、TS版の
// 「union-required」診断が無いこのリゾルバではfalseにして安全ガードを効かせる
// (ANYは型が分からないだけで確実に非unionとは言えないため、これまで通り寛容にtrue)
pub fn match_is_exhaustive(ctx: &CheckerCtx, subject_ty: &Type, arms: &[MatchArm]) -> bool {
    if arms.is_empty() {
        return false;
    }
    let Type::Union { members, .. } = subject_ty else { return matches!(subject_ty, Type::Any) };
    if arms.iter().any(|a| a.patterns.iter().any(|p| matches!(p, MatchPattern::Wildcard { .. }))) {
        return true;
    }
    members.iter().all(|m| arms.iter().any(|a| a.patterns.iter().any(|p| match_pattern_matches_member(ctx, m, p))))
}

// range-forのループ変数をsubjectの型に応じてスコープへ宣言する(TS版
// `checker/statements.ts`のrange-for検査を移植。個数不一致等の診断は対象外——
// 与えられた名前の数だけ順番に宣言する。`ctx.declare`が"_"を自動で捨てるので
// ブランク名の特別扱いは不要)。Array: names[0]→int(添字)・names[1]→elem型(あれば)。
// Map: names[0]→key型・names[1]→value型(あれば)。int: names[0]→int。
// それ以外(Any等)は与えられた名前全てをANYで宣言する
pub fn declare_range_for_names(ctx: &mut CheckerCtx, subject_ty: &Type, names: &[String]) {
    match subject_ty {
        Type::Array(elem) => {
            if let Some(n) = names.first() {
                ctx.declare(n, INT);
            }
            if let Some(n) = names.get(1) {
                ctx.declare(n, (**elem).clone());
            }
        }
        Type::Map { key, value } => {
            if let Some(n) = names.first() {
                ctx.declare(n, (**key).clone());
            }
            if let Some(n) = names.get(1) {
                ctx.declare(n, (**value).clone());
            }
        }
        t if types::type_equals(t, &INT) => {
            if let Some(n) = names.first() {
                ctx.declare(n, INT);
            }
        }
        _ => {
            for n in names {
                ctx.declare(n, ANY);
            }
        }
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
// 引けるようにして修正)。
// argsはmilestone 4で追加——get/sort/keys/values等、引数依存の組み込みの戻り値型を
// 解決するために必要(例えばsort(nums)の戻り値がint[]と分からないと、後続の算術が
// __iarith経由にならずTS版とbyte単位で食い違う出力になる)
fn infer_call(ctx: &CheckerCtx, callee: &Expr, args: &[Expr]) -> Type {
    if let Expr::Ident { name, .. } = callee {
        if let Some(Type::Fn { ret, .. }) = ctx.lookup_fn(name) {
            return (**ret).clone();
        }
        if let Some(t) = infer_builtin_call(ctx, name, args) {
            return t;
        }
    }
    // パッケージ修飾の自由関数呼び出し(`mathutil.add(...)`、milestone 6)。ローカル変数に
    // よるshadowが優先される(TS版tryPackageMemberと同じ優先順位)ので、is_package_alias
    // (ローカルスコープに無いことも確認済み)を通ったものだけをパッケージ参照とみなす
    if let Expr::Member { target, name, .. } = callee
        && let Expr::Ident { name: alias, .. } = &**target
        && ctx.is_package_alias(alias)
        && let Some(Type::Fn { ret, .. }) = ctx.lookup_package_fn(alias, name)
    {
        return (**ret).clone();
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

// codegenが実際に生成できる組み込みの戻り値型を解決する(TS版`checker/builtins.ts`の
// `inferBuiltinCall`を移植。診断は出さないので検査ロジックは持たず、型の解決だけ行う)。
// `filter`/`map`/`reduce`は無名関数(Expr::FnExpr)のcodegenが無くまだ呼び出せないため、
// ここでも対象外のまま(ANYへのフォールバックで実害は無い)
fn infer_builtin_call(ctx: &CheckerCtx, name: &str, args: &[Expr]) -> Option<Type> {
    Some(match name {
        "print" | "sleep" | "push" | "close" | "delete" => VOID,
        "str" | "join" | "trim" | "upper" | "lower" => STRING,
        "toInt" => types::union_of(vec![INT, ERROR]),
        "toFloat" => FLOAT,
        "round" | "floor" | "ceil" => INT,
        "error" => ERROR,
        "contains" => BOOL,
        "indexOf" => types::union_of(vec![INT, NONE]),
        "split" => Type::Array(Box::new(STRING)),
        "len" => INT,
        // get(arr, i): 範囲外はnoneになる安全な読み(`arr[i]`とは違いpanicしない)
        "get" => match args.first().map(|a| infer_expr(ctx, a)) {
            Some(Type::Array(elem)) => types::union_of(vec![*elem, NONE]),
            _ => ANY,
        },
        // sort(arr): 非破壊——同じ配列型のコピーを返す
        "sort" => match args.first().map(|a| infer_expr(ctx, a)) {
            Some(t @ Type::Array(_)) => t,
            _ => ANY,
        },
        "keys" => match args.first().map(|a| infer_expr(ctx, a)) {
            Some(Type::Map { key, .. }) => Type::Array(key),
            _ => Type::Array(Box::new(ANY)),
        },
        "values" => match args.first().map(|a| infer_expr(ctx, a)) {
            Some(Type::Map { value, .. }) => Type::Array(value),
            _ => Type::Array(Box::new(ANY)),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::SelectArm;
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
        let call_ret = infer_call(&ctx, &Expr::Ident { name: "add".into(), pos: pos() }, &[]);
        assert!(types::type_equals(&call_ret, &INT));
    }

    #[test]
    fn infer_callは組み込みの戻り値型も引く() {
        let ctx = CheckerCtx::new();
        let round_call = infer_call(&ctx, &Expr::Ident { name: "round".into(), pos: pos() }, &[]);
        assert!(types::type_equals(&round_call, &INT), "round() should infer as int, got {round_call:?}");
        let to_int_call = infer_call(&ctx, &Expr::Ident { name: "toInt".into(), pos: pos() }, &[]);
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
        resolve_type_decls(&mut ctx, &types).unwrap();
        let Some(Type::Struct { fields, .. }) = ctx.lookup_struct("Line").cloned() else { panic!("expected struct") };
        let Type::Struct { fields: point_fields, name, .. } = &fields[0].type_ else { panic!("expected resolved Point field") };
        assert_eq!(name, "Point");
        assert_eq!(point_fields.len(), 2);
    }

    #[test]
    fn resolve_struct_declsは相互循環structを検出してerrを返す() {
        let types = vec![struct_decl("A", &[("b", name_type("B"))]), struct_decl("B", &[("a", name_type("A"))])];
        let mut ctx = CheckerCtx::new();
        assert!(resolve_type_decls(&mut ctx, &types).is_err());
    }

    #[test]
    fn resolve_struct_declsは自己参照structも検出してerrを返す() {
        let types = vec![struct_decl("Node", &[("next", name_type("Node"))])];
        let mut ctx = CheckerCtx::new();
        assert!(resolve_type_decls(&mut ctx, &types).is_err());
    }

    #[test]
    fn infer_exprはstruct_litとフィールドアクセスの型を解決する() {
        let types = vec![struct_decl("User", &[("name", name_type("string")), ("age", name_type("int"))])];
        let mut ctx = CheckerCtx::new();
        resolve_type_decls(&mut ctx, &types).unwrap();
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
        resolve_type_decls(&mut ctx, &types).unwrap();
        let user_ty = ctx.lookup_struct("User").unwrap().clone();
        ctx.declare_method("User", "describe", Type::Fn { params: vec![user_ty], ret: Box::new(STRING) });
        let recv = Expr::StructLit { name: "User".into(), pkg: None, fields: vec![], pos: pos() };
        let call = Expr::Member { target: Box::new(recv), name: "describe".into(), pos: pos() };
        assert!(types::type_equals(&infer_call(&ctx, &call, &[]), &STRING));
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
        resolve_type_decls(&mut ctx, &types).unwrap();
        let Some(Type::Struct { is_error_type, .. }) = ctx.lookup_struct("NotFound") else { panic!("expected struct") };
        assert!(*is_error_type);
    }

    fn error_union_decl(name: &str, members: Vec<TypeNode>) -> TypeDecl {
        let mut decl = union_decl(name, members);
        decl.is_error = true;
        decl
    }

    #[test]
    fn resolve_type_declsはerror_typeのunion形式を全メンバーis_error_typeとして解決する() {
        let types = vec![error_union_decl(
            "DbError",
            vec![
                struct_type_node(&[("kind", literal_type("notFound")), ("table", name_type("string"))]),
                struct_type_node(&[("kind", literal_type("timeout")), ("ms", name_type("int"))]),
            ],
        )];
        let mut ctx = CheckerCtx::new();
        resolve_type_decls(&mut ctx, &types).unwrap();
        let Some(Type::Union { members, .. }) = ctx.lookup_union("DbError") else { panic!("expected union") };
        assert_eq!(members.len(), 2);
        assert!(members.iter().all(|m| matches!(m, Type::Struct { is_error_type: true, .. })));
    }

    #[test]
    fn resolve_type_declsはerror_typeのメンバーが非struct形ならerrになる() {
        // TS版のerror-type-must-be-struct相当。単一メンバー(`error type Bad = int`)は
        // Rust版のパーサーではUnionノードにすらならず素通りするため、実際に
        // Union分岐へ到達する2メンバーの非struct形(リテラルunion)で検証する
        let types = vec![error_union_decl("Bad", vec![literal_type("a"), literal_type("b")])];
        let mut ctx = CheckerCtx::new();
        assert!(resolve_type_decls(&mut ctx, &types).is_err());
    }

    #[test]
    fn resolve_type_declsはerror_typeのメンバーが既存の名前付き型への参照ならerrになる() {
        // TS版のerror-type-aliases-existing相当
        let types = vec![
            struct_decl("Existing", &[("x", name_type("int"))]),
            error_union_decl("Aliased", vec![name_type("Existing"), struct_type_node(&[("kind", literal_type("other"))])]),
        ];
        let mut ctx = CheckerCtx::new();
        assert!(resolve_type_decls(&mut ctx, &types).is_err());
    }

    #[test]
    fn has_structured_failureとor_binding_typeはerror_typeのunionを正しく失敗として認識する() {
        let types = vec![error_union_decl(
            "DbError",
            vec![struct_type_node(&[("kind", literal_type("notFound"))]), struct_type_node(&[("kind", literal_type("timeout"))])],
        )];
        let mut ctx = CheckerCtx::new();
        resolve_type_decls(&mut ctx, &types).unwrap();
        let db_error = ctx.lookup_union("DbError").unwrap().clone();
        let with_db_error = types::union_of(vec![INT, db_error]);
        assert!(has_structured_failure(&with_db_error));
        let bound = or_binding_type(&with_db_error);
        let Type::Union { members, .. } = &bound else { panic!("expected union binding type, got {bound:?}") };
        assert_eq!(members.len(), 2);
        assert!(members.iter().all(|m| matches!(m, Type::Struct { is_error_type: true, .. })));
    }

    #[test]
    fn infer_exprのprop_orelseはerror_struct込みのunionでも成功メンバーだけを返す() {
        let types = vec![error_struct_decl("NotFound", &[])];
        let mut ctx = CheckerCtx::new();
        resolve_type_decls(&mut ctx, &types).unwrap();
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

    #[test]
    fn infer_exprは配列リテラルの型を推論する() {
        let ctx = CheckerCtx::new();
        // 型注釈あり
        let typed = Expr::ArrayLit { elems: vec![], elem_type: Some(name_type("int")), pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &typed), &Type::Array(Box::new(INT))));
        // 型注釈なし・空 → Array(ANY)
        let empty = Expr::ArrayLit { elems: vec![], elem_type: None, pos: pos() };
        assert!(matches!(infer_expr(&ctx, &empty), Type::Array(e) if matches!(*e, Type::Any)));
        // 型注釈なし・非空 → 最初の要素の型(文字列リテラルはwiden_literalでstringになる)
        let strs = Expr::ArrayLit {
            elems: vec![Expr::String { value: "a".into(), pos: pos() }, Expr::String { value: "b".into(), pos: pos() }],
            elem_type: None,
            pos: pos(),
        };
        assert!(types::type_equals(&infer_expr(&ctx, &strs), &Type::Array(Box::new(STRING))));
    }

    #[test]
    fn infer_exprはmapリテラルの型を推論する() {
        let ctx = CheckerCtx::new();
        let lit = Expr::MapLit { key: name_type("string"), value: name_type("int"), entries: vec![], pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &lit), &Type::Map { key: Box::new(STRING), value: Box::new(INT) }));
    }

    #[test]
    fn infer_exprは添字読みをmap_array_stringで使い分ける() {
        let mut ctx = CheckerCtx::new();
        ctx.declare("m", Type::Map { key: Box::new(STRING), value: Box::new(INT) });
        ctx.declare("a", Type::Array(Box::new(INT)));
        ctx.declare("s", STRING);
        let idx = |name: &str| Expr::Index { target: Box::new(Expr::Ident { name: name.into(), pos: pos() }), index: Box::new(int_lit("0")), pos: pos() };
        // mapはV | none
        assert!(types::type_equals(&infer_expr(&ctx, &idx("m")), &types::union_of(vec![INT, NONE])));
        // 配列はelemそのまま(| noneは付かない——get()と違いa[i]は範囲外panicの設計)
        assert!(types::type_equals(&infer_expr(&ctx, &idx("a")), &INT));
        // 文字列はSTRING
        assert!(types::type_equals(&infer_expr(&ctx, &idx("s")), &STRING));
    }

    #[test]
    fn infer_callはget_sort_keys_valuesの引数依存の戻り値型を解決する() {
        let mut ctx = CheckerCtx::new();
        ctx.declare("arr", Type::Array(Box::new(INT)));
        ctx.declare("m", Type::Map { key: Box::new(STRING), value: Box::new(INT) });
        let ident = |name: &str| Expr::Ident { name: name.into(), pos: pos() };
        let call = |name: &str, arg: &str| Expr::Call { callee: Box::new(ident(name)), args: vec![ident(arg)], pos: pos() };

        assert!(types::type_equals(&infer_expr(&ctx, &call("get", "arr")), &types::union_of(vec![INT, NONE])));
        assert!(types::type_equals(&infer_expr(&ctx, &call("sort", "arr")), &Type::Array(Box::new(INT))));
        assert!(types::type_equals(&infer_expr(&ctx, &call("keys", "m")), &Type::Array(Box::new(STRING))));
        assert!(types::type_equals(&infer_expr(&ctx, &call("values", "m")), &Type::Array(Box::new(INT))));
        assert!(types::type_equals(&infer_expr(&ctx, &call("len", "arr")), &INT));
    }

    #[test]
    fn declare_range_for_namesは配列でindexとelemを宣言する() {
        let mut ctx = CheckerCtx::new();
        declare_range_for_names(&mut ctx, &Type::Array(Box::new(STRING)), &["i".to_string(), "v".to_string()]);
        assert!(types::type_equals(ctx.lookup("i").unwrap(), &INT));
        assert!(types::type_equals(ctx.lookup("v").unwrap(), &STRING));
    }

    #[test]
    fn declare_range_for_namesは配列で名前が1個でもindexだけ宣言する() {
        let mut ctx = CheckerCtx::new();
        declare_range_for_names(&mut ctx, &Type::Array(Box::new(STRING)), &["i".to_string()]);
        assert!(types::type_equals(ctx.lookup("i").unwrap(), &INT));
    }

    #[test]
    fn declare_range_for_namesはmapでkeyとvalueを宣言する() {
        let mut ctx = CheckerCtx::new();
        declare_range_for_names(&mut ctx, &Type::Map { key: Box::new(STRING), value: Box::new(INT) }, &["k".to_string(), "v".to_string()]);
        assert!(types::type_equals(ctx.lookup("k").unwrap(), &STRING));
        assert!(types::type_equals(ctx.lookup("v").unwrap(), &INT));
    }

    #[test]
    fn declare_range_for_namesはintで単一名をintとして宣言する() {
        let mut ctx = CheckerCtx::new();
        declare_range_for_names(&mut ctx, &INT, &["i".to_string()]);
        assert!(types::type_equals(ctx.lookup("i").unwrap(), &INT));
    }

    #[test]
    fn declare_range_for_namesはanyで与えられた名前全てをanyにする() {
        let mut ctx = CheckerCtx::new();
        declare_range_for_names(&mut ctx, &ANY, &["a".to_string(), "b".to_string()]);
        assert!(matches!(ctx.lookup("a"), Some(Type::Any)));
        assert!(matches!(ctx.lookup("b"), Some(Type::Any)));
    }

    fn int_type_node() -> TypeNode {
        TypeNode::Name { name: "int".into(), pkg: None, pos: pos() }
    }

    fn ident(name: &str) -> Expr {
        Expr::Ident { name: name.into(), pos: pos() }
    }

    #[test]
    fn infer_exprはchan生成の型をcapacityによらず推論する() {
        let ctx = CheckerCtx::new();
        let none_cap = Expr::Chan { elem: int_type_node(), capacity: Box::new(Expr::None { pos: pos() }), pos: pos() };
        let num_cap = Expr::Chan { elem: int_type_node(), capacity: Box::new(int_lit("5")), pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &none_cap), &Type::Chan(Box::new(INT))));
        assert!(types::type_equals(&infer_expr(&ctx, &num_cap), &Type::Chan(Box::new(INT))));
    }

    #[test]
    fn infer_exprのrecvはt_or_closedになりchan以外はanyにフォールバックする() {
        let mut ctx = CheckerCtx::new();
        ctx.declare("ch", Type::Chan(Box::new(INT)));
        ctx.declare("notch", INT);
        let recv_ch = Expr::Recv { channel: Box::new(ident("ch")), pos: pos() };
        let recv_notch = Expr::Recv { channel: Box::new(ident("notch")), pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &recv_ch), &types::union_of(vec![INT, types::CLOSED])));
        assert!(matches!(infer_expr(&ctx, &recv_notch), Type::Any));
    }

    #[test]
    fn infer_exprのspawnはvoid戻り値ならvoid_それ以外はchanになりdetachedを見ない() {
        let mut ctx = CheckerCtx::new();
        ctx.declare_fn("log", Type::Fn { params: vec![], ret: Box::new(types::VOID) });
        ctx.declare_fn("compute", Type::Fn { params: vec![], ret: Box::new(INT) });
        let call = |name: &str| Expr::Call { callee: Box::new(ident(name)), args: vec![], pos: pos() };
        let spawn_void = Expr::Spawn { call: Box::new(call("log")), detached: false, pos: pos() };
        let detach_void = Expr::Spawn { call: Box::new(call("log")), detached: true, pos: pos() };
        let spawn_int = Expr::Spawn { call: Box::new(call("compute")), detached: false, pos: pos() };
        let detach_int = Expr::Spawn { call: Box::new(call("compute")), detached: true, pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &spawn_void), &types::VOID));
        assert!(types::type_equals(&infer_expr(&ctx, &detach_void), &types::VOID));
        assert!(types::type_equals(&infer_expr(&ctx, &spawn_int), &Type::Chan(Box::new(INT))));
        assert!(types::type_equals(&infer_expr(&ctx, &detach_int), &Type::Chan(Box::new(INT))));
    }

    #[test]
    fn infer_exprのselectはアームとdefaultのunionになり全void_混在も扱う() {
        let mut ctx = CheckerCtx::new();
        ctx.declare_fn("log", Type::Fn { params: vec![], ret: Box::new(types::VOID) });
        let void_call = || Expr::Call { callee: Box::new(ident("log")), args: vec![], pos: pos() };
        let arm = |body: Expr| SelectArm { name: "v".into(), channel: ident("ch"), body, pos: pos() };

        let all_int = Expr::Select {
            arms: vec![arm(int_lit("1")), arm(int_lit("2"))],
            default_arm: Some(Box::new(int_lit("3"))),
            pos: pos(),
        };
        assert!(types::type_equals(&infer_expr(&ctx, &all_int), &INT));

        let all_void = Expr::Select { arms: vec![arm(void_call()), arm(void_call())], default_arm: None, pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &all_void), &types::VOID));

        let mixed = Expr::Select { arms: vec![arm(int_lit("1")), arm(void_call())], default_arm: None, pos: pos() };
        assert!(matches!(infer_expr(&ctx, &mixed), Type::Any));
    }

    #[test]
    fn infer_exprのselectはアーム束縛名を正しくelem_or_closedとして推論する() {
        // code review指摘: 以前は束縛名を宣言せずにbodyを推論していたため、
        // `v := <-ch => v`のようにbodyが束縛名をそのまま参照するとその参照が
        // 常にANYになり、select式全体の型もANYへ潰れていた(Union添字ガード等の
        // 「確実に非chan/非mapだと分かる場合だけ弾く」ガードをすり抜けてしまう)。
        // 束縛名を正しくchanのelem型(| closed)として推論できることを確認する
        let mut ctx = CheckerCtx::new();
        ctx.declare("ch", Type::Chan(Box::new(INT)));
        let arm = SelectArm { name: "v".into(), channel: ident("ch"), body: ident("v"), pos: pos() };
        let select = Expr::Select { arms: vec![arm], default_arm: None, pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &select), &types::union_of(vec![INT, types::CLOSED])));
    }

    #[test]
    fn infer_exprのselectはアーム束縛名が外側の同名変数をshadowしても外側の型を漏らさない() {
        // code review指摘: 束縛名が外側スコープの型が違う変数をshadowしている場合、
        // 以前は外側の(誤った)型がbodyの推論に漏れてきていた(`v := 42; ... select {
        // v := <-ch => v }`でchの中身がintでなくても`v`がintと誤推論される)
        let mut ctx = CheckerCtx::new();
        ctx.declare("v", INT); // 外側スコープのvはint
        ctx.declare("ch", Type::Chan(Box::new(STRING)));
        let arm = SelectArm { name: "v".into(), channel: ident("ch"), body: ident("v"), pos: pos() };
        let select = Expr::Select { arms: vec![arm], default_arm: None, pos: pos() };
        // 束縛したvの型(string | closed)が推論されるべきで、外側のint型が漏れてはいけない
        assert!(types::type_equals(&infer_expr(&ctx, &select), &types::union_of(vec![STRING, types::CLOSED])));
    }

    #[test]
    fn qualify_struct_nameはmainなら無修飾_それ以外はドット修飾する() {
        assert_eq!(qualify_struct_name("main", "Point"), "Point");
        assert_eq!(qualify_struct_name("mathutil", "Point"), "mathutil.Point");
    }

    #[test]
    fn resolve_struct_declsはmain以外のパッケージでstruct名をpkg修飾する() {
        let types = vec![struct_decl("Point", &[("x", name_type("int"))])];
        let mut ctx = CheckerCtx::new();
        ctx.begin_package("mathutil", HashSet::new());
        resolve_type_decls(&mut ctx, &types).unwrap();
        // struct_typesのキー自体は素の名前のまま(パッケージ内部からは無修飾で引ける)
        let Some(Type::Struct { name, .. }) = ctx.lookup_struct("Point").cloned() else { panic!("expected struct") };
        assert_eq!(name, "mathutil.Point");
    }

    #[test]
    fn パッケージレジストリはexportedシンボルをpkg修飾名で引ける() {
        let mut ctx = CheckerCtx::new();
        let mut symbols = PackageSymbols::default();
        symbols
            .types
            .insert("Point".into(), Type::Struct { name: "mathutil.Point".into(), fields: vec![types::StructField { name: "x".into(), type_: INT }], is_error_type: false });
        symbols.fns.insert("add".into(), Type::Fn { params: vec![INT, INT], ret: Box::new(INT) });
        ctx.register_package("mathutil", symbols);
        ctx.begin_package("main", HashSet::from(["mathutil".to_string()]));

        let qualified = TypeNode::Name { name: "Point".into(), pkg: Some("mathutil".into()), pos: pos() };
        let Type::Struct { name, fields, .. } = resolve_type_node(&ctx, &qualified) else { panic!("expected struct") };
        assert_eq!(name, "mathutil.Point");
        assert_eq!(fields.len(), 1, "importが宣言済みなのでレジストリの実体(フィールド込み)が引けるべき");

        let lit = Expr::StructLit { name: "Point".into(), pkg: Some("mathutil".into()), fields: vec![], pos: pos() };
        let Type::Struct { name, fields, .. } = infer_expr(&ctx, &lit) else { panic!("expected struct") };
        assert_eq!(name, "mathutil.Point");
        assert_eq!(fields.len(), 1);
    }

    #[test]
    fn import宣言していないパッケージへの修飾参照はレジストリを引かず殻にフォールバックする() {
        // code review指摘: 以前はimport_aliasesを確認せずレジストリを直接引いていたため、
        // 実際にはimportしていない(が別経路でロードされた)パッケージの型でも解決できて
        // しまっていた——これは依存グラフの循環検出がimport文だけを見て構築される、
        // という前提を崩す(パッケージ間参照がimport文を経由しなくても成立してしまうため)。
        // "mathutil"をregister_packageはするがbegin_packageのimport_aliasesには含めない
        // (=このパッケージはmathutilをimportしていない)ことで、レジストリに実体があっても
        // 解決されず殻にフォールバックすることを確認する
        let mut ctx = CheckerCtx::new();
        let mut symbols = PackageSymbols::default();
        symbols
            .types
            .insert("Point".into(), Type::Struct { name: "mathutil.Point".into(), fields: vec![types::StructField { name: "x".into(), type_: INT }], is_error_type: false });
        ctx.register_package("mathutil", symbols);
        ctx.begin_package("main", HashSet::new()); // "mathutil"をimportしていない

        let qualified = TypeNode::Name { name: "Point".into(), pkg: Some("mathutil".into()), pos: pos() };
        let Type::Struct { fields, .. } = resolve_type_node(&ctx, &qualified) else { panic!("expected struct") };
        assert!(fields.is_empty(), "importしていないので殻(空フィールド)にフォールバックすべき");

        let lit = Expr::StructLit { name: "Point".into(), pkg: Some("mathutil".into()), fields: vec![], pos: pos() };
        let Type::Struct { fields, .. } = infer_expr(&ctx, &lit) else { panic!("expected struct") };
        assert!(fields.is_empty());
    }

    #[test]
    fn infer_callはpkg修飾された自由関数呼び出しの戻り値型を引き_ローカル変数のshadowが優先される() {
        let mut ctx = CheckerCtx::new();
        let mut symbols = PackageSymbols::default();
        symbols.fns.insert("add".into(), Type::Fn { params: vec![INT, INT], ret: Box::new(INT) });
        ctx.register_package("mathutil", symbols);
        ctx.begin_package("main", HashSet::from(["mathutil".to_string()]));

        let callee = Expr::Member { target: Box::new(ident("mathutil")), name: "add".into(), pos: pos() };
        let args = [int_lit("1"), int_lit("2")];
        assert!(types::type_equals(&infer_call(&ctx, &callee, &args), &INT));

        // ローカル変数によるshadowが優先される(TS版tryPackageMemberと同じ優先順位)
        ctx.declare("mathutil", INT);
        assert!(matches!(infer_call(&ctx, &callee, &args), Type::Any));
    }

    // ---- milestone 7: match/is式・判別可能union ----

    fn struct_type_node(fields: &[(&str, TypeNode)]) -> TypeNode {
        TypeNode::StructType {
            fields: fields.iter().map(|(fname, ft)| StructFieldNode { name: fname.to_string(), type_node: ft.clone(), pos: pos() }).collect(),
            pos: pos(),
        }
    }

    fn literal_type(v: &str) -> TypeNode {
        TypeNode::Literal { value: v.to_string(), pos: pos() }
    }

    fn union_decl(name: &str, members: Vec<TypeNode>) -> TypeDecl {
        TypeDecl { name: name.to_string(), node: TypeNode::Union { members, pos: pos() }, exported: false, is_error: false, is_json: false, pos: pos() }
    }

    #[test]
    fn union型aliasが登録されresolve_type_nodeで解決できる() {
        let types = vec![union_decl("Status", vec![literal_type("active"), literal_type("banned")])];
        let mut ctx = CheckerCtx::new();
        resolve_type_decls(&mut ctx, &types).unwrap();
        let status_ty = resolve_type_node(&ctx, &name_type("Status"));
        let Type::Union { members, .. } = &status_ty else { panic!("expected union, got {status_ty:?}") };
        assert_eq!(members.len(), 2);
    }

    #[test]
    fn structとunion型aliasの相互参照でも循環が無ければ解決できる() {
        let types = vec![
            struct_decl("User", &[("name", name_type("string"))]),
            union_decl(
                "Resp",
                vec![
                    struct_type_node(&[("kind", literal_type("ok")), ("user", name_type("User"))]),
                    struct_type_node(&[("kind", literal_type("err"))]),
                ],
            ),
        ];
        let mut ctx = CheckerCtx::new();
        resolve_type_decls(&mut ctx, &types).unwrap();
        let resp_ty = resolve_type_node(&ctx, &name_type("Resp"));
        let Type::Union { members, .. } = &resp_ty else { panic!("expected union") };
        assert_eq!(members.len(), 2);
        let Type::Struct { fields, .. } = &members[0] else { panic!("expected struct member") };
        let user_field = fields.iter().find(|f| f.name == "user").expect("user field");
        assert!(types::type_equals(&user_field.type_, &resolve_type_node(&ctx, &name_type("User"))));
    }

    #[test]
    fn 自己参照するunion型aliasは循環として検出されerrになる() {
        let types = vec![union_decl(
            "Tree",
            vec![
                struct_type_node(&[("kind", literal_type("leaf")), ("value", name_type("int"))]),
                struct_type_node(&[("kind", literal_type("node")), ("left", name_type("Tree")), ("right", name_type("Tree"))]),
            ],
        )];
        let mut ctx = CheckerCtx::new();
        assert!(resolve_type_decls(&mut ctx, &types).is_err());
    }

    #[test]
    fn pattern_matches_memberはリテラル_裸型名_struct形パターンを判定する() {
        let ctx = CheckerCtx::new();
        assert!(pattern_matches_member(&ctx, &Type::Literal("active".into()), &literal_type("active")));
        assert!(!pattern_matches_member(&ctx, &Type::Literal("active".into()), &literal_type("banned")));

        assert!(pattern_matches_member(&ctx, &ERROR, &name_type("error")));
        // named error struct(is_error_type付き)は`error`パターンとは別物——TS版は
        // これをimpossible-patternで弾き、codegenの`instanceof Error`テストも
        // named error structには決してマッチしないため、checker側もマッチさせない
        let err_struct = Type::Struct { name: "Oops".into(), fields: vec![], is_error_type: true };
        assert!(!pattern_matches_member(&ctx, &err_struct, &name_type("error")));
        assert!(pattern_matches_member(&ctx, &NONE, &name_type("none")));
        assert!(pattern_matches_member(&ctx, &INT, &name_type("int")));

        let member = resolve_type_node(&ctx, &struct_type_node(&[("kind", literal_type("ok")), ("user", name_type("string"))]));
        // リテラル値フィールドが一致するパターンだけ通す
        assert!(pattern_matches_member(&ctx, &member, &struct_type_node(&[("kind", literal_type("ok"))])));
        assert!(!pattern_matches_member(&ctx, &member, &struct_type_node(&[("kind", literal_type("notFound"))])));
        // 非リテラルフィールドも型まで一致して初めて通る(TS版structPatternMatchesと同じ厳密さ)
        assert!(pattern_matches_member(&ctx, &member, &struct_type_node(&[("user", name_type("string"))])));
        assert!(!pattern_matches_member(&ctx, &member, &struct_type_node(&[("user", name_type("int"))])));
        // 対象memberに無いフィールド名を要求するパターンは一致しない
        assert!(!pattern_matches_member(&ctx, &member, &struct_type_node(&[("missing", name_type("string"))])));
    }

    #[test]
    fn narrow_for_match_patternsはunionを絞り込みワイルドカードなら絞り込まない() {
        let ctx = CheckerCtx::new();
        let ok_shape = struct_type_node(&[("kind", literal_type("ok"))]);
        let err_shape = struct_type_node(&[("kind", literal_type("notFound"))]);
        let subject_ty = resolve_type_node(&ctx, &TypeNode::Union { members: vec![ok_shape.clone(), err_shape.clone()], pos: pos() });

        let narrowed = narrow_for_match_patterns(&ctx, &subject_ty, &[MatchPattern::Type(ok_shape.clone())]);
        let Type::Struct { fields, .. } = &narrowed else { panic!("expected single struct member, got {narrowed:?}") };
        assert!(fields.iter().any(|f| f.name == "kind"));

        let wildcard_narrowed = narrow_for_match_patterns(&ctx, &subject_ty, &[MatchPattern::Wildcard { pos: pos() }]);
        assert!(types::type_equals(&wildcard_narrowed, &subject_ty));

        assert!(types::type_equals(&narrow_for_match_patterns(&ctx, &INT, &[MatchPattern::Type(name_type("int"))]), &INT));
    }

    #[test]
    fn match_is_exhaustiveは網羅性を判定する() {
        let ctx = CheckerCtx::new();
        let ok_shape = struct_type_node(&[("kind", literal_type("ok"))]);
        let err_shape = struct_type_node(&[("kind", literal_type("notFound"))]);
        let subject_ty = resolve_type_node(&ctx, &TypeNode::Union { members: vec![ok_shape.clone(), err_shape.clone()], pos: pos() });

        let arm = |pattern: TypeNode| MatchArm { patterns: vec![MatchPattern::Type(pattern)], body: int_lit("1"), pos: pos() };
        assert!(match_is_exhaustive(&ctx, &subject_ty, &[arm(ok_shape.clone()), arm(err_shape.clone())]));
        assert!(!match_is_exhaustive(&ctx, &subject_ty, &[arm(ok_shape.clone())]));

        let wildcard_arm = MatchArm { patterns: vec![MatchPattern::Wildcard { pos: pos() }], body: int_lit("1"), pos: pos() };
        assert!(match_is_exhaustive(&ctx, &subject_ty, &[arm(ok_shape.clone()), wildcard_arm]));

        // アーム0個は(subjectの型を問わず)絶対に非網羅的——空のmatchを無条件elseとして
        // 信用してしまうと、codegenが空のアーム本体を生成し構文的に壊れたJSになる
        assert!(!match_is_exhaustive(&ctx, &INT, &[]));
        // 確実にUnion以外だと分かるsubjectは、アームがあっても無条件では信用しない
        // (TS版の"union-required"診断が無いこのリゾルバの安全ガード)
        assert!(!match_is_exhaustive(&ctx, &INT, &[arm(ok_shape)]));
        // ANYは「確実に非unionとは言えない」ので、これまで通り寛容にtrue
        assert!(match_is_exhaustive(&ctx, &Type::Any, &[arm(name_type("int"))]));
    }

    #[test]
    fn infer_exprのisは常にboolになる() {
        let ctx = CheckerCtx::new();
        let is_expr = Expr::Is { operand: Box::new(int_lit("1")), target: name_type("int"), pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &is_expr), &BOOL));
    }

    #[test]
    fn infer_exprのmatchはアームの型のunionになりnarrowingが効く() {
        let base_ctx = CheckerCtx::new();
        let ok_shape = struct_type_node(&[("kind", literal_type("ok")), ("value", name_type("int"))]);
        let err_shape = struct_type_node(&[("kind", literal_type("err"))]);
        let union_ty = resolve_type_node(&base_ctx, &TypeNode::Union { members: vec![ok_shape.clone(), err_shape.clone()], pos: pos() });

        let mut ctx = CheckerCtx::new();
        ctx.declare("res", union_ty);

        // "ok"アームのbodyがres.valueを参照——narrowingが効いていればint、
        // 効いていなければ(絞り込まれずunion全体のままだと)フィールドが揃わずANYになる
        let field_access = Expr::Member { target: Box::new(ident("res")), name: "value".into(), pos: pos() };
        let match_expr = Expr::Match {
            subject: Box::new(ident("res")),
            arms: vec![
                MatchArm { patterns: vec![MatchPattern::Type(ok_shape)], body: field_access, pos: pos() },
                MatchArm { patterns: vec![MatchPattern::Type(err_shape)], body: int_lit("0"), pos: pos() },
            ],
            pos: pos(),
        };
        assert!(types::type_equals(&infer_expr(&ctx, &match_expr), &INT));
    }

    #[test]
    fn infer_exprのmatchは全void_混在も扱う() {
        let mut ctx = CheckerCtx::new();
        ctx.declare_fn("log", Type::Fn { params: vec![], ret: Box::new(types::VOID) });
        ctx.declare("x", INT);
        let void_call = || Expr::Call { callee: Box::new(ident("log")), args: vec![], pos: pos() };
        let arm = |body: Expr| MatchArm { patterns: vec![MatchPattern::Wildcard { pos: pos() }], body, pos: pos() };

        let all_void = Expr::Match { subject: Box::new(ident("x")), arms: vec![arm(void_call()), arm(void_call())], pos: pos() };
        assert!(types::type_equals(&infer_expr(&ctx, &all_void), &types::VOID));

        let mixed = Expr::Match { subject: Box::new(ident("x")), arms: vec![arm(int_lit("1")), arm(void_call())], pos: pos() };
        assert!(matches!(infer_expr(&ctx, &mixed), Type::Any));
    }
}
