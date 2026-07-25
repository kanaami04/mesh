// F-15(`mesh test`)のテスト発見。TS版は`checker/modules.ts`のシグネチャ登録ループの中で
// 兼ねているが、Rust版はchecker(codegen用の最小リゾルバ)とfull_checker(診断)が別なので、
// 「発見」と「診断」を1箇所にまとめてここへ置く——codegenはこの結果からハーネスを組み立て、
// CLIは診断をそのまま報告する。
//
// 規則(TS版と同じ): `_test.mesh`の中の、レシーバ無し・名前が`test`で始まるトップレベル関数。
// シグネチャは常に`() none | error`(P1: テストの合否表現をunion路線から増やさない——
// none=合格、error=失敗という既存の表現をそのまま流用する)。合っていなければ
// `invalid-test-signature`を出し、実行対象には含めない。

use crate::ast::Program;
use crate::checker::{resolve_return_type, CheckerCtx};
use crate::codegen::fn_js_name;
use crate::diagnostic_codes::{Diagnostic, DiagnosticCode};
use crate::types::{self, Type, ERROR, NONE};

#[derive(Debug, Clone, PartialEq)]
pub struct TestInfo {
    pub name: String,
    // 生成JS上の関数名(mainパッケージは無修飾、それ以外は`pkg$name`)
    pub js_name: String,
    pub file: String,
}

pub struct DiscoveredTests {
    pub tests: Vec<TestInfo>,
    pub diagnostics: Vec<Diagnostic>,
}

// 1ファイルぶんの発見。`file`は`_test.mesh`かどうかの判定と、報告する位置情報に使う。
// `pkg_types`は**そのファイルが属するパッケージ全体の型宣言**——テストの戻り値に
// `type R = none | error`のようなエイリアスを使うとき、その宣言は本体側のファイル
// (`main.mesh`等)にあるのが普通なので、自ファイルの型だけで解決すると
// `invalid-test-signature`の誤検知になる(code reviewで発覚・両CLIで再現確認。
// TS版はパッケージ全体を1つのcheckerで見るので元から問題にならない)
pub fn discover_in(pkg: &str, file: &str, program: &Program, pkg_types: &[crate::ast::TypeDecl]) -> DiscoveredTests {
    let mut tests = Vec::new();
    let mut diagnostics = Vec::new();
    if !file.ends_with("_test.mesh") {
        return DiscoveredTests { tests, diagnostics };
    }
    // 戻り値型の解決にはchecker.rs側のリゾルバを使う(エイリアス越しでも判定できるように
    // ——TS版もresolveType後にtypeEqualsで比べている)
    let mut ctx = CheckerCtx::new();
    let _ = crate::checker::resolve_type_decls(&mut ctx, pkg_types);
    for f in &program.fns {
        if f.receiver.is_some() || !f.name.starts_with("test") {
            continue;
        }
        let ret = resolve_return_type(&ctx, &f.ret);
        // 判定不能(型宣言の解決が失敗してunionの中身が埋まっていない)なら診断を出さず、
        // テストとしては受け入れる——**誤検知ではなく検出漏れ側に倒す**という既定方針どおり
        let ret_ok = is_none_or_error(&ret).unwrap_or(true);
        if !f.params.is_empty() || !ret_ok {
            let params: Vec<String> = f.params.iter().map(|p| types::type_to_string(&crate::checker::resolve_type_node(&ctx, &p.type_node))).collect();
            diagnostics.push(Diagnostic {
                pos: f.pos,
                code: DiagnosticCode::InvalidTestSignature,
                message: format!(
                    "test function '{}' must take no parameters and return 'none | error', got ({}) {}",
                    f.name,
                    params.join(", "),
                    types::type_to_string(&ret)
                ),
            });
            continue;
        }
        tests.push(TestInfo { name: f.name.clone(), js_name: fn_js_name(pkg, &f.name), file: file.to_string() });
    }
    DiscoveredTests { tests, diagnostics }
}

// 戻り値が`none | error`かを**パニックせずに**判定する。
// `Some(true/false)`が判定結果、`None`は「判定できない」。
//
// **`types::type_equals`を直接使わない理由**: あちらは`Type::Union`のbody(OnceCell)が
// 必ず埋まっている前提で`.expect()`する。ところが`resolve_type_decls`はknot-tyingのため
// 「空のunionを先に登録してから中身を埋める」ので、**裸のunion循環などで解決が失敗すると
// 中身が空のまま登録が残る**。そこへ`type_equals`を通すとpanicする——実際に
// `type A = B | int` / `type B = A | string`を戻り値に持つテスト関数でクラッシュした
// (code reviewで発覚)。full_checker側は同じ未解決unionを`OnceCell::get()`のOptionで
// 安全に扱っており、この関数もそれに揃える
fn is_none_or_error(t: &Type) -> Option<bool> {
    let Type::Union { body } = t else { return Some(false) };
    // 未解決(解決が途中で失敗した)——判定しない
    let ub = body.get()?;
    if ub.members.len() != 2 {
        return Some(false);
    }
    // メンバーはプリミティブなので、ここでの比較はpanicしない
    let has = |target: &Type| ub.members.iter().any(|m| types::type_equals(m, target));
    Some(has(&NONE) && has(&ERROR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn discover(file: &str, src: &str) -> DiscoveredTests {
        let program = parse(src).expect("テスト用ソースはパースできること");
        let types = program.types.clone();
        discover_in("main", file, &program, &types)
    }

    #[test]
    fn test_meshの中のtest関数を見つける() {
        let d = discover("a_test.mesh", "fn testAdds() none | error {\n    return none\n}\n");
        assert_eq!(d.diagnostics, vec![]);
        assert_eq!(d.tests.len(), 1);
        assert_eq!(d.tests[0].name, "testAdds");
        assert_eq!(d.tests[0].js_name, "testAdds"); // mainパッケージは無修飾
    }

    #[test]
    fn test_mesh以外は見ない() {
        let d = discover("a.mesh", "fn testAdds() none | error {\n    return none\n}\n");
        assert!(d.tests.is_empty());
        assert_eq!(d.diagnostics, vec![]);
    }

    #[test]
    fn test以外の名前は対象外() {
        let d = discover("a_test.mesh", "fn helper() none | error {\n    return none\n}\n");
        assert!(d.tests.is_empty());
        assert_eq!(d.diagnostics, vec![]);
    }

    #[test]
    fn シグネチャが違えばinvalid_test_signature() {
        // 引数あり
        let with_params = discover("a_test.mesh", "fn testX(n: int) none | error {\n    return none\n}\n");
        assert!(with_params.tests.is_empty());
        assert_eq!(with_params.diagnostics.len(), 1);
        assert_eq!(with_params.diagnostics[0].code, DiagnosticCode::InvalidTestSignature);
        assert_eq!(
            with_params.diagnostics[0].message,
            "test function 'testX' must take no parameters and return 'none | error', got (int) none | error"
        );
        // 戻り値が違う
        let bad_ret = discover("a_test.mesh", "fn testY() int {\n    return 1\n}\n");
        assert_eq!(bad_ret.diagnostics.len(), 1);
        assert!(bad_ret.diagnostics[0].message.contains("got () int"), "{}", bad_ret.diagnostics[0].message);
        // 戻り値なし
        let no_ret = discover("a_test.mesh", "fn testZ() {\n}\n");
        assert_eq!(no_ret.diagnostics.len(), 1);
    }

    #[test]
    fn パッケージ側のtestはpkg修飾のjs名になる() {
        let program = parse("fn testAdds() none | error {\n    return none\n}\n").unwrap();
        let d = discover_in("mathutil", "mathutil/ops_test.mesh", &program, &[]);
        assert_eq!(d.tests[0].js_name, "mathutil$testAdds");
    }

    #[test]
    fn 解決できない型宣言があってもパニックしない() {
        // 回帰(code reviewで発覚): 裸のunion循環があると`resolve_type_decls`がErrになり、
        // knot-tyingで登録済みのunionは中身が空のまま残る。そこへ`types::type_equals`を
        // 通すと`.expect()`でpanicしていた(`mesh test`がクラッシュしてJSONも診断も出ない)。
        // 判定不能として扱い、診断は出さずにテストとして受け入れる(検出漏れ側)
        let d = discover("a_test.mesh", "type A = B | int\ntype B = A | string\n\nfn testX() A {\n    return none\n}\n");
        assert_eq!(d.diagnostics, vec![]);
        assert_eq!(d.tests.len(), 1);
    }

    #[test]
    fn 同じパッケージの別ファイルにある型aliasも解決できる() {
        // 回帰(code reviewで発覚): テストの戻り値エイリアスは本体側のファイルで宣言される
        // のが普通なので、自ファイルの型だけで解決すると誤検知になる
        let body = parse("type R = none | error\n\nfn add(a: int, b: int) int {\n    return a + b\n}\n").unwrap();
        let test_file = parse("fn testAdd() R {\n    return none\n}\n").unwrap();
        let d = discover_in("main", "main_test.mesh", &test_file, &body.types);
        assert_eq!(d.diagnostics, vec![]);
        assert_eq!(d.tests.len(), 1);
    }

    #[test]
    fn 型aliasを経由した戻り値も受け付ける() {
        // TS版もresolveType後にtypeEqualsで比べるので、エイリアス越しでも通る
        let d = discover("a_test.mesh", "type R = none | error\n\nfn testX() R {\n    return none\n}\n");
        assert_eq!(d.diagnostics, vec![]);
        assert_eq!(d.tests.len(), 1);
    }
}
