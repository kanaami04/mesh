// ジェネリック関数(F-1後半)。宣言時の検査(validateTypeParams)と、呼び出し時の型推論
// (unifyTypeParam → substituteTypeParams → 通常の引数照合)がここに集まる

import type { Expr, FnDecl } from "../ast";
import { ANY, typeEquals, unionOf, type Type } from "../types";
import { BUILTIN_TYPE_NAMES, error, type CheckerCtx } from "./context";
import { checkArgsAgainst } from "./calls";
import { checkExprSingle } from "./expressions";

// 宣言時の検査: 型パラメータ名の衝突と、「呼び出し側の引数から推論できる位置に
// 最低1回は現れているか」(型パラメータが戻り値型にしか現れないと呼び出し側で
// 推論しようがないので、宣言の時点で拒否する — 毎回の呼び出しで謎エラーになるのを防ぐ)
export function validateTypeParams(ctx: CheckerCtx, fn: FnDecl, fnT: Type) {
  const seen = new Set<string>();
  for (const name of fn.typeParams) {
    if (BUILTIN_TYPE_NAMES.has(name)) {
      error(ctx, fn.pos, "generic-type-param-conflict", `type parameter '${name}' shadows a builtin type name`);
    } else if (ctx.typeTable.has(name)) {
      error(
        ctx,
        fn.pos,
        "generic-type-param-conflict",
        `type parameter '${name}' conflicts with an existing type '${name}'`,
      );
    } else if (seen.has(name)) {
      error(ctx, fn.pos, "generic-type-param-conflict", `type parameter '${name}' is declared more than once`);
    }
    seen.add(name);
  }
  if (fnT.kind !== "fn") return;
  for (const name of fn.typeParams) {
    if (!fnT.params.some((p) => typeContainsParam(p, name))) {
      error(
        ctx,
        fn.pos,
        "generic-type-param-not-inferable",
        `type parameter '${name}' must appear in a parameter type — it can't be inferred from ` +
          `the call site otherwise (e.g. 'T[]', not just as the return type)`,
      );
    }
  }
}

// unifyTypeParam が実際に辿れる形とそろえた「Tがこの型の中に(推論可能な位置で)現れるか」。
// union の中(T | error 等)も辿る — unifyTypeParamも同じ形を辿れるようにしたので、
// ここだけ辿らないと「宣言は通るのに呼び出しは毎回推論失敗する」という罠になる
export function typeContainsParam(t: Type, name: string): boolean {
  switch (t.kind) {
    case "typeParam":
      return t.name === name;
    case "array":
    case "chan":
      return typeContainsParam(t.elem, name);
    case "map":
      return typeContainsParam(t.key, name) || typeContainsParam(t.value, name);
    case "fn":
      return t.params.some((p) => typeContainsParam(p, name)) || typeContainsParam(t.ret, name);
    case "union":
      return t.members.some((m) => typeContainsParam(m, name));
    default:
      return false;
  }
}

// typeContainsParamの「特定の名前」版ではなく「何らかの型パラメータを含むか」だけを見る版。
// union内のどのメンバーが型パラメータを含む側かを仕分けるのに使う(unifyTypeParam参照)
export function containsAnyTypeParam(t: Type): boolean {
  switch (t.kind) {
    case "typeParam":
      return true;
    case "array":
    case "chan":
      return containsAnyTypeParam(t.elem);
    case "map":
      return containsAnyTypeParam(t.key) || containsAnyTypeParam(t.value);
    case "fn":
      return t.params.some((p) => containsAnyTypeParam(p)) || containsAnyTypeParam(t.ret);
    case "union":
      return t.members.some((m) => containsAnyTypeParam(m));
    default:
      return false;
  }
}

// 呼び出し引数から型パラメータを推論する。paramType(Tを含みうる)とargType(検査済みの
// 具体型)を並行に辿り、typeParamに初めて出会った位置の実引数型をそのまま束縛する
// (2回目以降の出現は上書きしない — 食い違いは後段のcheckArgsAgainstが通常の代入不可
// エラーとして報告する)。
export function unifyTypeParam(paramType: Type, argType: Type, bindings: Map<string, Type>) {
  if (paramType.kind === "typeParam") {
    if (!bindings.has(paramType.name)) bindings.set(paramType.name, argType);
    return;
  }
  if (paramType.kind === "array" && argType.kind === "array") {
    unifyTypeParam(paramType.elem, argType.elem, bindings);
  } else if (paramType.kind === "chan" && argType.kind === "chan") {
    unifyTypeParam(paramType.elem, argType.elem, bindings);
  } else if (paramType.kind === "map" && argType.kind === "map") {
    unifyTypeParam(paramType.key, argType.key, bindings);
    unifyTypeParam(paramType.value, argType.value, bindings);
  } else if (paramType.kind === "fn" && argType.kind === "fn") {
    const n = Math.min(paramType.params.length, argType.params.length);
    for (let i = 0; i < n; i++) unifyTypeParam(paramType.params[i], argType.params[i], bindings);
    unifyTypeParam(paramType.ret, argType.ret, bindings);
  } else if (paramType.kind === "union") {
    // T | error のようなunionを実引数の型と照合する。argTypeがunionでなければ
    // (例: firstNonNone(5) の 5 は素の int)1メンバーのunionとして扱う — T | none に
    // 素の値を渡すのは正当な呼び方であり、union同士のときだけ推論できるのでは足りない。
    // paramType側の「型パラメータを含まないメンバー」はargType側の対応するメンバーを
    // 消費するだけ(値自体の食い違いは後段のcheckArgsAgainstに任せる)。残ったargType側の
    // メンバーを、型パラメータを含む側のメンバー(通常は裸のTひとつ)へ割り当てる
    const argMembers = argType.kind === "union" ? argType.members : [argType];
    const varMembers = paramType.members.filter((m) => containsAnyTypeParam(m));
    const concreteMembers = paramType.members.filter((m) => !containsAnyTypeParam(m));
    const usedArgMembers = new Set<Type>();
    for (const cm of concreteMembers) {
      const match = argMembers.find((am) => !usedArgMembers.has(am) && typeEquals(cm, am));
      if (match) usedArgMembers.add(match);
    }
    const remaining = argMembers.filter((am) => !usedArgMembers.has(am));
    if (varMembers.length === 1 && remaining.length > 0) {
      unifyTypeParam(varMembers[0], remaining.length === 1 ? remaining[0] : unionOf(remaining), bindings);
    } else if (varMembers.length > 1) {
      // 型パラメータを含むメンバーが複数ある稀なケース: 順番に対応させるベストエフォート
      varMembers.forEach((vm, i) => {
        if (remaining[i]) unifyTypeParam(vm, remaining[i], bindings);
      });
    }
  }
}

// 集めた束縛を型に適用してtypeParamを具体型へ置き換える
export function substituteTypeParams(t: Type, bindings: Map<string, Type>): Type {
  switch (t.kind) {
    case "typeParam":
      return bindings.get(t.name) ?? ANY;
    case "array":
      return { kind: "array", elem: substituteTypeParams(t.elem, bindings) };
    case "chan":
      return { kind: "chan", elem: substituteTypeParams(t.elem, bindings) };
    case "map":
      return {
        kind: "map",
        key: substituteTypeParams(t.key, bindings),
        value: substituteTypeParams(t.value, bindings),
      };
    case "union":
      return unionOf(t.members.map((m) => substituteTypeParams(m, bindings)));
    case "fn":
      return {
        kind: "fn",
        params: t.params.map((p) => substituteTypeParams(p, bindings)),
        ret: substituteTypeParams(t.ret, bindings),
      };
    default:
      return t;
  }
}

// ジェネリック関数の直接呼び出し: 引数から型パラメータを推論 → paramTypes/retTypeへ
// 代入 → 通常の呼び出しと同じ照合(checkArgsAgainst)に合流する
export function inferGenericCall(ctx: CheckerCtx, expr: Expr & { kind: "call" }, name: string): Type {
  const generic = ctx.genericFns.get(name);
  if (!generic || generic.type.kind !== "fn") return ANY; // 到達しないはず(登録時にfn型のみ入れる)
  const { typeParams, type: fnT } = generic;

  const args = expr.args.map((a) => checkExprSingle(ctx, a));
  const bindings = new Map<string, Type>();
  const n = Math.min(args.length, fnT.params.length);
  for (let i = 0; i < n; i++) unifyTypeParam(fnT.params[i], args[i], bindings);

  const missing = typeParams.filter((p) => !bindings.has(p));
  if (missing.length > 0) {
    error(
      ctx,
      expr.pos,
      "generic-inference-failed",
      `cannot infer type parameter(s) ${missing.map((m) => `'${m}'`).join(", ")} of '${name}' from these arguments`,
    );
    return ANY;
  }

  const paramTypes = fnT.params.map((p) => substituteTypeParams(p, bindings));
  const retType = substituteTypeParams(fnT.ret, bindings);
  return checkArgsAgainst(ctx, expr, args, paramTypes, retType);
}
