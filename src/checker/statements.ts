// 文の検査。narrowing(F-6)の facts をブロック/if文に適用する部分もここ

import type { Block, Expr, Stmt } from "../ast";
import { ANY, BOOL, INT, VOID, assignable, isNumeric, typeEquals, typeToString, widenLiteral, type Type } from "../types";
import type { Pos } from "../token";
import { declareBinding, error, lookup, popScope, pushScope, type CheckerCtx } from "./context";
import { checkArithOp, checkExpr, checkExprSingle } from "./expressions";
import { collectFacts, invalidatePath, stablePath } from "./narrowing";
import { resolveType } from "./types-resolve";

// facts: narrowing 用に「このブロック内だけパス(変数名 or フィールドパス)の型を差し替える」
export function checkBlock(ctx: CheckerCtx, block: Block, facts?: Map<string, Type>) {
  pushScope(ctx);
  if (facts) applyFacts(ctx, facts);
  for (const stmt of block.stmts) checkStmt(ctx, stmt);
  popScope(ctx);
}

// 事実を「現在のスコープ」に書き込む(呼び出し側が必要なら先に pushScope しておく)
export function applyFacts(ctx: CheckerCtx, facts: Map<string, Type>) {
  const scope = ctx.scopes[ctx.scopes.length - 1];
  for (const [path, type] of facts) scope.set(path, { type, mutable: false });
}

// ブロックが必ず抜ける(return/break/continue で終わる)か — narrowing の継続判定に使う
export function blockTerminates(block: Block): boolean {
  const last = block.stmts[block.stmts.length - 1];
  return last !== undefined && (last.kind === "return" || last.kind === "break" || last.kind === "continue");
}

export function checkStmt(ctx: CheckerCtx, stmt: Stmt) {
  switch (stmt.kind) {
    case "shortVarDecl": {
      const types = checkExprList(ctx, stmt.values, stmt.names.length, stmt.pos);
      for (let i = 0; i < stmt.names.length; i++) {
        // mut 宣言はリテラル型を string に広げる(後で別の文字列を代入できるように)
        const t = stmt.mutable ? widenLiteral(types[i] ?? ANY) : (types[i] ?? ANY);
        declareBinding(ctx, stmt.names[i], t, stmt.pos, stmt.mutable);
      }
      break;
    }
    case "typedVarDecl": {
      // 宣言された型が「正」。値はそれに入れられればよい(none も union なら入る)
      const declared = resolveType(ctx, stmt.typeNode);
      const valueType = checkExprSingle(ctx, stmt.value);
      if (!assignable(valueType, declared)) {
        error(
          ctx,
          stmt.value.pos,
          "type-mismatch",
          `cannot use ${typeToString(valueType)} as ${typeToString(declared)}`,
        );
      }
      declareBinding(ctx, stmt.name, declared, stmt.pos, stmt.mutable);
      break;
    }
    case "assign": {
      const types = checkExprList(ctx, stmt.values, stmt.targets.length, stmt.pos);
      for (let i = 0; i < stmt.targets.length; i++) {
        const target = stmt.targets[i];
        if (target.kind === "ident" && target.name === "_") continue;
        // narrowing(F-6): `n.next = ...` はフィールド書き込みで、代入先そのものについては
        // 古い絞り込み事実を先に捨てておく(そうしないと targetType が絞り込み後の型に
        // なってしまい、代入できるはずの値が弾かれる)
        const path = stablePath(ctx, target);
        if (path !== null) invalidatePath(ctx, path);
        const targetType = checkExpr(ctx, target);
        if (target.kind === "ident") {
          const binding = lookup(ctx, target.name);
          if (!binding) continue; // 未宣言エラーは checkExpr が報告済み
          if (!binding.mutable) {
            error(
              ctx,
              target.pos,
              "immutable-assignment",
              `'${target.name}' is immutable — declare it with 'mut' to allow reassignment`,
            );
            continue;
          }
        }
        // map への書き込みは「値の型」に対して検査する(読みの V | none ではなく)
        let expected = targetType;
        if (target.kind === "index") {
          const container = target.target.resolvedType;
          if (container?.kind === "map") expected = container.value;
        }
        const valueType = types[i] ?? ANY;
        // F-9b: 複合代入(x += 1 等)は「現在値 op 右辺」を計算し、結果を代入先へ戻せるか検査する。
        // binary式の算術検査(checkArithOp)を共有し、safe-integer等のフラグはこの文自体へ立てる
        if (stmt.compoundOp && target.kind === "index" && target.target.resolvedType?.kind === "map") {
          // mapのキーは存在しないかもしれない(読みは常に V | none)。「現在値 + 右辺」を
          // 無条件に計算すると欠損キーが黙って壊れた値になるので、複合代入自体を禁止する
          error(
            ctx,
            stmt.pos,
            "compound-assign-on-map",
            `cannot use '${stmt.compoundOp}=' on a map entry — the key may not exist yet; ` +
              `write 'm[k] = (m[k] or fallback) ${stmt.compoundOp} value' instead`,
          );
        } else if (stmt.compoundOp) {
          const arith = checkArithOp(ctx, stmt.compoundOp, expected, stmt.values[i], valueType, stmt.pos);
          if (arith.intDiv) stmt.intDiv = true;
          if (arith.intMod) stmt.intMod = true;
          if (arith.intArith) stmt.intArith = true;
          if (!assignable(arith.type, expected)) {
            error(
              ctx,
              stmt.pos,
              "type-mismatch",
              `cannot assign ${typeToString(arith.type)} to ${typeToString(expected)}`,
            );
          }
        } else if (!assignable(valueType, expected)) {
          error(
            ctx,
            stmt.pos,
            "type-mismatch",
            `cannot assign ${typeToString(valueType)} to ${typeToString(expected)}`,
          );
        }
      }
      break;
    }
    case "exprStmt":
      checkExpr(ctx, stmt.expr);
      break;
    case "deferStmt": {
      // 'call'であることの検査が先 — そうでなければ中身を検査しても位置がずれた
      // 別のエラーになるだけ(例えば裸のintリテラルを式として検査してもエラーにならず、
      // 本来報告すべき「呼び出しじゃない」がまるごと消える)
      if (stmt.call.kind !== "call") {
        error(
          ctx,
          stmt.call.pos,
          "defer-requires-call",
          "'defer' must be followed by a function or method call, e.g. 'defer f(x)'",
        );
        break;
      }
      checkExpr(ctx, stmt.call);
      break;
    }
    case "return": {
      const expected = ctx.retStack[ctx.retStack.length - 1] ?? VOID;
      if (stmt.value === null) {
        if (!typeEquals(expected, VOID)) {
          error(ctx, stmt.pos, "missing-return-value", `this function must return ${typeToString(expected)}`);
        }
        break;
      }
      const t = checkExprSingle(ctx, stmt.value);
      if (typeEquals(expected, VOID)) {
        error(ctx, stmt.value.pos, "void-used-as-value", "this function has no return value");
      } else if (!assignable(t, expected)) {
        error(
          ctx,
          stmt.value.pos,
          "type-mismatch",
          `cannot return ${typeToString(t)} as ${typeToString(expected)}`,
        );
      }
      break;
    }
    case "if": {
      const cond = checkExprSingle(ctx, stmt.cond);
      if (!typeEquals(cond, BOOL) && cond.kind !== "any") {
        error(ctx, stmt.cond.pos, "not-bool", `if condition must be bool, got ${typeToString(cond)}`);
      }

      // narrowing(F-6): 条件式から then側/else側それぞれで成り立つ事実(パス→絞り込み型)を
      // 再帰的に集める(is / ! / && / || 、フィールドパスを含む)。事実が無ければ空のMapなので
      // 以下は「narrowing無し」の旧経路と同じに振る舞う
      const facts = collectFacts(ctx, stmt.cond);
      checkBlock(ctx, stmt.then, facts.then);
      if (stmt.else_) {
        if (stmt.else_.kind === "if") {
          pushScope(ctx);
          applyFacts(ctx, facts.else);
          checkStmt(ctx, stmt.else_);
          popScope(ctx);
        } else {
          checkBlock(ctx, stmt.else_, facts.else);
        }
      } else if (blockTerminates(stmt.then)) {
        applyFacts(ctx, facts.else); // 早期リターン後の残りの行では絞り込みが効き続ける
      }
      break;
    }
    case "for": {
      pushScope(ctx); // for i := ... の i はループ内スコープ
      // C風forのヘッダ変数は暗黙に可変(B-4決定。デフォルト不変の唯一の構造的例外)
      if (stmt.init?.kind === "shortVarDecl") stmt.init.mutable = true;
      if (stmt.init) checkStmt(ctx, stmt.init);
      if (stmt.cond) {
        const cond = checkExprSingle(ctx, stmt.cond);
        if (!typeEquals(cond, BOOL) && cond.kind !== "any") {
          error(ctx, stmt.cond.pos, "not-bool", `for condition must be bool, got ${typeToString(cond)}`);
        }
      }
      if (stmt.post) checkStmt(ctx, stmt.post);
      checkBlock(ctx, stmt.body);
      popScope(ctx);
      break;
    }
    case "wait":
      checkBlock(ctx, stmt.body);
      break;
    case "rangeFor": {
      const subject = checkExprSingle(ctx, stmt.subject);
      pushScope(ctx);
      const declare2 = (t1: Type, t2: Type, what: string) => {
        if (stmt.names.length !== 2) {
          error(
            ctx,
            stmt.pos,
            "range-arity",
            `range over ${what} needs two names: 'for a, b := range ...' (use _ to ignore one)`,
          );
        }
        declareBinding(ctx, stmt.names[0] ?? "_", t1, stmt.pos);
        declareBinding(ctx, stmt.names[1] ?? "_", t2, stmt.pos);
      };
      if (subject.kind === "array") {
        declare2(INT, subject.elem, "an array");
      } else if (subject.kind === "map") {
        declare2(subject.key, subject.value, "a map");
      } else if (typeEquals(subject, INT)) {
        if (stmt.names.length !== 1) {
          error(
            ctx,
            stmt.pos,
            "range-arity",
            "range over an int takes exactly one name: 'for i := range n'",
          );
        }
        declareBinding(ctx, stmt.names[0], INT, stmt.pos);
      } else if (subject.kind === "any") {
        for (const n of stmt.names) declareBinding(ctx, n, ANY, stmt.pos);
      } else {
        error(ctx, stmt.subject.pos, "not-rangeable", `cannot range over ${typeToString(subject)}`);
        for (const n of stmt.names) declareBinding(ctx, n, ANY, stmt.pos);
      }
      checkBlock(ctx, stmt.body);
      popScope(ctx);
      break;
    }
    case "send": {
      const ch = checkExprSingle(ctx, stmt.channel);
      const value = checkExprSingle(ctx, stmt.value);
      if (ch.kind === "chan") {
        if (!assignable(value, ch.elem)) {
          error(ctx, stmt.pos, "type-mismatch", `cannot send ${typeToString(value)} to ${typeToString(ch)}`);
        }
      } else if (ch.kind !== "any") {
        error(ctx, stmt.channel.pos, "not-a-channel", `cannot send to non-channel type ${typeToString(ch)}`);
      }
      break;
    }
    case "incDec": {
      const t = checkExprSingle(ctx, stmt.target);
      if (!isNumeric(t)) {
        error(ctx, stmt.pos, "invalid-operation", `'${stmt.op}' requires int or float, got ${typeToString(t)}`);
      }
      if (stmt.target.kind === "ident") {
        const binding = lookup(ctx, stmt.target.name);
        if (binding && !binding.mutable) {
          error(
            ctx,
            stmt.pos,
            "immutable-assignment",
            `'${stmt.target.name}' is immutable — declare it with 'mut' to allow '${stmt.op}'`,
          );
        }
      }
      break;
    }
    case "break":
    case "continue":
      break;
  }
}

// `a, b := 1, 2` のような「左辺N個 vs 右辺N個」の型リストを求める
export function checkExprList(ctx: CheckerCtx, values: Expr[], targetCount: number, pos: Pos): Type[] {
  if (values.length !== targetCount) {
    error(ctx, pos, "argument-count", `expected ${targetCount} value(s), got ${values.length}`);
  }
  return values.map((v) => checkExprSingle(ctx, v));
}
