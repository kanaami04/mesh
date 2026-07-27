// parityコーパスの**構文カバレッジ**を測る(milestone 67)。
//
// milestone 65で「mainで既に出荷されていた誤検知」を5種見つけたが、原因は
// **コーパスがその形(`!`/`&&`/`||`で包んだ条件式)を1つも含んでいなかった**こと。
// 「parityが0件」は「コーパスに載っている形については一致」という意味しかない。
//
// そこで「ASTのどの変種がコーパスに一度も現れないか」を機械的に検出する。
// 未使用の変種があるということは、**その構文についてTS版との一致を一度も測っていない**
// ということ——新しい検査を足したときに誤検知を出しても気づけない。
//
// 判定は`format!("{:?}", program)`(Debug出力)に変種名が現れるかで行う。専用のvisitorを
// 書くより遥かに短く、ASTへ変種を足したときに自動で追随する(visitorだと足し忘れる)。
// 変種名がフィールド名や文字列リテラルと衝突しないよう、`Ident {`のように**開き括弧まで**
// 含めて数える。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

// ASTの各enumの変種名。`rust/src/ast.rs`から書き写している——ここが古くなると
// カバレッジを過小評価するので、下の`列挙が実際のASTと一致している`テストが見張る
const EXPR_VARIANTS: &[&str] = &[
    "ArrayLit", "Binary", "Bool", "Call", "Chan", "Float", "FnExpr", "Ident", "Index", "Int", "Interp", "Is", "MapLit", "Match", "Member", "None",
    "OrElse", "Prop", "Recv", "Select", "Spawn", "String", "StructLit", "Unary",
];
const STMT_VARIANTS: &[&str] = &[
    "Assign", "Break", "Continue", "DeferStmt", "ExprStmt", "For", "If", "IncDec", "RangeFor", "Return", "Send", "ShortVarDecl", "TypedVarDecl",
    "Wait",
];
const TYPE_NODE_VARIANTS: &[&str] = &["Array", "Chan", "FnType", "Literal", "MapType", "Name", "StructType", "Union"];

fn repo_root() -> PathBuf {
    // このテストは rust/ から走るので1つ上がリポジトリルート
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("rust/ の親").to_path_buf()
}

// コーパス(examples/*.mesh と tests/parity/**/*.mesh)の全ソースを集める
fn corpus_sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out = Vec::new();
    let mut push_dir = |dir: PathBuf, recurse: bool| {
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if recurse {
                        stack.push(p);
                    }
                } else if p.extension().is_some_and(|x| x == "mesh")
                    && let Ok(src) = fs::read_to_string(&p)
                {
                    out.push((p.display().to_string(), src));
                }
            }
        }
    };
    push_dir(root.join("examples"), false);
    push_dir(root.join("tests/parity"), true);
    out
}

// コーパス全体をパースして、Debug出力に現れた変種名を集める。
// パースできないケース(構文エラーを固定しているコーパス)は素通りする
fn seen_variants() -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for (_, src) in corpus_sources() {
        let Ok(program) = mesh::parser::parse(&src) else { continue };
        let dump = format!("{program:?}");
        for v in EXPR_VARIANTS.iter().chain(STMT_VARIANTS).chain(TYPE_NODE_VARIANTS) {
            if dump.contains(&format!("{v} {{")) || dump.contains(&format!("{v}(")) {
                seen.insert((*v).to_string());
            }
        }
    }
    seen
}

#[test]
fn 列挙が実際のastと一致している() {
    // 上の3つの定数が`ast.rs`から漏れていないことを見張る。漏れるとカバレッジを
    // 過小評価する(=測れていない構文を「測れている」と誤認する)
    let ast = fs::read_to_string(repo_root().join("rust/src/ast.rs")).expect("ast.rsを読めること");
    for (enum_name, listed) in [("Expr", EXPR_VARIANTS), ("Stmt", STMT_VARIANTS), ("TypeNode", TYPE_NODE_VARIANTS)] {
        let head = format!("pub enum {enum_name} {{");
        let start = ast.find(&head).unwrap_or_else(|| panic!("enum {enum_name} が見つからない"));
        let body = &ast[start + head.len()..];
        // 波括弧の対応で enum 本体の終わりを探す
        let mut depth = 1usize;
        let mut end = 0usize;
        for (i, c) in body.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let actual: BTreeSet<String> = body[..end]
            .lines()
            .filter_map(|l| {
                let t = l.trim_start();
                // インデント4のみ(ネストしたフィールドを拾わない)、かつ大文字始まり
                if l.len() - t.len() != 4 {
                    return None;
                }
                let name: String = t.chars().take_while(|c| c.is_alphanumeric()).collect();
                let rest = t[name.len()..].trim_start();
                let is_variant = rest.starts_with('{') || rest.starts_with('(') || rest.starts_with(',');
                (is_variant && name.chars().next().is_some_and(char::is_uppercase)).then_some(name)
            })
            .collect();
        let listed: BTreeSet<String> = listed.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(actual, listed, "{enum_name} の変種一覧がast.rsとずれている(このファイルの定数を更新すること)");
    }
}

#[test]
fn コーパスが全ての構文を少なくとも1回は含む() {
    let seen = seen_variants();
    let all: BTreeSet<String> =
        EXPR_VARIANTS.iter().chain(STMT_VARIANTS).chain(TYPE_NODE_VARIANTS).map(|s| (*s).to_string()).collect();
    let missing: Vec<&String> = all.difference(&seen).collect();
    assert!(
        missing.is_empty(),
        "コーパスに一度も現れない構文がある: {missing:?}\n\
         これらは**TS版との一致を一度も測っていない**——tests/parity/ にケースを足すこと\n\
         (milestone 65で、コーパスに無い形の誤検知がmainで出荷されていたのが見つかった)"
    );
}
