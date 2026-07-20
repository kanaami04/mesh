// 型解決: 型注釈(構文)→内部表現の変換、type宣言のエイリアス解決(knot-tying)、
// 判別可能unionのタグ判定、error type/structのタグ付け

import type { TypeNode } from "../ast";
import type { Pos } from "../token";
import {
  ANY,
  BOOL,
  CLOSED,
  ERROR,
  FLOAT,
  INT,
  NONE,
  STRING,
  VOID,
  isFailure,
  typeToString,
  unionOf,
  type Type,
} from "../types";
import { checkFieldName, error, isTypeParam, type CheckerCtx } from "./context";

// 型注釈(構文)を内部表現の型へ変換
export function resolveType(ctx: CheckerCtx, node: TypeNode): Type {
  switch (node.kind) {
    case "array":
      return { kind: "array", elem: resolveType(ctx, node.elem) };
    case "chan":
      return { kind: "chan", elem: resolveType(ctx, node.elem) };
    case "mapType":
      return { kind: "map", key: resolveType(ctx, node.key), value: resolveType(ctx, node.value) };
    case "union":
      return unionOf(node.members.map((m) => resolveType(ctx, m)));
    case "literal":
      return { kind: "literal", value: node.value };
    case "structType": {
      // 名前なし文脈で来た場合(通常は resolveAlias 経由で来る)
      for (const f of node.fields) checkFieldName(ctx, f.name, f.pos);
      return {
        kind: "struct",
        name: "(anonymous)",
        fields: node.fields.map((f) => ({ name: f.name, type: resolveType(ctx, f.type) })),
      };
    }
    case "fnType":
      return {
        kind: "fn",
        params: node.params.map((p) => resolveType(ctx, p)),
        ret: node.ret ? resolveType(ctx, node.ret) : VOID,
      };
    case "name":
      // math.User — importしたパッケージのexported型
      if (node.pkg) return resolvePackageType(ctx, node.pkg, node.name, node.pos);
      switch (node.name) {
        case "int": return INT;
        case "float": return FLOAT;
        case "string": return STRING;
        case "bool": return BOOL;
        case "void": return VOID;
        case "error": return ERROR;
        case "none": return NONE;
        case "closed": return CLOSED;
        case "any": return ANY;
        default:
          // ジェネリック関数(F-1後半)の型パラメータ: fn first<T>(...) の T のような名前は
          // 通常のtype宣言より先に「今アクティブな型パラメータか」を確認する
          if (isTypeParam(ctx, node.name)) return { kind: "typeParam", name: node.name };
          return resolveAlias(ctx, node.name, node.pos);
      }
  }
}

// importしたパッケージのexported型を引く(math.User / math.Status)
export function resolvePackageType(ctx: CheckerCtx, pkg: string, name: string, pos: Pos): Type {
  if (!ctx.importAliases.has(pkg)) {
    error(ctx, pos, "unknown-package", `unknown package '${pkg}' — add: import "${pkg}"`);
    return ANY;
  }
  const symbols = ctx.registry.get(pkg);
  const entry = symbols?.types.get(name);
  if (!entry) {
    error(ctx, pos, "unknown-package-type", `package '${pkg}' has no type '${name}'`);
    return ANY;
  }
  if (!entry.exported) {
    error(
      ctx,
      pos,
      "not-exported",
      `'${name}' is not exported by package '${pkg}' — add 'export' to its declaration`,
    );
    return ANY;
  }
  return entry.type;
}

// type 宣言された名前の解決(メモ化+循環検出)
export function resolveAlias(ctx: CheckerCtx, name: string, pos: Pos): Type {
  const memo = ctx.resolvedAliases.get(name);
  if (memo) return memo;
  const node = ctx.typeTable.get(name);
  if (!node) {
    error(ctx, pos, "unknown-type", `unknown type '${name}'`);
    return ANY;
  }
  // struct は「先に器を登録 → 後からフィールドを埋める」(knot-tying)。
  // これにより struct Node { next: Node | none } のような再帰型が書ける。
  // struct名は "main" 以外では pkg.Name に修飾する(表示・メソッド表のキーが
  // パッケージ間で衝突しないように。同一性は構造的なので意味論には影響しない)
  if (node.kind === "structType") {
    const displayName = ctx.pkg === "main" ? name : `${ctx.pkg}.${name}`;
    const struct: Type = { kind: "struct", name: displayName, fields: [] };
    ctx.resolvedAliases.set(name, struct);
    for (const f of node.fields) {
      checkFieldName(ctx, f.name, f.pos);
      struct.fields.push({ name: f.name, type: resolveType(ctx, f.type) });
    }
    if (ctx.errorTypeNames.has(name)) tagErrorMembers(ctx, struct, displayName, name, pos);
    return struct;
  }
  // union も同じ知恵の輪(knot-tying)で解決する。判別可能unionが自分自身を struct フィールド
  // 越しに参照する再帰型(木構造など: { kind: "leaf" } | { kind: "node", left: Tree, right: Tree })
  // を許すため。ただし「struct/array等に包まれない裸のunion参照同士の相互再帰」
  // (例: type A = B | none  type B = A | error)は、flatten時に相手がまだ空のplaceholderで
  // 型情報が消える不具合が過去にあったため、今まで通り循環エラーにする。
  // 完成した union は必ずメンバー2個以上を持つ(unionOfが1個以下を単独の型に潰すため)ので、
  // 「kind: "union" かつ members が空」は「まだ解決中のplaceholderが裸で出てきた」ことの
  // 確実な目印になる
  if (node.kind === "union") {
    const union: Type = { kind: "union", members: [] };
    ctx.resolvedAliases.set(name, union);
    const rawMembers = node.members.map((m) => resolveType(ctx, m));
    const unsafe = rawMembers.find((m) => m.kind === "union" && m.members.length === 0);
    if (unsafe) {
      error(ctx, pos, "type-alias-cycle", `type alias cycle involving '${name}'`);
      ctx.resolvedAliases.set(name, ANY);
      return ANY;
    }
    const flattened = unionOf(rawMembers);
    if (flattened.kind === "any") {
      ctx.resolvedAliases.set(name, ANY);
      return ANY;
    }
    union.members = flattened.kind === "union" ? flattened.members : [flattened];
    if (ctx.errorTypeNames.has(name)) tagErrorMembers(ctx, union, null, name, pos);
    // F-7: 判別可能union(C-1の無名{...}メンバーが2個以上)は必ずタグフィールドを持つ。
    // 構築時(structLit)はこのタグの値だけを見てメンバーを特定する(フィールド集合は見ない)。
    // 名前付きstruct同士のunion(type Shape = Circle | Square)はそれぞれ自分の名前で構築される
    // ので対象外 — フィールド集合の遠隔作用が起きるのは「union自身の名前で構築する無名メンバー」
    // だけであり、名前付きメンバーには元々この曖昧さが無い
    const anonymousMembers = union.members.filter(
      (m): m is Type & { kind: "struct" } => m.kind === "struct" && m.name === "(anonymous)",
    );
    if (anonymousMembers.length >= 2) {
      const tag = findDiscriminantTag(anonymousMembers);
      if (tag === null) {
        error(
          ctx,
          pos,
          "discriminated-union-tag-required",
          `discriminated union '${name}' needs a tag field — every struct member must share one ` +
            `field with a distinct string-literal value (e.g. kind: "...") so a member can be ` +
            `identified from its tag alone, without comparing against the other members (F-7)`,
        );
      } else {
        union.discriminantTag = tag;
      }
    }
    return union;
  }
  if (ctx.resolvingAliases.has(name)) {
    error(ctx, pos, "type-alias-cycle", `type alias cycle involving '${name}'`);
    return ANY;
  }
  ctx.resolvingAliases.add(name);
  const resolved = resolveType(ctx, node);
  ctx.resolvingAliases.delete(name);
  ctx.resolvedAliases.set(name, resolved);
  if (ctx.errorTypeNames.has(name)) tagErrorMembers(ctx, resolved, null, name, pos);
  return resolved;
}

// F-7: 判別可能unionのタグフィールド名を求める。「全メンバーに存在し、リテラル型(文字列
// リテラルのみが存在)で、値が互いに異なる」フィールドが1つでもあればそれを使う(複数の
// 候補があっても最初に見つかったものでよい — どれを選んでも局所解決という性質は変わらない)。
// 無ければ null(判別可能unionとして構築できない)
export function findDiscriminantTag(members: (Type & { kind: "struct" })[]): string | null {
  for (const name of members[0].fields.map((f) => f.name)) {
    const fields = members.map((m) => m.fields.find((f) => f.name === name));
    if (fields.some((f) => f === undefined || f.type.kind !== "literal")) continue;
    const values = fields.map((f) => (f!.type as Type & { kind: "literal" }).value);
    if (new Set(values).size === members.length) return name;
  }
  return null;
}

// error type/struct 宣言(F-2後半)で名付けられたエイリアスの各メンバーに isErrorType を立てる。
// 「このエイリアス専用に今まさに作られた struct」だけが対象(無名 {...} 由来、または
// struct宣言直下の器そのもの=freshName)。既存の名前付き型への参照にタグを付けると、
// その型が使われる他の場所すべてに漏れてしまうので、そこは拒否する
export function tagErrorMembers(ctx: CheckerCtx, t: Type, freshName: string | null, declName: string, pos: Pos) {
  const members = t.kind === "union" ? t.members : [t];
  for (const m of members) {
    if (m.kind !== "struct") {
      error(
        ctx,
        pos,
        "error-type-must-be-struct",
        `error type '${declName}' members must be struct-shaped (like a discriminated union) — got ${typeToString(m)}`,
      );
    } else if (m.name === "(anonymous)" || m.name === freshName) {
      m.isErrorType = true;
    } else {
      error(
        ctx,
        pos,
        "error-type-aliases-existing",
        `error type '${declName}' can't tag the existing type '${m.name}' — use an inline struct ` +
          `shape ({ ... }) or declare a fresh 'error struct ${m.name} { ... }' instead`,
      );
    }
  }
}

// '?'/'or' の伝播対象か: 組み込みのnone/error(isFailure)に加えて、F-2後半のerror
// type/structでタグ付けされた構造化エラーもここで「失敗」として扱う
export function isFailureType(t: Type): boolean {
  return isFailure(t) || (t.kind === "struct" && t.isErrorType === true);
}

export function fnType(ctx: CheckerCtx, params: { type: TypeNode }[], ret: TypeNode | null): Type {
  return {
    kind: "fn",
    params: params.map((p) => resolveType(ctx, p.type)),
    ret: ret ? resolveType(ctx, ret) : VOID,
  };
}
