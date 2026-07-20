// match式・select式。どちらも「複数アームから1つを選び、アーム内で対象を絞り込む」という
// 同じ形をしているのでまとめて置く

import type { Expr } from "../ast";
import { ANY, CLOSED, VOID, typeEquals, typeToString, unionOf, type Type } from "../types";
import { declareBinding, error, popScope, pushScope, type CheckerCtx } from "./context";
import { resolveType } from "./types-resolve";
import { stablePath, structPatternMatches } from "./narrowing";
import { checkExpr, checkExprSingle } from "./expressions";

// match式: 型パターンで union を分解する。網羅性検査とアーム内 narrowing はここ
export function inferMatch(ctx: CheckerCtx, expr: Expr & { kind: "match" }): Type {
  const subject = checkExprSingle(ctx, expr.subject);
  if (expr.arms.length === 0) {
    error(ctx, expr.pos, "empty-match", "match must have at least one arm");
    return ANY;
  }
  if (subject.kind !== "union" && subject.kind !== "any") {
    error(
      ctx,
      expr.subject.pos,
      "union-required",
      `match subject must be a union type, got ${typeToString(subject)}`,
    );
  }
  const members = subject.kind === "union" ? subject.members : null;

  // 対象が安定パス(不変な変数、またはそこからのフィールドアクセス)なら、アーム内で絞り込める
  const narrowPath = stablePath(ctx, expr.subject);

  const covered: Type[] = [];
  const armTypes: Type[] = [];
  let sawWildcard = false;

  for (const arm of expr.arms) {
    if (sawWildcard) {
      error(ctx, arm.pos, "unreachable-pattern", "unreachable arm — '_' already matches everything before this");
      continue;
    }
    const patternTypes: Type[] = [];
    for (const p of arm.patterns) {
      if (p.kind === "wildcard") {
        if (arm.patterns.length > 1) {
          error(ctx, p.pos, "wildcard-not-alone", "'_' must be the only pattern in its arm");
        }
        sawWildcard = true;
        continue;
      }
      const pt = resolveType(ctx, p.type);
      // 判別可能union: { kind: "ok" } のような部分構造パターンは、書かれたフィールドが
      // 一致する union メンバー(具体的な形)へ解決してから、通常の型パターンと同じに扱う
      // (1個のパターンが複数メンバーに一致することもある — その場合は両方カバーしたことにする)。
      // 通常の型パターン(int/error/...)は今まで通り「union の実メンバーか」をそのまま検査する
      let resolvedPatterns: Type[];
      if (pt.kind === "struct" && members) {
        resolvedPatterns = members.filter((m) => structPatternMatches(m, pt));
      } else if (members && !members.some((m) => typeEquals(m, pt))) {
        resolvedPatterns = [];
      } else {
        resolvedPatterns = [pt];
      }
      if (members && resolvedPatterns.length === 0) {
        error(ctx, arm.pos, "impossible-pattern", `${typeToString(subject)} can never be ${typeToString(pt)}`);
      }
      for (const rp of resolvedPatterns) {
        if (members && covered.some((c) => typeEquals(c, rp))) {
          error(ctx, arm.pos, "unreachable-pattern", `unreachable pattern — ${typeToString(rp)} is already covered`);
        }
        covered.push(rp);
        patternTypes.push(rp);
      }
    }

    // アーム内の型: 型パターンならその union、ワイルドカードなら「残り全部」
    const narrowedTo = sawWildcard && patternTypes.length === 0 && members
      ? unionOf(members.filter((m) => !covered.some((c) => typeEquals(c, m))))
      : unionOf(patternTypes.length > 0 ? patternTypes : [ANY]);

    pushScope(ctx);
    if (narrowPath) {
      ctx.scopes[ctx.scopes.length - 1].set(narrowPath, { type: narrowedTo, mutable: false });
    }
    armTypes.push(checkExpr(ctx, arm.body));
    popScope(ctx);
  }

  // 網羅性検査: union の全メンバーがカバーされているか
  if (members && !sawWildcard) {
    const missing = members.filter((m) => !covered.some((c) => typeEquals(c, m)));
    if (missing.length > 0) {
      error(
        ctx,
        expr.pos,
        "match-not-exhaustive",
        `match is not exhaustive — missing: ${missing.map((t) => typeToString(t)).join(", ")}` +
          ` (add arms for them, or a '_' arm)`,
      );
    }
  }

  // 結果型: 全アーム void なら void(文として使う)、そうでなければアームの union
  const voids = armTypes.filter((t) => typeEquals(t, VOID));
  if (voids.length === armTypes.length) return VOID;
  if (voids.length > 0) {
    error(ctx, expr.pos, "mixed-void-arms", "match arms mix values and void — all arms must return a value, or none");
    return ANY;
  }
  return unionOf(armTypes);
}

// select式: 複数チャネルのうちどれかが準備できたら、そのアームを評価する。
// matchと見た目は揃えるが、パターンは「型」ではなく「どのチャネル操作が先に終わったか」
export function inferSelect(ctx: CheckerCtx, expr: Expr & { kind: "select" }): Type {
  if (expr.arms.length === 0 && !expr.defaultArm) {
    error(ctx, expr.pos, "empty-select", "select must have at least one arm");
    return ANY;
  }
  const armTypes: Type[] = [];
  for (const arm of expr.arms) {
    const chType = checkExprSingle(ctx, arm.channel);
    let bindingType: Type = ANY;
    if (chType.kind === "chan") {
      bindingType = unionOf([chType.elem, CLOSED]);
    } else if (chType.kind !== "any") {
      error(ctx, arm.channel.pos, "not-a-channel", `select arm requires a channel, got ${typeToString(chType)}`);
    }
    pushScope(ctx);
    declareBinding(ctx, arm.name, bindingType, arm.pos);
    armTypes.push(checkExpr(ctx, arm.body));
    popScope(ctx);
  }
  if (expr.defaultArm) {
    armTypes.push(checkExpr(ctx, expr.defaultArm));
  }

  const voids = armTypes.filter((t) => typeEquals(t, VOID));
  if (voids.length === armTypes.length) return VOID;
  if (voids.length > 0) {
    error(ctx, expr.pos, "mixed-void-arms", "select arms mix values and void — all arms must return a value, or none");
    return ANY;
  }
  return unionOf(armTypes);
}
