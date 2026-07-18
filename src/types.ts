// Checker が内部で使う「型」の表現。
// 2026-07-17 の背骨決定(union路線)により、不在は `T | none`、失敗は `T | error` の
// union 型で表現する。汎用の null は存在しない。

export type Type =
  | { kind: "prim"; name: "int" | "float" | "string" | "bool" | "void" | "error" }
  | { kind: "any" }
  | { kind: "none" } // 「不在」を表す単位型。T | none の形でだけ現れる
  | { kind: "closed" } // channelがcloseされたことを表す単位型。<-ch は常に T | closed を返す
  | { kind: "literal"; value: string } // 文字列リテラル型: "active"。string の部分型
  | { kind: "array"; elem: Type }
  | { kind: "chan"; elem: Type }
  | { kind: "map"; key: Type; value: Type } // map<string, int>。読みは V | none を返す
  | { kind: "fn"; params: Type[]; ret: Type }
  | { kind: "union"; members: Type[] }
  // struct User { name: string }。fields は再帰型(Node | none 等)を許すため
  // 宣言の解決時に後から埋められる(knot-tying)。v1 の同一性判定は名前ベース
  // (無名 {...} 型式が入るときに構造的比較へ拡張する)
  | { kind: "struct"; name: string; fields: StructField[] };

export interface StructField {
  name: string;
  type: Type;
}

export const INT: Type = { kind: "prim", name: "int" };
export const FLOAT: Type = { kind: "prim", name: "float" };
export const STRING: Type = { kind: "prim", name: "string" };
export const BOOL: Type = { kind: "prim", name: "bool" };
export const VOID: Type = { kind: "prim", name: "void" };
export const ERROR: Type = { kind: "prim", name: "error" };
export const ANY: Type = { kind: "any" };
export const NONE: Type = { kind: "none" };
export const CLOSED: Type = { kind: "closed" };

export function typeToString(t: Type): string {
  switch (t.kind) {
    case "prim":
      return t.name;
    case "any":
      return "any";
    case "none":
      return "none";
    case "closed":
      return "closed";
    case "literal":
      return JSON.stringify(t.value);
    case "array":
      return `${typeToString(t.elem)}[]`;
    case "chan":
      return `chan<${typeToString(t.elem)}>`;
    case "map":
      return `map<${typeToString(t.key)}, ${typeToString(t.value)}>`;
    case "fn":
      return `fn(${t.params.map(typeToString).join(", ")}) ${typeToString(t.ret)}`;
    case "union":
      return t.members.map(typeToString).join(" | ");
    case "struct":
      // 無名struct(判別可能unionのメンバー)は名前ではなく形を表示する。エラーメッセージで
      // 「missing: (anonymous)」のような読めない表示にならないようにするため
      return t.name === "(anonymous)"
        ? `{ ${t.fields.map((f) => `${f.name}: ${typeToString(f.type)}`).join(", ")} }`
        : t.name;
  }
}

// seen: 比較中の struct ペアを覚えておく(再帰struct同士の無限再帰を止めるための「知恵の輪」ガード。
// 一度比較を始めたペアはその場では「等しいと仮定」して先に進む — 等再帰型の同値判定の定石)
export function typeEquals(a: Type, b: Type, seen: Array<[Type, Type]> = []): boolean {
  if (a.kind !== b.kind) return false;
  switch (a.kind) {
    case "prim":
      return a.name === (b as typeof a).name;
    case "any":
    case "none":
    case "closed":
      return true;
    case "literal":
      return a.value === (b as typeof a).value;
    case "array":
    case "chan":
      return typeEquals(a.elem, (b as typeof a).elem, seen);
    case "map": {
      const bm = b as typeof a;
      return typeEquals(a.key, bm.key, seen) && typeEquals(a.value, bm.value, seen);
    }
    case "fn": {
      const bf = b as typeof a;
      return (
        a.params.length === bf.params.length &&
        typeEquals(a.ret, bf.ret, seen) &&
        a.params.every((p, i) => typeEquals(p, bf.params[i], seen))
      );
    }
    case "union": {
      const bu = b as typeof a;
      return (
        a.members.length === bu.members.length &&
        a.members.every((m) => bu.members.some((n) => typeEquals(m, n, seen)))
      );
    }
    case "struct": {
      // 構造的型付け(2026-07-17決定分の実装): struct の同一性は名前ではなく形で決まる。
      // 無名 {...} 型式(判別可能union のメンバー)も、名前付き struct 同士も同じ規則で比較する
      const bs = b as typeof a;
      if (a === bs) return true;
      if (seen.some(([sa, sb]) => sa === a && sb === bs)) return true;
      if (a.fields.length !== bs.fields.length) return false;
      const nextSeen: Array<[Type, Type]> = [...seen, [a, bs]];
      return a.fields.every((fa) => {
        const fb = bs.fields.find((f) => f.name === fa.name);
        return fb !== undefined && typeEquals(fa.type, fb.type, nextSeen);
      });
    }
  }
}

// メンバーの並びから union 型を作る(平坦化・重複除去・1個なら素の型に)
export function unionOf(members: Type[]): Type {
  const flat: Type[] = [];
  for (const m of members) {
    const candidates = m.kind === "union" ? m.members : [m];
    for (const c of candidates) {
      if (c.kind === "any") return ANY;
      if (!flat.some((f) => typeEquals(f, c))) flat.push(c);
    }
  }
  if (flat.length === 0) return VOID;
  if (flat.length === 1) return flat[0];
  return { kind: "union", members: flat };
}

// union から条件に合うメンバーを取り除く(narrowing の中核)
export function unionWithout(t: Type, remove: (m: Type) => boolean): Type {
  if (t.kind !== "union") return remove(t) ? VOID : t;
  return unionOf(t.members.filter((m) => !remove(m)));
}

// 「失敗」を表すメンバーか(none または error)。!/or の伝播対象はこれだけ
// (closed は「入力が終わった」であって「このコードが失敗した」ではないので含めない)
export function isFailure(t: Type): boolean {
  return t.kind === "none" || (t.kind === "prim" && t.name === "error");
}

// `is` で絞り込める対象か(none / error / closed)
export function isNarrowTarget(t: Type): boolean {
  return isFailure(t) || t.kind === "closed";
}

// from の値を to の場所に入れてよいか
export function assignable(from: Type, to: Type): boolean {
  if (from.kind === "any" || to.kind === "any") return true;
  // union へは「どれかのメンバーに入れられればよい」
  if (to.kind === "union") {
    if (from.kind === "union") return from.members.every((m) => assignable(m, to));
    return to.members.some((m) => assignable(from, m));
  }
  // union からは「全メンバーが入れられる場合のみ」(=事実上、絞り込みが必要)
  if (from.kind === "union") {
    return from.members.every((m) => assignable(m, to));
  }
  // 配列: 要素が any 側なら互換(空配列 [] = any[] を型付き配列へ入れる等)。
  // 具体型同士は typeEquals(下のフォールバック)なので int[] を string[] には入れられない
  if (from.kind === "array" && to.kind === "array") {
    if (from.elem.kind === "any" || to.elem.kind === "any") return true;
    return typeEquals(from.elem, to.elem);
  }
  // int は float に暗黙で広げられる(逆は不可)
  if (from.kind === "prim" && from.name === "int" && to.kind === "prim" && to.name === "float") {
    return true;
  }
  // リテラル型 "active" は string の部分型(逆は不可: string を "active" には入れられない)
  if (from.kind === "literal" && to.kind === "prim" && to.name === "string") {
    return true;
  }
  return typeEquals(from, to);
}

export function isNumeric(t: Type): boolean {
  return t.kind === "any" || (t.kind === "prim" && (t.name === "int" || t.name === "float"));
}

// string として扱えるか(string 本体またはリテラル型)
export function isStringy(t: Type): boolean {
  return t.kind === "literal" || (t.kind === "prim" && t.name === "string");
}

// リテラル型を string に広げる(mut 宣言・配列要素の推論で使う)
export function widenLiteral(t: Type): Type {
  return t.kind === "literal" ? STRING : t;
}
