// narrowing(F-6): is/match/&&/||/! から「このパスはこの型に絞り込まれる」という
// 事実(facts)を集め、代入で古くなった事実を捨てる。判別可能unionの部分構造パターン一致
// (structPatternMatches)もここに置く — is式(inferExprのcase "is")とmatch(match-select.ts)
// の両方がこれを使う

import type { Expr } from "../ast";
import { typeEquals, unionOf, type Type } from "../types";
import { lookup, type CheckerCtx } from "./context";

// narrowing(F-6)の対象になりうる「安定パス」: 不変な変数から始まる ident/フィールド
// アクセスの連鎖("n" / "n.next" / "n.next.left" ...)。mut変数はいつでも再代入されうる
// ので対象外(rootが不変でも中間の構造体フィールドは代入可能 — その場合は代入文の側で
// invalidatePath して古い事実を捨てる)
export function stablePath(ctx: CheckerCtx, expr: Expr): string | null {
  if (expr.kind === "ident") {
    const binding = lookup(ctx, expr.name);
    return binding && !binding.mutable ? expr.name : null;
  }
  if (expr.kind === "member") {
    const base = stablePath(ctx, expr.target);
    return base === null ? null : `${base}.${expr.name}`;
  }
  return null;
}

// x.f = ... / x = ... のような代入は、重なるパスの narrowing 事実を古くしうるので捨てる。
// 代入先そのもの・その子パス(x を代入 → x.f も無効)だけを消す。祖先パス(x.f.g を代入
// したときの x.f)はそのまま残す — 中間パスの型自体は変わらないため
export function invalidatePath(ctx: CheckerCtx, path: string) {
  for (const scope of ctx.scopes) {
    for (const key of scope.keys()) {
      if (key.includes(".") && (key === path || key.startsWith(`${path}.`))) {
        scope.delete(key);
      }
    }
  }
}

export function noFacts(): { then: Map<string, Type>; else: Map<string, Type> } {
  return { then: new Map(), else: new Map() };
}

// 条件式から then側/else側それぞれで成り立つ事実(パス→絞り込み型)を再帰的に集める。
// 呼び出す前に cond(を含む式)は checkExprSingle 済みで、resolvedType / isTargetTypes が
// 埋まっている前提(ここでは resolveType 等の副作用のある呼び出しを一切しない — 診断の
// 重複を避けるため)
export function collectFacts(ctx: CheckerCtx, expr: Expr): { then: Map<string, Type>; else: Map<string, Type> } {
  if (expr.kind === "is") {
    const path = stablePath(ctx, expr.operand);
    const opType = expr.operand.resolvedType;
    const target = ctx.isTargetTypes.get(expr);
    if (path === null || !opType || opType.kind !== "union" || !target) return noFacts();
    const matched = opType.members.filter((m) => structPatternMatches(m, target));
    if (matched.length === 0) return noFacts(); // 「can never be」は is の式検査が報告済み
    const rest = opType.members.filter((m) => !matched.includes(m));
    return { then: new Map([[path, unionOf(matched)]]), else: new Map([[path, unionOf(rest)]]) };
  }

  if (expr.kind === "unary" && expr.op === "!") {
    // ! はド・モルガン: 内側の then/else をそのまま入れ替える
    const inner = collectFacts(ctx, expr.operand);
    return { then: inner.else, else: inner.then };
  }

  if (expr.kind === "binary" && (expr.op === "&&" || expr.op === "||")) {
    const left = collectFacts(ctx, expr.left);
    const right = collectFacts(ctx, expr.right);
    // && の then側 = 両方成り立つ(積) / || の else側 = 両方不成立(積、ド・モルガン)。
    // 逆側(&&のelse、||のthen)は一般に単一パスの型へ畳めない(OR)ので事実を作らない
    return expr.op === "&&"
      ? { then: andFacts(left.then, right.then), else: new Map() }
      : { then: new Map(), else: andFacts(left.else, right.else) };
  }

  return noFacts();
}

// 2組の事実が同時に成り立つ場合を合成する。同じパスに複数の絞り込みが重なったら、
// 両方を満たすメンバーの積を取る(全滅すれば「その分岐は到達不能」= 空union = VOID)
export function andFacts(a: Map<string, Type>, b: Map<string, Type>): Map<string, Type> {
  if (a.size === 0) return b;
  if (b.size === 0) return a;
  const out = new Map(a);
  for (const [path, t] of b) {
    const prev = out.get(path);
    out.set(path, prev ? intersectTypes(prev, t) : t);
  }
  return out;
}

export function intersectTypes(a: Type, b: Type): Type {
  const am = a.kind === "union" ? a.members : [a];
  const bm = b.kind === "union" ? b.members : [b];
  return unionOf(am.filter((m) => bm.some((m2) => typeEquals(m, m2))));
}

// 判別可能union用: パターンが構造体型メンバーの「部分形」として一致するか。
// パターンに書かれたフィールドが全部あって型が一致すればよい(書かれてないフィールドは無視。
// { kind: "ok" } は user フィールドの有無を問わず kind: "ok" を持つメンバーに一致する)
export function structPatternMatches(member: Type, pattern: Type): boolean {
  if (member.kind !== "struct" || pattern.kind !== "struct") return typeEquals(member, pattern);
  return pattern.fields.every((pf) => {
    const mf = member.fields.find((f) => f.name === pf.name);
    return mf !== undefined && typeEquals(mf.type, pf.type);
  });
}
