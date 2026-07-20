// 関数・メソッド宣言: メソッドシグネチャの登録(declareMethod)と、関数本体の検査の入口(checkFn)

import type { FnDecl, FnExpr } from "../ast";
import { VOID, typeToString } from "../types";
import { declareBinding, error, popScope, popTypeParams, pushScope, pushTypeParams, BUILTINS, type CheckerCtx } from "./context";
import { fnType, resolveType } from "./types-resolve";
import { checkBlock } from "./statements";

// fn (u: User) describe() ... のシグネチャを methodTable に登録する(グローバルscopeには置かない)
export function declareMethod(ctx: CheckerCtx, fn: FnDecl) {
  if (!fn.receiver) return;
  const recvType = resolveType(ctx, fn.receiver.type);
  if (recvType.kind !== "struct") {
    error(
      ctx,
      fn.receiver.pos,
      "invalid-receiver-type",
      `method receiver must be a struct type, got ${typeToString(recvType)}`,
    );
    return;
  }
  if (BUILTINS.has(fn.name)) {
    error(ctx, fn.pos, "builtin-redeclared", `'${fn.name}' is a builtin function and cannot be used as a method name`);
    return;
  }
  if (recvType.fields.some((f) => f.name === fn.name)) {
    error(ctx, fn.pos, "method-field-conflict", `${recvType.name} already has a field named '${fn.name}'`);
    return;
  }
  let methods = ctx.methodTable.get(recvType.name);
  if (!methods) {
    methods = new Map();
    ctx.methodTable.set(recvType.name, methods);
  }
  if (methods.has(fn.name)) {
    error(ctx, fn.pos, "duplicate-method", `${recvType.name} already has a method named '${fn.name}'`);
    return;
  }
  const base = fnType(ctx, fn.params, fn.ret);
  if (base.kind !== "fn") return; // 到達しない(fnTypeは常にkind:"fn"を返す)
  methods.set(fn.name, { kind: "fn", params: [recvType, ...base.params], ret: base.ret });
}

export function checkFn(ctx: CheckerCtx, fn: FnDecl | FnExpr) {
  pushScope(ctx);
  // ジェネリック関数(F-1後半)の本体・パラメータ・戻り値型からも <T> の T を参照できるように
  pushTypeParams(ctx, fn.kind === "fnDecl" ? fn.typeParams : []);
  if (fn.kind === "fnDecl" && fn.receiver) {
    declareBinding(ctx, fn.receiver.name, resolveType(ctx, fn.receiver.type), fn.receiver.pos);
  }
  for (const p of fn.params) declareBinding(ctx, p.name, resolveType(ctx, p.type), p.pos);
  ctx.retStack.push(fn.ret ? resolveType(ctx, fn.ret) : VOID);
  checkBlock(ctx, fn.body);
  ctx.retStack.pop();
  popTypeParams(ctx);
  popScope(ctx);
}
