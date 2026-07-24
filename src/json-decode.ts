// H-2(2026-07-21): `json struct X { ... }` は decode<X>(v: json.Value) X | error を自動生成する。
// design-agenda.md J節(2026-07-24提起・kanayamaと討議のうえ選択肢(a)を採用): 同じ
// `json struct`宣言から、逆方向の encode<X>(x: X) json.Value も自動生成する(decode<X>との
// 対称性を優先——H-2時点はデコードのみで、demo/todo-api実装でエンコード側の欠落が
// ボイラープレートとして顕在化した)。エンコードは検証を伴わない(Mesh側は既に型付き
// なので失敗し得ない)ため、戻り値はunionではなく素の`json.Value`。
//
// アプローチ: 生JSを手組みするのではなく、Meshの構文レベルのAST(Stmt/Expr)を合成し、
// 通常のFnDeclとしてprogram.fnsへ追加する。こうすることで、以降のcheck/codegenの経路は
// 一切変更せずそのまま流用でき(生成した関数も普通の関数として型検査・コード生成される)、
// json.field/json.asString等のヘルパー(stdlib.ts+runtime.ts)を'?'で繋ぐだけの
// 「手書きデコーダと全く同じ形」のコードを機械的に組み立てる。エンコード側も同じ
// 「AST合成」方式で、json.Value{kind:...}のstruct literalを組み立てる。
//
// 対応するフィールド型(v1スコープ、デコード・エンコード共通): int/float/string/bool、
// 他のjson struct(同一ファイル内)への参照、それらの配列、それらの'T | none'。それ以外
// (素のstruct・map・一般unionなど)は合成時にエラーにし、手書きの変換(json.field等を
// 直接使うデコーダ/json.Value{...}を直接組み立てるエンコーダ)を書くよう誘導する。

import type { Block, Expr, FnDecl, Program, Stmt, TypeDecl, TypeNode } from "./ast";
import { CompileError, MultiCompileError } from "./token";
import type { Pos } from "./token";

const PRIMITIVE_HELPERS: Record<string, string> = {
  int: "asInt",
  float: "asFloat",
  string: "asString",
  bool: "asBool",
};

// ---- AST合成の小さな部品 ----

function identExpr(name: string, pos: Pos): Expr {
  return { kind: "ident", name, pos };
}
function stringLit(value: string, pos: Pos): Expr {
  return { kind: "string", value, pos };
}
function noneExpr(pos: Pos): Expr {
  return { kind: "none", pos };
}
function memberExpr(target: Expr, name: string, pos: Pos): Expr {
  return { kind: "member", target, name, pos };
}
function callExpr(callee: Expr, args: Expr[], pos: Pos): Expr {
  return { kind: "call", callee, args, pos };
}
function propExpr(operand: Expr, pos: Pos): Expr {
  return { kind: "prop", operand, pos };
}
function isExpr(operand: Expr, target: TypeNode, pos: Pos): Expr {
  return { kind: "is", operand, target, pos };
}
function notExpr(operand: Expr, pos: Pos): Expr {
  return { kind: "unary", op: "!", operand, pos };
}
function jsonCall(fnName: string, args: Expr[], pos: Pos): Expr {
  return callExpr(memberExpr(identExpr("json", pos), fnName, pos), args, pos);
}
function block(stmts: Stmt[]): Block {
  return { kind: "block", stmts };
}
function shortVarDecl(name: string, value: Expr, pos: Pos, mutable = false): Stmt {
  return { kind: "shortVarDecl", names: [name], values: [value], mutable, pos };
}
function typedVarDecl(name: string, typeNode: TypeNode, value: Expr, mutable: boolean, pos: Pos): Stmt {
  return { kind: "typedVarDecl", name, typeNode, value, mutable, pos };
}
function assignStmt(name: string, value: Expr, pos: Pos): Stmt {
  return { kind: "assign", targets: [identExpr(name, pos)], values: [value], pos };
}
function exprStmt(expr: Expr, pos: Pos): Stmt {
  return { kind: "exprStmt", expr, pos };
}
function returnStmt(value: Expr | null, pos: Pos): Stmt {
  return { kind: "return", value, pos };
}
function ifStmt(cond: Expr, then: Block, pos: Pos): Stmt {
  return { kind: "if", cond, then, else_: null, pos };
}
function rangeForStmt(names: string[], subject: Expr, body: Block, pos: Pos): Stmt {
  return { kind: "rangeFor", names, subject, body, pos };
}
function nameType(name: string, pos: Pos): TypeNode {
  return { kind: "name", name, pos };
}
function arrayType(elem: TypeNode, pos: Pos): TypeNode {
  return { kind: "array", elem, pos };
}
function unionType(members: TypeNode[], pos: Pos): TypeNode {
  return { kind: "union", members, pos };
}

function unsupportedFieldError(structName: string, fieldName: string, reason: string, pos: Pos): CompileError {
  return new CompileError(
    `'json struct ${structName}' can't auto-decode field '${fieldName}': ${reason}`,
    pos,
    "json-struct-unsupported-field",
  );
}

function isPrimitive(t: TypeNode): t is TypeNode & { kind: "name" } {
  return t.kind === "name" && !t.pkg && t.name in PRIMITIVE_HELPERS;
}
function isNestedJsonStruct(t: TypeNode, jsonStructNames: Set<string>): t is TypeNode & { kind: "name" } {
  return t.kind === "name" && !t.pkg && jsonStructNames.has(t.name);
}
function isSimple(t: TypeNode, jsonStructNames: Set<string>): boolean {
  return isPrimitive(t) || isNestedJsonStruct(t, jsonStructNames);
}
// 'T | none' の形だけを対象にする(2メンバーちょうど、片方がnone)
function optionalInner(t: TypeNode): TypeNode | null {
  if (t.kind !== "union" || t.members.length !== 2) return null;
  const noneIdx = t.members.findIndex((m) => m.kind === "name" && !m.pkg && m.name === "none");
  if (noneIdx === -1) return null;
  return t.members[1 - noneIdx];
}

// primitive/nested な型を、既に取り出し済みのjson.Value式(rawExpr)からデコードする
// 「式1つ」を作る(文は不要 — json.asXxx(...)?  /  decode<Name>(...)? のどちらか)
function genSimpleDecodeExpr(rawExpr: Expr, t: TypeNode & { kind: "name" }, jsonStructNames: Set<string>, pos: Pos): Expr {
  if (t.name in PRIMITIVE_HELPERS) {
    return propExpr(jsonCall(PRIMITIVE_HELPERS[t.name], [rawExpr], pos), pos);
  }
  return propExpr(callExpr(identExpr(`decode${t.name}`, pos), [rawExpr], pos), pos);
}

// 配列フィールドのデコード文一式を作る(ループで1つずつ組み立てる)。
// targetMode: "declare"なら`mut <target>: elem[] = []`から新規に、"assign"なら既存のmut変数へ
// 最終代入する(optionalの中で使う — 一時変数に組み立ててから代入する)
function genArrayDecodeStmts(
  rawArrayExpr: Expr,
  elem: TypeNode,
  target: string,
  targetMode: "declare" | "assign",
  jsonStructNames: Set<string>,
  pos: Pos,
  uid: string,
): Stmt[] {
  const rawArrName = `__raw_arr_${uid}`;
  const itemVar = `__item_${uid}`;
  const decodedVar = `__decoded_${uid}`;
  const accName = targetMode === "declare" ? target : `__acc_${uid}`;
  const stmts: Stmt[] = [];
  stmts.push(shortVarDecl(rawArrName, rawArrayExpr, pos));
  stmts.push(typedVarDecl(accName, arrayType(elem, pos), { kind: "arrayLit", elems: [], pos }, true, pos));
  if (!isSimple(elem, jsonStructNames)) {
    // 呼び出し元(analyzeとgen両方)で弾いているはずだが、念のための防御
    throw new Error("unreachable: unsupported array element type reached codegen");
  }
  const loopBody = block([
    shortVarDecl(
      decodedVar,
      genSimpleDecodeExpr(identExpr(itemVar, pos), elem as TypeNode & { kind: "name" }, jsonStructNames, pos),
      pos,
    ),
    exprStmt(callExpr(identExpr("push", pos), [identExpr(accName, pos), identExpr(decodedVar, pos)], pos), pos),
  ]);
  stmts.push(rangeForStmt(["_", itemVar], identExpr(rawArrName, pos), loopBody, pos));
  if (targetMode === "assign") {
    stmts.push(assignStmt(target, identExpr(accName, pos), pos));
  }
  return stmts;
}

// 1フィールド分の「取り出し+デコード」文一式を作る。戻り値のresultVarは、後で
// struct リテラルを組み立てるときに参照する変数名
function genFieldStmts(
  structName: string,
  vExpr: Expr,
  fieldName: string,
  t: TypeNode,
  jsonStructNames: Set<string>,
  pos: Pos,
): { stmts: Stmt[]; resultVar: string } {
  const resultVar = `__f_${fieldName}`;

  if (isSimple(t, jsonStructNames)) {
    const rawExpr = propExpr(jsonCall("field", [vExpr, stringLit(fieldName, pos)], pos), pos);
    const valueExpr = genSimpleDecodeExpr(rawExpr, t as TypeNode & { kind: "name" }, jsonStructNames, pos);
    return { stmts: [shortVarDecl(resultVar, valueExpr, pos)], resultVar };
  }

  if (t.kind === "array") {
    if (!isSimple(t.elem, jsonStructNames)) {
      throw unsupportedFieldError(
        structName,
        fieldName,
        "array element type isn't supported for automatic decoding (only int/float/string/bool or a nested 'json struct')",
        pos,
      );
    }
    const rawExpr = propExpr(
      jsonCall("asArray", [propExpr(jsonCall("field", [vExpr, stringLit(fieldName, pos)], pos), pos)], pos),
      pos,
    );
    const stmts = genArrayDecodeStmts(rawExpr, t.elem, resultVar, "declare", jsonStructNames, pos, fieldName);
    return { stmts, resultVar };
  }

  const inner = optionalInner(t);
  if (inner) {
    if (!isSimple(inner, jsonStructNames) && inner.kind !== "array") {
      throw unsupportedFieldError(
        structName,
        fieldName,
        "the non-'none' side of this optional field isn't supported for automatic decoding",
        pos,
      );
    }
    if (inner.kind === "array" && !isSimple(inner.elem, jsonStructNames)) {
      throw unsupportedFieldError(
        structName,
        fieldName,
        "array element type isn't supported for automatic decoding (only int/float/string/bool or a nested 'json struct')",
        pos,
      );
    }
    const rawVar = `__raw_${fieldName}`;
    const stmts: Stmt[] = [];
    stmts.push(shortVarDecl(rawVar, jsonCall("optField", [vExpr, stringLit(fieldName, pos)], pos), pos));
    stmts.push(typedVarDecl(resultVar, unionType([inner, nameType("none", pos)], pos), noneExpr(pos), true, pos));
    const rawIdent = identExpr(rawVar, pos);
    const innerStmts =
      inner.kind === "array"
        ? genArrayDecodeStmts(
            propExpr(jsonCall("asArray", [rawIdent], pos), pos),
            inner.elem,
            resultVar,
            "assign",
            jsonStructNames,
            pos,
            fieldName,
          )
        : [assignStmt(resultVar, genSimpleDecodeExpr(rawIdent, inner as TypeNode & { kind: "name" }, jsonStructNames, pos), pos)];
    stmts.push(ifStmt(notExpr(isExpr(rawIdent, nameType("none", pos), pos), pos), block(innerStmts), pos));
    return { stmts, resultVar };
  }

  throw unsupportedFieldError(
    structName,
    fieldName,
    "only int/float/string/bool, a nested 'json struct', an array of those, or 'T | none' of those are " +
      "supported — write a hand-written decoder (using json.field/json.asString/etc.) for this field instead",
    pos,
  );
}

// 1つのjson struct宣言から decode<Name> のFnDeclを合成する
function synthesizeDecoderFn(td: TypeDecl, jsonStructNames: Set<string>): FnDecl {
  if (td.node.kind !== "structType") {
    // parserが"json type"を弾いているので通常は到達しない
    throw new CompileError(
      `'json' can only mark a 'struct' declaration, not this type shape`,
      td.pos,
      "json-type-not-supported",
    );
  }
  const pos = td.pos;
  const vParam = "v";
  const stmts: Stmt[] = [];
  const fieldValues: { name: string; value: Expr; pos: Pos }[] = [];
  for (const f of td.node.fields) {
    const { stmts: fieldStmts, resultVar } = genFieldStmts(
      td.name,
      identExpr(vParam, f.pos),
      f.name,
      f.type,
      jsonStructNames,
      f.pos,
    );
    stmts.push(...fieldStmts);
    fieldValues.push({ name: f.name, value: identExpr(resultVar, f.pos), pos: f.pos });
  }
  stmts.push(
    returnStmt(
      { kind: "structLit", name: td.name, fields: fieldValues, pos },
      pos,
    ),
  );
  return {
    kind: "fnDecl",
    name: `decode${td.name}`,
    receiver: null,
    typeParams: [],
    params: [{ name: vParam, type: { kind: "name", name: "Value", pkg: "json", pos }, pos }],
    ret: unionType([nameType(td.name, pos), nameType("error", pos)], pos),
    body: block(stmts),
    exported: td.exported,
    pos,
  };
}

// program中の全 json struct から decode<Name> 関数群を合成し、program.fnsへ追加する。
// ネスト参照(struct内の別structフィールド)は同一ファイル内のjson structだけを対象にする
// (v1制約 — 他ファイル/他パッケージをまたぐ場合は手書きデコーダで対応する)
export function synthesizeJsonDecoders(program: Program): void {
  const jsonStructDecls = program.types.filter((t) => t.isJson);
  if (jsonStructDecls.length === 0) return;
  const hasJsonImport = program.imports.some((i) => i.path === "mesh/json");
  if (!hasJsonImport) {
    throw new CompileError(
      "'json struct' needs 'import \"mesh/json\"' (the generated decoder calls json.field/json.asString/etc.)",
      jsonStructDecls[0].pos,
      "json-struct-missing-import",
    );
  }
  const jsonStructNames = new Set(jsonStructDecls.map((t) => t.name));
  const errors: CompileError[] = [];
  for (const td of jsonStructDecls) {
    try {
      program.fns.push(synthesizeDecoderFn(td, jsonStructNames));
    } catch (e) {
      if (e instanceof CompileError) errors.push(e);
      else throw e;
    }
  }
  if (errors.length === 1) throw errors[0];
  if (errors.length > 1) throw new MultiCompileError(errors); // compiler.ts側は既にこれを処理できる
}

// ---- ここからJ節: エンコード方向(decodeの裏返し) ----

function jsonValueTypeNode(pos: Pos): TypeNode {
  return { kind: "name", name: "Value", pkg: "json", pos };
}
function jsonValueStructLit(kindValue: string, extraFields: { name: string; value: Expr; pos: Pos }[], pos: Pos): Expr {
  return {
    kind: "structLit",
    name: "Value",
    pkg: "json",
    fields: [{ name: "kind", value: stringLit(kindValue, pos), pos }, ...extraFields],
    pos,
  };
}

function unsupportedEncodeFieldError(structName: string, fieldName: string, reason: string, pos: Pos): CompileError {
  return new CompileError(
    `'json struct ${structName}' can't auto-encode field '${fieldName}': ${reason}`,
    pos,
    "json-struct-unsupported-field",
  );
}

// primitive/nested な型の値(valueExpr)をjson.Value式1つに変換する(genSimpleDecodeExprの裏返し)
function genSimpleEncodeExpr(valueExpr: Expr, t: TypeNode & { kind: "name" }, pos: Pos): Expr {
  if (t.name === "int" || t.name === "float") return jsonValueStructLit("num", [{ name: "n", value: valueExpr, pos }], pos);
  if (t.name === "string") return jsonValueStructLit("str", [{ name: "s", value: valueExpr, pos }], pos);
  if (t.name === "bool") return jsonValueStructLit("bool", [{ name: "b", value: valueExpr, pos }], pos);
  // nested json struct
  return callExpr(identExpr(`encode${t.name}`, pos), [valueExpr], pos);
}

// 配列(arrExpr: elem[])を、accNameという名前のjson.Value[]変数へループで組み立てる文一式
// (genArrayDecodeStmtsの裏返し。デコードと違い「途中で失敗する」ことが無いので、
// targetMode/'?'伝播のようなdecode側の分岐は不要——常にループで素直に組み立てるだけでよい)
function genArrayEncodeStmts(arrExpr: Expr, elem: TypeNode & { kind: "name" }, accName: string, pos: Pos, uid: string): Stmt[] {
  const itemVar = `__eitem_${uid}`;
  const stmts: Stmt[] = [];
  stmts.push(typedVarDecl(accName, arrayType(jsonValueTypeNode(pos), pos), { kind: "arrayLit", elems: [], pos }, true, pos));
  const loopBody = block([
    exprStmt(callExpr(identExpr("push", pos), [identExpr(accName, pos), genSimpleEncodeExpr(identExpr(itemVar, pos), elem, pos)], pos), pos),
  ]);
  stmts.push(rangeForStmt(["_", itemVar], arrExpr, loopBody, pos));
  return stmts;
}

// 1フィールド分の「値の取り出し+エンコード」文一式を作る(genFieldStmtsの裏返し)。
// 戻り値のresultExprは、呼び出し元がmap literalのentryへそのまま埋め込める式
// (statementが不要な単純な場合は式1つ、配列/optionalはstatementで一時変数に組み立ててから
// その変数への参照を返す)
function genFieldEncodeStmts(
  structName: string,
  xExpr: Expr,
  fieldName: string,
  t: TypeNode,
  jsonStructNames: Set<string>,
  pos: Pos,
): { stmts: Stmt[]; resultExpr: Expr } {
  const fieldAccess = memberExpr(xExpr, fieldName, pos);

  if (isSimple(t, jsonStructNames)) {
    return { stmts: [], resultExpr: genSimpleEncodeExpr(fieldAccess, t as TypeNode & { kind: "name" }, pos) };
  }

  if (t.kind === "array") {
    if (!isSimple(t.elem, jsonStructNames)) {
      throw unsupportedEncodeFieldError(
        structName,
        fieldName,
        "array element type isn't supported for automatic encoding (only int/float/string/bool or a nested 'json struct')",
        pos,
      );
    }
    const accName = `__earr_${fieldName}`;
    const stmts = genArrayEncodeStmts(fieldAccess, t.elem as TypeNode & { kind: "name" }, accName, pos, fieldName);
    return { stmts, resultExpr: jsonValueStructLit("arr", [{ name: "items", value: identExpr(accName, pos), pos }], pos) };
  }

  const inner = optionalInner(t);
  if (inner) {
    if (!isSimple(inner, jsonStructNames) && inner.kind !== "array") {
      throw unsupportedEncodeFieldError(
        structName,
        fieldName,
        "the non-'none' side of this optional field isn't supported for automatic encoding",
        pos,
      );
    }
    if (inner.kind === "array" && !isSimple(inner.elem, jsonStructNames)) {
      throw unsupportedEncodeFieldError(
        structName,
        fieldName,
        "array element type isn't supported for automatic encoding (only int/float/string/bool or a nested 'json struct')",
        pos,
      );
    }
    const fieldValVar = `__efv_${fieldName}`;
    const resultVar = `__ef_${fieldName}`;
    const stmts: Stmt[] = [];
    stmts.push(shortVarDecl(fieldValVar, fieldAccess, pos));
    stmts.push(typedVarDecl(resultVar, jsonValueTypeNode(pos), jsonValueStructLit("null", [], pos), true, pos));
    const fieldValIdent = identExpr(fieldValVar, pos);
    const innerStmts =
      inner.kind === "array"
        ? [
            ...genArrayEncodeStmts(fieldValIdent, inner.elem as TypeNode & { kind: "name" }, `__earr_${fieldName}`, pos, fieldName),
            assignStmt(
              resultVar,
              jsonValueStructLit("arr", [{ name: "items", value: identExpr(`__earr_${fieldName}`, pos), pos }], pos),
              pos,
            ),
          ]
        : [assignStmt(resultVar, genSimpleEncodeExpr(fieldValIdent, inner as TypeNode & { kind: "name" }, pos), pos)];
    stmts.push(ifStmt(notExpr(isExpr(fieldValIdent, nameType("none", pos), pos), pos), block(innerStmts), pos));
    return { stmts, resultExpr: identExpr(resultVar, pos) };
  }

  throw unsupportedEncodeFieldError(
    structName,
    fieldName,
    "only int/float/string/bool, a nested 'json struct', an array of those, or 'T | none' of those are " +
      "supported — write a hand-written encoder (building json.Value{...} directly) for this field instead",
    pos,
  );
}

// 1つのjson struct宣言から encode<Name> のFnDeclを合成する
function synthesizeEncoderFn(td: TypeDecl, jsonStructNames: Set<string>): FnDecl {
  if (td.node.kind !== "structType") {
    // parserが"json type"を弾いているので通常は到達しない(synthesizeDecoderFnと同じ防御)
    throw new CompileError(
      `'json' can only mark a 'struct' declaration, not this type shape`,
      td.pos,
      "json-type-not-supported",
    );
  }
  const pos = td.pos;
  const xParam = "x";
  const xExpr = identExpr(xParam, pos);
  const stmts: Stmt[] = [];
  const mapEntries: { key: Expr; value: Expr; pos: Pos }[] = [];
  for (const f of td.node.fields) {
    const { stmts: fieldStmts, resultExpr } = genFieldEncodeStmts(td.name, xExpr, f.name, f.type, jsonStructNames, f.pos);
    stmts.push(...fieldStmts);
    mapEntries.push({ key: stringLit(f.name, f.pos), value: resultExpr, pos: f.pos });
  }
  const entriesExpr: Expr = {
    kind: "mapLit",
    key: { kind: "name", name: "string", pos },
    value: jsonValueTypeNode(pos),
    entries: mapEntries,
    pos,
  };
  stmts.push(returnStmt(jsonValueStructLit("obj", [{ name: "entries", value: entriesExpr, pos }], pos), pos));
  return {
    kind: "fnDecl",
    name: `encode${td.name}`,
    receiver: null,
    typeParams: [],
    params: [{ name: xParam, type: { kind: "name", name: td.name, pos }, pos }],
    ret: jsonValueTypeNode(pos),
    body: block(stmts),
    exported: td.exported,
    pos,
  };
}

// program中の全 json struct から encode<Name> 関数群を合成し、program.fnsへ追加する。
// synthesizeJsonDecodersと対の関数(呼び出し元のcompiler.tsで両方呼ぶ)。デコード成功に
// 必要な制約(import・対応フィールド型)はエンコードでも同じなので、decode成功後なら
// 通常はエンコードも成功するが、関数として独立して自己完結させる(decode側の実装都合に
// 依存しない)ため同じ検査をここでも行う
export function synthesizeJsonEncoders(program: Program): void {
  const jsonStructDecls = program.types.filter((t) => t.isJson);
  if (jsonStructDecls.length === 0) return;
  const hasJsonImport = program.imports.some((i) => i.path === "mesh/json");
  if (!hasJsonImport) {
    throw new CompileError(
      "'json struct' needs 'import \"mesh/json\"' (the generated encoder builds json.Value{...})",
      jsonStructDecls[0].pos,
      "json-struct-missing-import",
    );
  }
  const jsonStructNames = new Set(jsonStructDecls.map((t) => t.name));
  const errors: CompileError[] = [];
  for (const td of jsonStructDecls) {
    try {
      program.fns.push(synthesizeEncoderFn(td, jsonStructNames));
    } catch (e) {
      if (e instanceof CompileError) errors.push(e);
      else throw e;
    }
  }
  if (errors.length === 1) throw errors[0];
  if (errors.length > 1) throw new MultiCompileError(errors);
}
