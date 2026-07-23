// H-2(milestone 9): `json struct X { ... }` は decode<X>(v: json.Value) X | error を
// 自動生成する。TS版`src/json-decode.ts`の移植。
//
// アプローチ(TS版と同じ): 生JSを手組みするのではなく、Meshの構文レベルのAST(Stmt/Expr)を
// 合成し、通常のFnDeclとしてprogram.fnsへ追加する。こうすることで、以降のcheck/codegenの
// 経路は一切変更せずそのまま流用でき(合成した関数も普通の関数として型解決・コード生成
// される)、json.field/json.asString等のヘルパー(codegen.rsのjson_stdlib_symbols+
// prelude()側に実装済み)を`?`で繋ぐだけの「手書きデコーダと全く同じ形」のコードを
// 機械的に組み立てる。
//
// 対応するフィールド型(TS版と同じv1スコープ): int/float/string/bool、他のjson struct
// (同一ファイル内)への参照、それらの配列、それらの'T | none'。それ以外(素のstruct・map・
// 一般unionなど)は合成時にErrにし、手書きデコーダ(json.field等を直接使う)を書くよう誘導する。
// TS版の`MultiCompileError`(複数エラー蓄積)は、このリゾルバの「Result<_, String>単一
// エラー」設計と馴染まないため移植しない——最初に見つかったエラーだけを返す(診断を
// 出さない設計なので実害は無い)。

use crate::ast::{Block, Expr, FnDecl, IfStmt, Param, Program, Stmt, StructLitField, TypeDecl, TypeNode};
use crate::token::{Pos, TokenType};
use std::collections::HashSet;

fn primitive_helper(name: &str) -> Option<&'static str> {
    match name {
        "int" => Some("asInt"),
        "float" => Some("asFloat"),
        "string" => Some("asString"),
        "bool" => Some("asBool"),
        _ => None,
    }
}

// ---- AST合成の小さな部品 ----

fn ident_expr(name: &str, pos: Pos) -> Expr {
    Expr::Ident { name: name.to_string(), pos }
}
fn string_lit(value: &str, pos: Pos) -> Expr {
    Expr::String { value: value.to_string(), pos }
}
fn none_expr(pos: Pos) -> Expr {
    Expr::None { pos }
}
fn member_expr(target: Expr, name: &str, pos: Pos) -> Expr {
    Expr::Member { target: Box::new(target), name: name.to_string(), pos }
}
fn call_expr(callee: Expr, args: Vec<Expr>, pos: Pos) -> Expr {
    Expr::Call { callee: Box::new(callee), args, pos }
}
fn prop_expr(operand: Expr, pos: Pos) -> Expr {
    Expr::Prop { operand: Box::new(operand), context: None, pos }
}
fn is_expr(operand: Expr, target: TypeNode, pos: Pos) -> Expr {
    Expr::Is { operand: Box::new(operand), target, pos }
}
fn not_expr(operand: Expr, pos: Pos) -> Expr {
    Expr::Unary { op: TokenType::Bang, operand: Box::new(operand), pos }
}
fn json_call(fn_name: &str, args: Vec<Expr>, pos: Pos) -> Expr {
    call_expr(member_expr(ident_expr("json", pos), fn_name, pos), args, pos)
}
fn block(stmts: Vec<Stmt>) -> Block {
    Block { stmts }
}
fn short_var_decl(name: &str, value: Expr, pos: Pos) -> Stmt {
    Stmt::ShortVarDecl { names: vec![name.to_string()], values: vec![value], mutable: false, pos }
}
fn typed_var_decl(name: &str, type_node: TypeNode, value: Expr, mutable: bool, pos: Pos) -> Stmt {
    Stmt::TypedVarDecl { name: name.to_string(), type_node, value, mutable, pos }
}
fn assign_stmt(name: &str, value: Expr, pos: Pos) -> Stmt {
    Stmt::Assign { targets: vec![ident_expr(name, pos)], values: vec![value], compound_op: None, pos }
}
fn expr_stmt(expr: Expr, pos: Pos) -> Stmt {
    Stmt::ExprStmt { expr, pos }
}
fn return_stmt(value: Option<Expr>, pos: Pos) -> Stmt {
    Stmt::Return { value, pos }
}
fn if_stmt(cond: Expr, then: Block, pos: Pos) -> Stmt {
    Stmt::If(IfStmt { cond, then, else_: None, pos })
}
fn range_for_stmt(names: Vec<String>, subject: Expr, body: Block, pos: Pos) -> Stmt {
    Stmt::RangeFor { names, subject, body, pos }
}
fn name_type(name: &str, pos: Pos) -> TypeNode {
    TypeNode::Name { name: name.to_string(), pkg: None, pos }
}
fn array_type(elem: TypeNode, pos: Pos) -> TypeNode {
    TypeNode::Array { elem: Box::new(elem), pos }
}
fn union_type(members: Vec<TypeNode>, pos: Pos) -> TypeNode {
    TypeNode::Union { members, pos }
}

fn unsupported_field_error(struct_name: &str, field_name: &str, reason: &str) -> String {
    format!("json struct: 'json struct {struct_name}' can't auto-decode field '{field_name}': {reason}")
}

fn is_primitive(t: &TypeNode) -> bool {
    matches!(t, TypeNode::Name { name, pkg: None, .. } if primitive_helper(name).is_some())
}
fn is_nested_json_struct(t: &TypeNode, json_struct_names: &HashSet<String>) -> bool {
    matches!(t, TypeNode::Name { name, pkg: None, .. } if json_struct_names.contains(name))
}
fn is_simple(t: &TypeNode, json_struct_names: &HashSet<String>) -> bool {
    is_primitive(t) || is_nested_json_struct(t, json_struct_names)
}
// 'T | none' の形だけを対象にする(2メンバーちょうど、片方がnone)
fn optional_inner(t: &TypeNode) -> Option<&TypeNode> {
    let TypeNode::Union { members, .. } = t else { return None };
    if members.len() != 2 {
        return None;
    }
    let none_idx = members.iter().position(|m| matches!(m, TypeNode::Name { name, pkg: None, .. } if name == "none"))?;
    Some(&members[1 - none_idx])
}

// primitive/nested な型を、既に取り出し済みのjson.Value式(raw_expr)からデコードする
// 「式1つ」を作る(文は不要 — json.asXxx(...)?  /  decode<Name>(...)? のどちらか)。
// tはis_simpleで確認済み(Name{pkg: None}かつプリミティブ or ネストjson struct)の前提
fn gen_simple_decode_expr(raw_expr: Expr, t: &TypeNode, pos: Pos) -> Expr {
    let TypeNode::Name { name, .. } = t else { unreachable!("gen_simple_decode_expr requires is_simple(t)") };
    match primitive_helper(name) {
        Some(helper) => prop_expr(json_call(helper, vec![raw_expr], pos), pos),
        None => prop_expr(call_expr(ident_expr(&format!("decode{name}"), pos), vec![raw_expr], pos), pos),
    }
}

enum TargetMode {
    Declare,
    Assign,
}

// 配列フィールドのデコード文一式を作る(ループで1つずつ組み立てる)。
// Declareなら`mut <target>: elem[] = []`から新規に、Assignなら既存のmut変数へ最終代入する
// (optionalの中で使う — 一時変数に組み立ててから代入する)
fn gen_array_decode_stmts(raw_array_expr: Expr, elem: &TypeNode, target: &str, target_mode: TargetMode, pos: Pos, uid: &str) -> Vec<Stmt> {
    let raw_arr_name = format!("__raw_arr_{uid}");
    let item_var = format!("__item_{uid}");
    let decoded_var = format!("__decoded_{uid}");
    let acc_name = match target_mode {
        TargetMode::Declare => target.to_string(),
        TargetMode::Assign => format!("__acc_{uid}"),
    };
    let mut stmts = Vec::new();
    stmts.push(short_var_decl(&raw_arr_name, raw_array_expr, pos));
    stmts.push(typed_var_decl(&acc_name, array_type(elem.clone(), pos), Expr::ArrayLit { elems: vec![], elem_type: None, pos }, true, pos));
    let loop_body = block(vec![
        short_var_decl(&decoded_var, gen_simple_decode_expr(ident_expr(&item_var, pos), elem, pos), pos),
        expr_stmt(call_expr(ident_expr("push", pos), vec![ident_expr(&acc_name, pos), ident_expr(&decoded_var, pos)], pos), pos),
    ]);
    stmts.push(range_for_stmt(vec!["_".to_string(), item_var], ident_expr(&raw_arr_name, pos), loop_body, pos));
    if let TargetMode::Assign = target_mode {
        stmts.push(assign_stmt(target, ident_expr(&acc_name, pos), pos));
    }
    stmts
}

// 1フィールド分の「取り出し+デコード」文一式を作る。戻り値のresult_varは、後でstruct
// リテラルを組み立てるときに参照する変数名
fn gen_field_stmts(
    struct_name: &str,
    v_expr: Expr,
    field_name: &str,
    t: &TypeNode,
    json_struct_names: &HashSet<String>,
    pos: Pos,
) -> Result<(Vec<Stmt>, String), String> {
    let result_var = format!("__f_{field_name}");

    if is_simple(t, json_struct_names) {
        let raw_expr = prop_expr(json_call("field", vec![v_expr, string_lit(field_name, pos)], pos), pos);
        let value_expr = gen_simple_decode_expr(raw_expr, t, pos);
        return Ok((vec![short_var_decl(&result_var, value_expr, pos)], result_var));
    }

    if let TypeNode::Array { elem, .. } = t {
        if !is_simple(elem, json_struct_names) {
            return Err(unsupported_field_error(
                struct_name,
                field_name,
                "array element type isn't supported for automatic decoding (only int/float/string/bool or a nested 'json struct')",
            ));
        }
        let raw_expr = prop_expr(json_call("asArray", vec![prop_expr(json_call("field", vec![v_expr, string_lit(field_name, pos)], pos), pos)], pos), pos);
        let stmts = gen_array_decode_stmts(raw_expr, elem, &result_var, TargetMode::Declare, pos, field_name);
        return Ok((stmts, result_var));
    }

    if let Some(inner) = optional_inner(t) {
        if !is_simple(inner, json_struct_names) && !matches!(inner, TypeNode::Array { .. }) {
            return Err(unsupported_field_error(struct_name, field_name, "the non-'none' side of this optional field isn't supported for automatic decoding"));
        }
        if let TypeNode::Array { elem, .. } = inner
            && !is_simple(elem, json_struct_names)
        {
            return Err(unsupported_field_error(
                struct_name,
                field_name,
                "array element type isn't supported for automatic decoding (only int/float/string/bool or a nested 'json struct')",
            ));
        }
        let raw_var = format!("__raw_{field_name}");
        let mut stmts = Vec::new();
        stmts.push(short_var_decl(&raw_var, json_call("optField", vec![v_expr, string_lit(field_name, pos)], pos), pos));
        stmts.push(typed_var_decl(&result_var, union_type(vec![inner.clone(), name_type("none", pos)], pos), none_expr(pos), true, pos));
        let raw_ident = ident_expr(&raw_var, pos);
        let inner_stmts = if let TypeNode::Array { elem, .. } = inner {
            gen_array_decode_stmts(prop_expr(json_call("asArray", vec![raw_ident.clone()], pos), pos), elem, &result_var, TargetMode::Assign, pos, field_name)
        } else {
            vec![assign_stmt(&result_var, gen_simple_decode_expr(raw_ident.clone(), inner, pos), pos)]
        };
        stmts.push(if_stmt(not_expr(is_expr(raw_ident, name_type("none", pos), pos), pos), block(inner_stmts), pos));
        return Ok((stmts, result_var));
    }

    Err(unsupported_field_error(
        struct_name,
        field_name,
        "only int/float/string/bool, a nested 'json struct', an array of those, or 'T | none' of those are \
         supported — write a hand-written decoder (using json.field/json.asString/etc.) for this field instead",
    ))
}

// 1つのjson struct宣言から decode<Name> のFnDeclを合成する
fn synthesize_decoder_fn(td: &TypeDecl, json_struct_names: &HashSet<String>) -> Result<FnDecl, String> {
    let TypeNode::StructType { fields, .. } = &td.node else {
        // parserが"json type"を弾いているので通常は到達しない
        return Err(format!("json struct: 'json' can only mark a 'struct' declaration, not this type shape (found via '{}')", td.name));
    };
    let pos = td.pos;
    let v_param = "v";
    let mut stmts = Vec::new();
    let mut field_values = Vec::new();
    for f in fields {
        let (field_stmts, result_var) = gen_field_stmts(&td.name, ident_expr(v_param, f.pos), &f.name, &f.type_node, json_struct_names, f.pos)?;
        stmts.extend(field_stmts);
        field_values.push(StructLitField { name: f.name.clone(), value: ident_expr(&result_var, f.pos), pos: f.pos });
    }
    stmts.push(return_stmt(Some(Expr::StructLit { name: td.name.clone(), pkg: None, fields: field_values, pos }), pos));
    Ok(FnDecl {
        name: format!("decode{}", td.name),
        receiver: None,
        type_params: vec![],
        params: vec![Param { name: v_param.to_string(), type_node: TypeNode::Name { name: "Value".to_string(), pkg: Some("json".to_string()), pos }, pos }],
        ret: Some(union_type(vec![name_type(&td.name, pos), name_type("error", pos)], pos)),
        body: block(stmts),
        exported: td.exported,
        pos,
    })
}

// program中の全 json struct から decode<Name> 関数群を合成し、program.fnsへ追加する。
// ネスト参照(struct内の別structフィールド)は同一ファイル内のjson structだけを対象にする
// (TS版と同じv1制約 — 他ファイル/他パッケージをまたぐ場合は手書きデコーダで対応する)
pub fn synthesize_json_decoders(program: &mut Program) -> Result<(), String> {
    let json_struct_decls: Vec<TypeDecl> = program.types.iter().filter(|t| t.is_json).cloned().collect();
    if json_struct_decls.is_empty() {
        return Ok(());
    }
    let has_json_import = program.imports.iter().any(|i| i.path == "mesh/json");
    if !has_json_import {
        return Err(
            "json struct: 'json struct' needs 'import \"mesh/json\"' (the generated decoder calls json.field/json.asString/etc.)".to_string(),
        );
    }
    let json_struct_names: HashSet<String> = json_struct_decls.iter().map(|t| t.name.clone()).collect();
    for td in &json_struct_decls {
        program.fns.push(synthesize_decoder_fn(td, &json_struct_names)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::StructFieldNode;
    use crate::parser::parse;

    fn pos() -> Pos {
        Pos { line: 1, col: 1 }
    }

    fn field(name: &str, type_node: TypeNode) -> StructFieldNode {
        StructFieldNode { name: name.to_string(), type_node, pos: pos() }
    }

    fn json_struct_decl(name: &str, fields: Vec<StructFieldNode>) -> TypeDecl {
        TypeDecl { name: name.to_string(), node: TypeNode::StructType { fields, pos: pos() }, exported: false, is_error: false, is_json: true, pos: pos() }
    }

    fn program_with(imports_json: bool, types: Vec<TypeDecl>) -> Program {
        let mut src = String::new();
        if imports_json {
            src.push_str("import \"mesh/json\"\n");
        }
        src.push_str("fn main() {}\n");
        let mut program = parse(&src).unwrap();
        program.types = types;
        program
    }

    #[test]
    fn json_struct宣言が無ければ何もしない() {
        let mut program = program_with(false, vec![]);
        synthesize_json_decoders(&mut program).unwrap();
        assert!(program.fns.iter().all(|f| !f.name.starts_with("decode")));
    }

    #[test]
    fn import_mesh_json_が無ければerrになる() {
        let mut program = program_with(false, vec![json_struct_decl("User", vec![field("name", name_type("string", pos()))])]);
        let err = synthesize_json_decoders(&mut program).unwrap_err();
        assert!(err.contains("needs 'import \"mesh/json\"'"), "got: {err}");
    }

    #[test]
    fn flatなjson_structはdecode関数を合成する() {
        let mut program =
            program_with(true, vec![json_struct_decl("User", vec![field("name", name_type("string", pos())), field("age", name_type("int", pos()))])]);
        synthesize_json_decoders(&mut program).unwrap();
        let decode_fn = program.fns.iter().find(|f| f.name == "decodeUser").expect("decodeUser should be synthesized");
        assert_eq!(decode_fn.params.len(), 1);
        assert!(matches!(&decode_fn.params[0].type_node, TypeNode::Name { name, pkg: Some(p), .. } if name == "Value" && p == "json"));
        assert!(matches!(&decode_fn.ret, Some(TypeNode::Union { members, .. }) if members.len() == 2));
        // 2フィールド分の文 + 最後のreturn
        assert!(decode_fn.body.stmts.len() >= 3);
        assert!(matches!(decode_fn.body.stmts.last(), Some(Stmt::Return { value: Some(Expr::StructLit { .. }), .. })));
    }

    #[test]
    fn 同一ファイル内のネストしたjson_structはdecode呼び出しで参照する() {
        let mut program = program_with(
            true,
            vec![
                json_struct_decl("Address", vec![field("city", name_type("string", pos()))]),
                json_struct_decl("User", vec![field("address", name_type("Address", pos()))]),
            ],
        );
        synthesize_json_decoders(&mut program).unwrap();
        assert!(program.fns.iter().any(|f| f.name == "decodeAddress"));
        assert!(program.fns.iter().any(|f| f.name == "decodeUser"));
    }

    #[test]
    fn 配列フィールドはrange_forでpushするコードになる() {
        let mut program = program_with(true, vec![json_struct_decl("Tags", vec![field("names", array_type(name_type("string", pos()), pos()))])]);
        synthesize_json_decoders(&mut program).unwrap();
        let decode_fn = program.fns.iter().find(|f| f.name == "decodeTags").unwrap();
        assert!(decode_fn.body.stmts.iter().any(|s| matches!(s, Stmt::RangeFor { .. })));
    }

    #[test]
    fn optionalフィールドはoptfieldとifガードになる() {
        let mut program =
            program_with(true, vec![json_struct_decl("User", vec![field("nickname", union_type(vec![name_type("string", pos()), name_type("none", pos())], pos()))])]);
        synthesize_json_decoders(&mut program).unwrap();
        let decode_fn = program.fns.iter().find(|f| f.name == "decodeUser").unwrap();
        assert!(decode_fn.body.stmts.iter().any(|s| matches!(s, Stmt::If(_))));
    }

    #[test]
    fn 未対応のフィールド型はerrになる() {
        // mapフィールドは対応範囲外
        let mut program = program_with(
            true,
            vec![json_struct_decl(
                "Bad",
                vec![field("m", TypeNode::MapType { key: Box::new(name_type("string", pos())), value: Box::new(name_type("int", pos())), pos: pos() })],
            )],
        );
        let err = synthesize_json_decoders(&mut program).unwrap_err();
        assert!(err.contains("can't auto-decode field 'm'"), "got: {err}");
    }

    #[test]
    fn 配列要素が未対応型ならerrになる() {
        let mut program = program_with(
            true,
            vec![json_struct_decl(
                "Bad",
                vec![field("items", array_type(TypeNode::MapType { key: Box::new(name_type("string", pos())), value: Box::new(name_type("int", pos())), pos: pos() }, pos()))],
            )],
        );
        let err = synthesize_json_decoders(&mut program).unwrap_err();
        assert!(err.contains("array element type isn't supported"), "got: {err}");
    }
}
