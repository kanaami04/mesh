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

// 1ファイルぶんの発見。`file`は`_test.mesh`かどうかの判定と、報告する位置情報に使う
pub fn discover_in(pkg: &str, file: &str, program: &Program) -> DiscoveredTests {
    let mut tests = Vec::new();
    let mut diagnostics = Vec::new();
    if !file.ends_with("_test.mesh") {
        return DiscoveredTests { tests, diagnostics };
    }
    // 戻り値型の解決にはchecker.rs側のリゾルバを使う(`type R = none | error`のような
    // エイリアス越しでも判定できるように——TS版もresolveType後にtypeEqualsで比べている)
    let mut ctx = CheckerCtx::new();
    let _ = crate::checker::resolve_type_decls(&mut ctx, &program.types);
    let expected = types::union_of(vec![NONE, ERROR]);
    for f in &program.fns {
        if f.receiver.is_some() || !f.name.starts_with("test") {
            continue;
        }
        let ret = resolve_return_type(&ctx, &f.ret);
        if !f.params.is_empty() || !types::type_equals(&ret, &expected) {
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

// voidの戻り値は`Type::Void`になる——`() none | error`との比較で使うので公開はしない
#[allow(dead_code)]
fn _unused(_t: &Type) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn discover(file: &str, src: &str) -> DiscoveredTests {
        discover_in("main", file, &parse(src).expect("テスト用ソースはパースできること"))
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
        let d = discover_in("mathutil", "mathutil/ops_test.mesh", &program);
        assert_eq!(d.tests[0].js_name, "mathutil$testAdds");
    }

    #[test]
    fn 型aliasを経由した戻り値も受け付ける() {
        // TS版もresolveType後にtypeEqualsで比べるので、エイリアス越しでも通る
        let d = discover("a_test.mesh", "type R = none | error\n\nfn testX() R {\n    return none\n}\n");
        assert_eq!(d.diagnostics, vec![]);
        assert_eq!(d.tests.len(), 1);
    }
}
