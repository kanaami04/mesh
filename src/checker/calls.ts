// 呼び出し解決: math.add(...) のようなパッケージ修飾関数、recv.method(...) のような
// メソッド、ただの関数値の呼び出し。引数照合の共通部分(checkArgsAgainst)もここに集約する
// (checkCallArgs / checkCallOfValue / ジェネリック呼び出しがここへ合流する)

import type { Expr, MemberExpr } from "../ast";
import { ANY, assignable, typeToString, type Type } from "../types";
import { BUILTINS, error, lookup, type CheckerCtx } from "./context";
import { checkExprSingle } from "./expressions";
import { inferBuiltinCall } from "./builtins";
import { inferGenericCall } from "./generics";

// math.add のようなパッケージ修飾メンバー参照の解決を試みる。
// target がimportしたパッケージ名(かつローカル束縛に無い)ときだけ成立し、
// exported関数の型を返す(codegen用に resolvedPkg も書き込む)。それ以外は null
export function tryPackageMember(ctx: CheckerCtx, member: MemberExpr): Type | null {
  if (member.target.kind !== "ident") return null;
  const alias = member.target.name;
  if (lookup(ctx, alias) !== undefined) return null; // ローカル束縛が優先(declareが衝突を防いでいる)
  if (!ctx.importAliases.has(alias)) return null;
  const symbols = ctx.registry.get(alias);
  // F-9c: 関数だけでなくトップレベル定数(pkg.constName)も同じ経路で解決する
  const fn = symbols?.fns.get(member.name) ?? symbols?.consts.get(member.name);
  if (!fn) {
    if (symbols?.types.get(member.name)?.exported) {
      error(
        ctx,
        member.pos,
        "package-symbol-is-a-type",
        `'${member.name}' is a type — use ${alias}.${member.name} in a type position, or ${alias}.${member.name}{...} to construct it`,
      );
    } else {
      error(
        ctx,
        member.pos,
        "unknown-package-function",
        `package '${alias}' has no exported function or constant '${member.name}'`,
      );
    }
    member.resolvedPkg = alias;
    return ANY;
  }
  if (!fn.exported) {
    error(
      ctx,
      member.pos,
      "not-exported",
      `'${member.name}' is not exported by package '${alias}' — add 'export' to its declaration`,
    );
    member.resolvedPkg = alias;
    return ANY;
  }
  member.resolvedPkg = alias;
  return fn.type;
}

export function inferCall(ctx: CheckerCtx, expr: Expr & { kind: "call" }): Type {
  // 組み込み関数(シャドーイング禁止なので名前で判定できる)
  if (expr.callee.kind === "ident" && BUILTINS.has(expr.callee.name)) {
    return inferBuiltinCall(ctx, expr.callee.name, expr);
  }

  // ジェネリック関数(F-1後半): name(args) の直接呼び出しだけ対応(shadowing禁止なので
  // 名前で判定できる。変数へ代入してから呼ぶ・コールバックとして渡す等は非対応 — その場合
  // 型パラメータがtypeParamのまま残り、代入不可として自然にエラーになる)
  if (expr.callee.kind === "ident" && ctx.genericFns.has(expr.callee.name)) {
    return inferGenericCall(ctx, expr, expr.callee.name);
  }

  // math.add(args) — パッケージ修飾の関数呼び出しを先に解決する
  if (expr.callee.kind === "member") {
    const pkgFn = tryPackageMember(ctx, expr.callee);
    if (pkgFn) {
      expr.callee.resolvedType = pkgFn;
      return checkCallOfValue(ctx, expr, pkgFn);
    }
  }

  // recv.method(args) — struct のメソッドとして解決を試みる。
  // target の型はここで一度だけ評価する(呼び出し不成立時のフォールバックでも
  // 再評価しない。二重評価すると undefined 変数などのエラーが2回出てしまう)
  if (expr.callee.kind === "member") {
    const member = expr.callee;
    const targetType = checkExprSingle(ctx, member.target);

    if (targetType.kind === "struct" && !targetType.fields.some((f) => f.name === member.name)) {
      const methodType = ctx.methodTable.get(targetType.name)?.get(member.name);
      if (methodType && methodType.kind === "fn") {
        member.resolvedType = methodType;
        const [, ...paramsWithoutReceiver] = methodType.params;
        return checkCallArgs(ctx, expr, paramsWithoutReceiver, methodType.ret);
      }
      error(
        ctx,
        member.pos,
        "unknown-field",
        `${typeToString(targetType)} has no field or method '${member.name}'` +
          ` (fields: ${targetType.fields.map((f) => f.name).join(", ") || "none"})`,
      );
      member.resolvedType = ANY;
      expr.args.forEach((a) => checkExprSingle(ctx, a)); // 引数側もチェックしてエラーの連鎖を減らす
      return ANY;
    }

    // メソッド対象でなければ、member を通常どおり「値」として評価し、それを呼び出す
    // (struct フィールドが関数値のケース・union未絞り込み・非structなど)
    const memberType = memberFieldType(ctx, member, targetType);
    member.resolvedType = memberType;
    return checkCallOfValue(ctx, expr, memberType);
  }

  const callee = checkExprSingle(ctx, expr.callee);
  return checkCallOfValue(ctx, expr, callee);
}

// struct フィールドアクセスの型検査(member式・メソッド呼び出しの両方から使う共通部分)。
// target の型は呼び出し元が(二重評価を避けるため)既に確定させて渡す
export function memberFieldType(ctx: CheckerCtx, member: MemberExpr, targetType: Type): Type {
  if (targetType.kind === "struct") {
    const field = targetType.fields.find((f) => f.name === member.name);
    if (field) return field.type;
    if (ctx.methodTable.get(targetType.name)?.has(member.name)) {
      error(ctx, member.pos, "method-not-called", `'${member.name}' is a method — call it like ${member.name}(...)`);
      return ANY;
    }
    error(
      ctx,
      member.pos,
      "unknown-field",
      `${typeToString(targetType)} has no field '${member.name}'` +
        ` (fields: ${targetType.fields.map((f) => f.name).join(", ")})`,
    );
    return ANY;
  }
  if (targetType.kind === "union") {
    error(
      ctx,
      member.pos,
      "narrow-required",
      `cannot access field or method on ${typeToString(targetType)} — narrow it first (with 'is' or 'match')`,
    );
    return ANY;
  }
  if (targetType.kind !== "any") {
    error(ctx, member.pos, "not-a-struct", `${typeToString(targetType)} has no fields`);
  }
  return ANY;
}

// 引数(検査済みの型)を paramTypes と照合する共通部分。checkCallArgs / checkCallOfValue /
// ジェネリック呼び出し(inferGenericCall)がここに集約する
export function checkArgsAgainst(
  ctx: CheckerCtx,
  callExpr: Expr & { kind: "call" },
  args: Type[],
  paramTypes: Type[],
  retType: Type,
): Type {
  if (args.length !== paramTypes.length) {
    error(ctx, callExpr.pos, "argument-count", `expected ${paramTypes.length} argument(s), got ${args.length}`);
  }
  for (let i = 0; i < Math.min(args.length, paramTypes.length); i++) {
    if (!assignable(args[i], paramTypes[i])) {
      error(
        ctx,
        callExpr.args[i].pos,
        "type-mismatch",
        `argument ${i + 1}: cannot use ${typeToString(args[i])} as ${typeToString(paramTypes[i])}`,
      );
    }
  }
  return retType;
}

// 引数リストを既知の paramTypes と照合する(メソッド呼び出し用。callee自体は常にfnなので
// 「呼べない型」チェックは不要)
export function checkCallArgs(
  ctx: CheckerCtx,
  callExpr: Expr & { kind: "call" },
  paramTypes: Type[],
  retType: Type,
): Type {
  const args = callExpr.args.map((a) => checkExprSingle(ctx, a));
  return checkArgsAgainst(ctx, callExpr, args, paramTypes, retType);
}

// callee の型が分かっている状態からの呼び出し検査(通常の関数呼び出し・
// structフィールドが関数値のケースの両方で使う)
export function checkCallOfValue(
  ctx: CheckerCtx,
  callExpr: Expr & { kind: "call" },
  calleeType: Type,
): Type {
  const args = callExpr.args.map((a) => checkExprSingle(ctx, a));
  if (calleeType.kind === "any") return ANY;
  if (calleeType.kind !== "fn") {
    error(ctx, callExpr.pos, "not-callable", `cannot call non-function type ${typeToString(calleeType)}`);
    return ANY;
  }
  return checkArgsAgainst(ctx, callExpr, args, calleeType.params, calleeType.ret);
}
