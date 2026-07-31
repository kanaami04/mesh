// H-2(milestone 9): `json struct X { ... }` は decode<X>(v: json.Value) X | error を
// 自動生成する。TS版`src/json-decode.ts`の移植。
// **milestone 66で逆方向の`encode<X>(x: X) json.Value`も移植した**(このファイル後半。
// design-agenda.md J節)——それまでRust版はデコード方向だけで、生成JSがTS版と食い違っていた。
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
// TS版の`MultiCompileError`(複数エラー蓄積)は、このリゾルバの「Result<_, CompileError>単一
// エラー」設計と馴染まないため移植しない——最初に見つかったエラーだけを返す(診断を
// 出さない設計なので実害は無い)。

use crate::ast::{Block, Expr, FnDecl, IfStmt, MapLitEntry, Param, Program, Stmt, StructLitField, TypeDecl, TypeNode};
use crate::token::{CompileError, Pos, TokenType};
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
    Expr::Call { multiline: false, callee: Box::new(callee), args, pos }
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
    Block { stmts, multiline: false }
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
    TypeNode::Union { members, pos, multiline: false }
}

// **milestone 59でCompileErrorへ変えた**。それまでは`String`だったので位置も診断コードも
// 持てず、TS版の`json-struct-unsupported-field`と突き合わせられなかった(文言だけは一致
// していた)。位置はそのフィールドの型注釈——TS版も同じ場所を指す(実測で確認)
fn unsupported_field_error(struct_name: &str, field_name: &str, reason: &str, pos: Pos) -> Box<CompileError> {
    Box::new(CompileError {
        message: format!("'json struct {struct_name}' can't auto-decode field '{field_name}': {reason}"),
        pos,
        code: "json-struct-unsupported-field",
        fix: None,
    })
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
    stmts.push(typed_var_decl(&acc_name, array_type(elem.clone(), pos), Expr::ArrayLit { multiline: false, elems: vec![], elem_type: None, pos }, true, pos));
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
) -> Result<(Vec<Stmt>, String), Box<CompileError>> {
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
                pos,
            ));
        }
        let raw_expr = prop_expr(json_call("asArray", vec![prop_expr(json_call("field", vec![v_expr, string_lit(field_name, pos)], pos), pos)], pos), pos);
        let stmts = gen_array_decode_stmts(raw_expr, elem, &result_var, TargetMode::Declare, pos, field_name);
        return Ok((stmts, result_var));
    }

    if let Some(inner) = optional_inner(t) {
        if !is_simple(inner, json_struct_names) && !matches!(inner, TypeNode::Array { .. }) {
            return Err(unsupported_field_error(
                struct_name,
                field_name,
                "the non-'none' side of this optional field isn't supported for automatic decoding",
                pos,
            ));
        }
        if let TypeNode::Array { elem, .. } = inner
            && !is_simple(elem, json_struct_names)
        {
            return Err(unsupported_field_error(
                struct_name,
                field_name,
                "array element type isn't supported for automatic decoding (only int/float/string/bool or a nested 'json struct')",
                pos,
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
        pos,
    ))
}

// 1つのjson struct宣言から decode<Name> のFnDeclを合成する
fn synthesize_decoder_fn(td: &TypeDecl, json_struct_names: &HashSet<String>) -> Result<FnDecl, Box<CompileError>> {
    let TypeNode::StructType { fields, .. } = &td.node else {
        // parserが"json type"を弾いているので通常は到達しない
        // **TS版に対応する診断コードが無いRust固有のガード**なので`syntax-error`を使う
        // (parserが"json type"を弾いているので通常は到達しない)
        return Err(Box::new(CompileError {
            message: format!("'json' can only mark a 'struct' declaration, not this type shape (found via '{}')", td.name),
            pos: td.pos,
            code: "syntax-error",
            fix: None,
        }));
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
    stmts.push(return_stmt(Some(Expr::StructLit { multiline: false, name: td.name.clone(), pkg: None, fields: field_values, pos }), pos));
    Ok(FnDecl {
        name: format!("decode{}", td.name),
        receiver: None,
        type_params: vec![],
        params: vec![Param { name: v_param.to_string(), type_node: TypeNode::Name { name: "Value".to_string(), pkg: Some("json".to_string()), pos }, pos }],
        ret: Some(union_type(vec![name_type(&td.name, pos), name_type("error", pos)], pos)),
        body: block(stmts),
        synthesized: true,
        exported: td.exported,
        pos,
    })
}

// ---- エンコード方向(milestone 66。TS版`json-decode.ts`の後半)-----------------------
// `json struct`宣言から、逆方向の `encode<X>(x: X) json.Value` も自動生成する。
// **decode側と対になっていて、失敗しうる箇所が無い**のが構造上の違い——`?`伝播も
// `TargetMode`の分岐も要らず、常に素直に組み立てるだけでよい。

// `json.Value{kind: "...", <extra>}` を組み立てる
fn json_value_struct_lit(kind_value: &str, extra_fields: Vec<StructLitField>, pos: Pos) -> Expr {
    let mut fields = vec![StructLitField { name: "kind".to_string(), value: string_lit(kind_value, pos), pos }];
    fields.extend(extra_fields);
    Expr::StructLit { multiline: false, name: "Value".to_string(), pkg: Some("json".to_string()), fields, pos }
}

fn json_value_type_node(pos: Pos) -> TypeNode {
    TypeNode::Name { name: "Value".to_string(), pkg: Some("json".to_string()), pos }
}

fn unsupported_encode_field_error(struct_name: &str, field_name: &str, reason: &str, pos: Pos) -> Box<CompileError> {
    Box::new(CompileError {
        message: format!("'json struct {struct_name}' can't auto-encode field '{field_name}': {reason}"),
        pos,
        code: "json-struct-unsupported-field",
        fix: None,
    })
}

// primitive/nestedな型の値をjson.Value式1つに変換する(`gen_simple_decode_expr`の裏返し)
fn gen_simple_encode_expr(value_expr: Expr, t: &TypeNode, pos: Pos) -> Expr {
    let TypeNode::Name { name, .. } = t else { unreachable!("gen_simple_encode_expr requires is_simple(t)") };
    match name.as_str() {
        "int" | "float" => json_value_struct_lit("num", vec![StructLitField { name: "n".to_string(), value: value_expr, pos }], pos),
        "string" => json_value_struct_lit("str", vec![StructLitField { name: "s".to_string(), value: value_expr, pos }], pos),
        "bool" => json_value_struct_lit("bool", vec![StructLitField { name: "b".to_string(), value: value_expr, pos }], pos),
        // ネストしたjson struct
        _ => call_expr(ident_expr(&format!("encode{name}"), pos), vec![value_expr], pos),
    }
}

// 配列を`acc_name`という名前の`json.Value[]`変数へループで組み立てる文一式
// (`gen_array_decode_stmts`の裏返し)
fn gen_array_encode_stmts(arr_expr: Expr, elem: &TypeNode, acc_name: &str, pos: Pos) -> Vec<Stmt> {
    let item_var = format!("__eitem_{}", acc_name.trim_start_matches("__earr_"));
    let mut stmts = Vec::new();
    stmts.push(typed_var_decl(
        acc_name,
        array_type(json_value_type_node(pos), pos),
        Expr::ArrayLit { multiline: false, elems: vec![], elem_type: None, pos },
        true,
        pos,
    ));
    let loop_body = block(vec![expr_stmt(
        call_expr(
            ident_expr("push", pos),
            vec![ident_expr(acc_name, pos), gen_simple_encode_expr(ident_expr(&item_var, pos), elem, pos)],
            pos,
        ),
        pos,
    )]);
    stmts.push(range_for_stmt(vec!["_".to_string(), item_var], arr_expr, loop_body, pos));
    stmts
}

// 1フィールド分の「値の取り出し+エンコード」文一式(`gen_field_stmts`の裏返し)。
// 戻り値のresult_exprは、呼び出し元がmapリテラルのentryへそのまま埋め込める式
fn gen_field_encode_stmts(
    struct_name: &str,
    x_expr: Expr,
    field_name: &str,
    t: &TypeNode,
    json_struct_names: &HashSet<String>,
    pos: Pos,
) -> Result<(Vec<Stmt>, Expr), Box<CompileError>> {
    let field_access = member_expr(x_expr, field_name, pos);

    if is_simple(t, json_struct_names) {
        return Ok((Vec::new(), gen_simple_encode_expr(field_access, t, pos)));
    }

    if let TypeNode::Array { elem, .. } = t {
        if !is_simple(elem, json_struct_names) {
            return Err(unsupported_encode_field_error(
                struct_name,
                field_name,
                "array element type isn't supported for automatic encoding (only int/float/string/bool or a nested 'json struct')",
                pos,
            ));
        }
        let acc_name = format!("__earr_{field_name}");
        let stmts = gen_array_encode_stmts(field_access, elem, &acc_name, pos);
        let result = json_value_struct_lit("arr", vec![StructLitField { name: "items".to_string(), value: ident_expr(&acc_name, pos), pos }], pos);
        return Ok((stmts, result));
    }

    if let Some(inner) = optional_inner(t) {
        let inner_array_elem = match inner {
            TypeNode::Array { elem, .. } => Some(&**elem),
            _ => None,
        };
        if !is_simple(inner, json_struct_names) && inner_array_elem.is_none() {
            return Err(unsupported_encode_field_error(
                struct_name,
                field_name,
                "the non-'none' side of this optional field isn't supported for automatic encoding",
                pos,
            ));
        }
        if let Some(elem) = inner_array_elem
            && !is_simple(elem, json_struct_names)
        {
            return Err(unsupported_encode_field_error(
                struct_name,
                field_name,
                "array element type isn't supported for automatic encoding (only int/float/string/bool or a nested 'json struct')",
                pos,
            ));
        }
        let field_val_var = format!("__efv_{field_name}");
        let result_var = format!("__ef_{field_name}");
        let mut stmts = Vec::new();
        stmts.push(short_var_decl(&field_val_var, field_access, pos));
        stmts.push(typed_var_decl(&result_var, json_value_type_node(pos), json_value_struct_lit("null", vec![], pos), true, pos));
        let inner_stmts = match inner_array_elem {
            Some(elem) => {
                let acc_name = format!("__earr_{field_name}");
                let mut v = gen_array_encode_stmts(ident_expr(&field_val_var, pos), elem, &acc_name, pos);
                v.push(assign_stmt(
                    &result_var,
                    json_value_struct_lit("arr", vec![StructLitField { name: "items".to_string(), value: ident_expr(&acc_name, pos), pos }], pos),
                    pos,
                ));
                v
            }
            None => vec![assign_stmt(&result_var, gen_simple_encode_expr(ident_expr(&field_val_var, pos), inner, pos), pos)],
        };
        stmts.push(if_stmt(
            not_expr(is_expr(ident_expr(&field_val_var, pos), name_type("none", pos), pos), pos),
            block(inner_stmts),
            pos,
        ));
        return Ok((stmts, ident_expr(&result_var, pos)));
    }

    Err(unsupported_encode_field_error(
        struct_name,
        field_name,
        "only int/float/string/bool, a nested 'json struct', an array of those, or 'T | none' of those are \
supported — write a hand-written encoder (building json.Value{...} directly) for this field instead",
        pos,
    ))
}

// 1つのjson struct宣言から encode<Name> のFnDeclを合成する
fn synthesize_encoder_fn(td: &TypeDecl, json_struct_names: &HashSet<String>) -> Result<FnDecl, Box<CompileError>> {
    let TypeNode::StructType { fields, .. } = &td.node else {
        // parserが"json type"を弾いているので通常は到達しない(synthesize_decoder_fnと同じ防御)
        return Err(Box::new(CompileError {
            message: format!("'json' can only mark a 'struct' declaration, not this type shape (found via '{}')", td.name),
            pos: td.pos,
            code: "syntax-error",
            fix: None,
        }));
    };
    let pos = td.pos;
    let x_param = "x";
    let mut stmts = Vec::new();
    let mut entries = Vec::new();
    for f in fields {
        let (field_stmts, result_expr) =
            gen_field_encode_stmts(&td.name, ident_expr(x_param, f.pos), &f.name, &f.type_node, json_struct_names, f.pos)?;
        stmts.extend(field_stmts);
        entries.push(MapLitEntry { key: string_lit(&f.name, f.pos), value: result_expr, pos: f.pos });
    }
    let entries_expr = Expr::MapLit {
        multiline: false,
        key: name_type("string", pos),
        value: json_value_type_node(pos),
        entries,
        pos,
    };
    stmts.push(return_stmt(
        Some(json_value_struct_lit("obj", vec![StructLitField { name: "entries".to_string(), value: entries_expr, pos }], pos)),
        pos,
    ));
    Ok(FnDecl {
        name: format!("encode{}", td.name),
        receiver: None,
        type_params: vec![],
        params: vec![Param { name: x_param.to_string(), type_node: name_type(&td.name, pos), pos }],
        ret: Some(json_value_type_node(pos)),
        body: block(stmts),
        synthesized: true,
        exported: td.exported,
        pos,
    })
}

// program中の全 json struct から encode<Name> 関数群を合成し、program.fnsへ追加する。
// `synthesize_json_decoders`と対の関数(呼び出し元で両方呼ぶ)。デコード成功に必要な制約
// (import・対応フィールド型)はエンコードでも同じだが、**decode側の実装都合に依存しない
// よう同じ検査をここでも行う**(TS版と同じ方針)
pub fn synthesize_json_encoders(program: &mut Program) -> Result<(), Box<CompileError>> {
    let json_struct_decls: Vec<TypeDecl> = program.types.iter().filter(|t| t.is_json).cloned().collect();
    if json_struct_decls.is_empty() {
        return Ok(());
    }
    let has_json_import = program.imports.iter().any(|i| i.path == "mesh/json");
    if !has_json_import {
        return Err(Box::new(CompileError {
            message: "'json struct' needs 'import \"mesh/json\"' (the generated encoder builds json.Value{...})".to_string(),
            pos: json_struct_decls[0].pos,
            code: "json-struct-missing-import",
            fix: None,
        }));
    }
    let json_struct_names: HashSet<String> = json_struct_decls.iter().map(|t| t.name.clone()).collect();
    // **手書き関数との名前衝突はここでは弾かない**——合成した`encode<Name>`をそのまま
    // program.fnsへ積めば、checkerが通常の`already-declared`として報告する(TS版と同じ)。
    // milestone 9当時はRust版がトップレベル関数名の重複を検出できなかったため専用の
    // ガードを置いていたが、milestone 48で`already-declared`が入って不要になった
    // (残しておくとTS版と違う診断コードになる。実測で発覚——milestone 66)
    for td in &json_struct_decls {
        let f = synthesize_encoder_fn(td, &json_struct_names)?;
        program.fns.push(f);
    }
    Ok(())
}

// program中の全 json struct から decode<Name> 関数群を合成し、program.fnsへ追加する。
// ネスト参照(struct内の別structフィールド)は同一ファイル内のjson structだけを対象にする
// (TS版と同じv1制約 — 他ファイル/他パッケージをまたぐ場合は手書きデコーダで対応する)
pub fn synthesize_json_decoders(program: &mut Program) -> Result<(), Box<CompileError>> {
    let json_struct_decls: Vec<TypeDecl> = program.types.iter().filter(|t| t.is_json).cloned().collect();
    if json_struct_decls.is_empty() {
        return Ok(());
    }
    let has_json_import = program.imports.iter().any(|i| i.path == "mesh/json");
    if !has_json_import {
        // 位置は最初のjson struct宣言(TS版も同じ。実測で確認)
        return Err(Box::new(CompileError {
            message: "'json struct' needs 'import \"mesh/json\"' (the generated decoder calls json.field/json.asString/etc.)".to_string(),
            pos: json_struct_decls[0].pos,
            code: "json-struct-missing-import",
            fix: None,
        }));
    }
    let json_struct_names: HashSet<String> = json_struct_decls.iter().map(|t| t.name.clone()).collect();
    // **手書き関数との名前衝突はここでは弾かない**——合成した`decode<Name>`をそのまま
    // program.fnsへ積めば、checkerが通常の`already-declared`として報告する(TS版と同じ)。
    // milestone 9のcode reviewでは「同じファイルにdecode<Name>が2つ定義された壊れたJSを
    // 静かに出力してしまう」ことを避けるため専用のガードを置いていたが、当時の前提だった
    // 「トップレベル関数名の重複を検出しない」はmilestone 48で解消済み。
    // 残しておくとTS版と違う診断コードになる(実測で発覚——milestone 66)
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
        assert!(err.message.contains("needs 'import \"mesh/json\"'"), "got: {}", err.message);
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
        assert!(err.message.contains("can't auto-decode field 'm'"), "got: {}", err.message);
    }

    #[test]
    fn 合成する関数名が既存の手書き関数と衝突しても合成自体は成功する() {
        // milestone 9のcode reviewでは、検出しないと同名の関数が2つ定義された壊れたJS
        // (SyntaxError)を静かに出力してしまうため専用のガードを置いていた。
        // **milestone 66でガードを外した**——当時の前提「トップレベル関数名の重複を
        // 検出しない」はmilestone 48で解消しており、そのまま積めばcheckerが通常の
        // `already-declared`として報告する(TS版と同じ。専用ガードを残すと診断コードが
        // TS版と食い違う、と実測で分かった)。
        // ここでは「合成が成功し、同名の関数が2つ並ぶ状態がcheckerへ渡る」ことを固定する
        let src = "import \"mesh/json\"\nfn decodeUser(v: json.Value) User | error {\n  return error(\"hand-written\")\n}\nfn main() {}\n";
        let mut program = parse(src).unwrap();
        program.types = vec![json_struct_decl("User", vec![field("name", name_type("string", pos()))])];
        synthesize_json_decoders(&mut program).unwrap();
        assert_eq!(program.fns.iter().filter(|f| f.name == "decodeUser").count(), 2, "手書きと合成の2つが並ぶ");
    }

    #[test]
    fn エンコーダも合成される() {
        // milestone 66。decodeの裏返しで`encode<Name>(x: X) json.Value`を作る
        let mut program = program_with(true, vec![json_struct_decl("User", vec![field("name", name_type("string", pos()))])]);
        synthesize_json_encoders(&mut program).unwrap();
        let f = program.fns.iter().find(|f| f.name == "encodeUser").expect("encodeUserが合成されること");
        assert_eq!(f.params.len(), 1);
        assert!(matches!(&f.ret, Some(TypeNode::Name { name, pkg: Some(p), .. }) if name == "Value" && p == "json"));
    }

    #[test]
    fn エンコーダもimport不足と未対応フィールドを弾く() {
        // decode側と同じ検査をencode側でも独立して行う(TS版と同じ方針——decode側の
        // 実装都合に依存させない)
        let mut program = program_with(false, vec![json_struct_decl("User", vec![field("name", name_type("string", pos()))])]);
        let err = synthesize_json_encoders(&mut program).unwrap_err();
        assert_eq!(err.code, "json-struct-missing-import");

        let mut program = program_with(
            true,
            vec![json_struct_decl("Bad", vec![field("m", TypeNode::MapType { key: Box::new(name_type("string", pos())), value: Box::new(name_type("int", pos())), pos: pos() })])],
        );
        let err = synthesize_json_encoders(&mut program).unwrap_err();
        assert_eq!(err.code, "json-struct-unsupported-field");
        assert!(err.message.contains("can't auto-encode field 'm'"), "got: {}", err.message);
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
        assert!(err.message.contains("array element type isn't supported"), "got: {}", err.message);
    }
}
