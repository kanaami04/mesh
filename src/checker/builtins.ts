// 組み込み関数(print/len/push/...)の呼び出し検査。1つの巨大なswitchで、
// 他の検査(式推論・文検査)とは独立して読み書きできるのでここに分離する

import type { Expr } from "../ast";
import {
  ANY,
  BOOL,
  ERROR,
  FLOAT,
  INT,
  NONE,
  STRING,
  VOID,
  assignable,
  isNumeric,
  isStringy,
  typeEquals,
  typeToString,
  unionOf,
  type Type,
} from "../types";
import { error, type CheckerCtx } from "./context";
import { checkExprSingle } from "./expressions";

export function inferBuiltinCall(ctx: CheckerCtx, name: string, expr: Expr & { kind: "call" }): Type {
  const args = expr.args.map((a) => checkExprSingle(ctx, a));
  const expectArity = (n: number): boolean => {
    if (args.length !== n) {
      error(ctx, expr.pos, "argument-count", `${name}() expects ${n} argument(s), got ${args.length}`);
      return false;
    }
    return true;
  };

  switch (name) {
    case "print":
      return VOID; // 可変長・任意型
    case "str":
      expectArity(1);
      return STRING;
    case "len": {
      if (expectArity(1)) {
        const t = args[0];
        const ok = t.kind === "array" || t.kind === "map" || t.kind === "any" || isStringy(t);
        if (!ok) {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `len() requires string, array or map, got ${typeToString(t)}`);
        }
      }
      return INT;
    }
    case "push": {
      if (expectArity(2)) {
        const arr = args[0];
        if (arr.kind === "array") {
          if (!assignable(args[1], arr.elem)) {
            error(
              ctx,
              expr.args[1].pos,
              "type-mismatch",
              `cannot push ${typeToString(args[1])} into ${typeToString(arr)}`,
            );
          }
        } else if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `push() requires an array, got ${typeToString(arr)}`);
        }
      }
      return VOID;
    }
    case "error": {
      if (expectArity(1) && !assignable(args[0], STRING)) {
        error(
          ctx,
          expr.args[0].pos,
          "builtin-arg-type",
          `error() requires a string message, got ${typeToString(args[0])}`,
        );
      }
      return ERROR;
    }
    case "delete": {
      if (expectArity(2)) {
        const m = args[0];
        if (m.kind === "map") {
          if (!assignable(args[1], m.key)) {
            error(
              ctx,
              expr.args[1].pos,
              "type-mismatch",
              `map key must be ${typeToString(m.key)}, got ${typeToString(args[1])}`,
            );
          }
        } else if (m.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `delete() requires a map, got ${typeToString(m)}`);
        }
      }
      return VOID;
    }
    case "sleep": {
      if (expectArity(1) && !isNumeric(args[0])) {
        error(
          ctx,
          expr.args[0].pos,
          "builtin-arg-type",
          `sleep() requires milliseconds (int), got ${typeToString(args[0])}`,
        );
      }
      return VOID;
    }
    case "contains": {
      if (expectArity(2)) {
        const arr = args[0];
        if (arr.kind === "array") {
          if (!assignable(args[1], arr.elem)) {
            error(
              ctx,
              expr.args[1].pos,
              "type-mismatch",
              `contains() second argument must be ${typeToString(arr.elem)}, got ${typeToString(args[1])}`,
            );
          }
        } else if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `contains() requires an array, got ${typeToString(arr)}`);
        }
      }
      return BOOL;
    }
    case "indexOf": {
      if (expectArity(2)) {
        const arr = args[0];
        if (arr.kind === "array") {
          if (!assignable(args[1], arr.elem)) {
            error(
              ctx,
              expr.args[1].pos,
              "type-mismatch",
              `indexOf() second argument must be ${typeToString(arr.elem)}, got ${typeToString(args[1])}`,
            );
          }
        } else if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `indexOf() requires an array, got ${typeToString(arr)}`);
        }
      }
      return unionOf([INT, NONE]);
    }
    case "get": {
      // F-9d: 配列の範囲外アクセスはpanicが正しいことも多いが(ユーザー入力由来のindex等)、
      // mapの欠損キーと同じく型で強制される安全な読みも用意する(arr[i]はpanic用途のまま残す)
      if (expectArity(2)) {
        const arr = args[0];
        if (arr.kind === "array") {
          if (!typeEquals(args[1], INT) && args[1].kind !== "any") {
            error(ctx, expr.args[1].pos, "invalid-index-type", `index must be int, got ${typeToString(args[1])}`);
          }
          return unionOf([arr.elem, NONE]);
        }
        if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `get() requires an array, got ${typeToString(arr)}`);
        }
      }
      return ANY;
    }
    case "keys": {
      if (!expectArity(1)) return { kind: "array", elem: ANY };
      const m = args[0];
      if (m.kind === "map") return { kind: "array", elem: m.key };
      if (m.kind !== "any") {
        error(ctx, expr.args[0].pos, "builtin-arg-type", `keys() requires a map, got ${typeToString(m)}`);
      }
      return { kind: "array", elem: ANY };
    }
    case "values": {
      if (!expectArity(1)) return { kind: "array", elem: ANY };
      const m = args[0];
      if (m.kind === "map") return { kind: "array", elem: m.value };
      if (m.kind !== "any") {
        error(ctx, expr.args[0].pos, "builtin-arg-type", `values() requires a map, got ${typeToString(m)}`);
      }
      return { kind: "array", elem: ANY };
    }
    case "sort": {
      if (expectArity(1)) {
        const arr = args[0];
        if (arr.kind === "array") {
          if (!isNumeric(arr.elem) && !isStringy(arr.elem)) {
            error(
              ctx,
              expr.args[0].pos,
              "builtin-arg-type",
              `sort() requires int[], float[] or string[], got ${typeToString(arr)}`,
            );
          }
        } else if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `sort() requires an array, got ${typeToString(arr)}`);
        }
      }
      // 非破壊(new arrayを返す)。引数の配列自体は変わらない
      return args[0]?.kind === "array" ? args[0] : ANY;
    }
    case "split": {
      if (expectArity(2)) {
        if (!isStringy(args[0])) {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `split() requires a string, got ${typeToString(args[0])}`);
        }
        if (!isStringy(args[1])) {
          error(
            ctx,
            expr.args[1].pos,
            "builtin-arg-type",
            `split() separator must be a string, got ${typeToString(args[1])}`,
          );
        }
      }
      return { kind: "array", elem: STRING };
    }
    case "join": {
      if (expectArity(2)) {
        const arr = args[0];
        if (arr.kind === "array") {
          if (!isStringy(arr.elem) && arr.elem.kind !== "any") {
            error(ctx, expr.args[0].pos, "builtin-arg-type", `join() requires string[], got ${typeToString(arr)}`);
          }
        } else if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `join() requires an array, got ${typeToString(arr)}`);
        }
        if (!isStringy(args[1])) {
          error(
            ctx,
            expr.args[1].pos,
            "builtin-arg-type",
            `join() separator must be a string, got ${typeToString(args[1])}`,
          );
        }
      }
      return STRING;
    }
    case "trim":
    case "upper":
    case "lower": {
      if (expectArity(1) && !isStringy(args[0])) {
        error(ctx, expr.args[0].pos, "builtin-arg-type", `${name}() requires a string, got ${typeToString(args[0])}`);
      }
      return STRING;
    }
    case "toInt": {
      if (expectArity(1) && !isStringy(args[0])) {
        error(ctx, expr.args[0].pos, "builtin-arg-type", `toInt() requires a string, got ${typeToString(args[0])}`);
      }
      return unionOf([INT, ERROR]);
    }
    // int/floatが片道(int→float)にしか変換できず、json.Value.n(float)を配列添字や
    // ループ境界のintへ戻す手段が無かった穴を埋める(レビュー起点)。丸め方向を選ばせるため
    // round/floor/ceilの3つに分け、「floatを持っている前提」を強制するためint入力は弾く
    // (すでにintなら変換の必要が無い、というP1の一貫性)
    case "toFloat": {
      if (expectArity(1) && !typeEquals(args[0], INT) && args[0].kind !== "any") {
        error(ctx, expr.args[0].pos, "builtin-arg-type", `toFloat() requires an int, got ${typeToString(args[0])}`);
      }
      return FLOAT;
    }
    case "round":
    case "floor":
    case "ceil": {
      if (expectArity(1) && !typeEquals(args[0], FLOAT) && args[0].kind !== "any") {
        error(
          ctx,
          expr.args[0].pos,
          "builtin-arg-type",
          `${name}() requires a float, got ${typeToString(args[0])}`,
        );
      }
      return INT;
    }
    case "close": {
      if (expectArity(1)) {
        const ch = args[0];
        if (ch.kind !== "chan" && ch.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `close() requires a channel, got ${typeToString(ch)}`);
        }
      }
      return VOID;
    }
    case "filter": {
      if (expectArity(2)) {
        const arr = args[0];
        const pred = args[1];
        if (arr.kind === "array") {
          if (pred.kind === "fn") {
            if (pred.params.length !== 1 || !assignable(arr.elem, pred.params[0])) {
              error(
                ctx,
                expr.args[1].pos,
                "callback-signature-mismatch",
                `filter() callback must take a single ${typeToString(arr.elem)} parameter`,
              );
            }
            if (!typeEquals(pred.ret, BOOL) && pred.ret.kind !== "any") {
              error(
                ctx,
                expr.args[1].pos,
                "callback-signature-mismatch",
                `filter() callback must return bool, got ${typeToString(pred.ret)}`,
              );
            }
          } else if (pred.kind !== "any") {
            error(
              ctx,
              expr.args[1].pos,
              "builtin-arg-type",
              `filter() second argument must be a function, got ${typeToString(pred)}`,
            );
          }
        } else if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `filter() requires an array, got ${typeToString(arr)}`);
        }
      }
      return args[0]?.kind === "array" ? args[0] : ANY;
    }
    case "map": { // F-8: 旧transform。高階関数のmap-over-array
      if (expectArity(2)) {
        const arr = args[0];
        const f = args[1];
        if (arr.kind === "array") {
          if (f.kind === "fn") {
            if (f.params.length !== 1 || !assignable(arr.elem, f.params[0])) {
              error(
                ctx,
                expr.args[1].pos,
                "callback-signature-mismatch",
                `map() callback must take a single ${typeToString(arr.elem)} parameter`,
              );
            }
          } else if (f.kind !== "any") {
            error(
              ctx,
              expr.args[1].pos,
              "builtin-arg-type",
              `map() second argument must be a function, got ${typeToString(f)}`,
            );
          }
        } else if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `map() requires an array, got ${typeToString(arr)}`);
        }
      }
      const f = args[1];
      return { kind: "array", elem: f?.kind === "fn" ? f.ret : ANY };
    }
    case "reduce": {
      if (expectArity(3)) {
        const arr = args[0];
        const f = args[1];
        const init = args[2];
        if (arr.kind === "array") {
          if (f.kind === "fn") {
            if (f.params.length !== 2) {
              error(
                ctx,
                expr.args[1].pos,
                "callback-signature-mismatch",
                "reduce() callback must take (accumulator, element)",
              );
            } else {
              if (!assignable(init, f.params[0])) {
                error(
                  ctx,
                  expr.args[2].pos,
                  "type-mismatch",
                  `reduce() initial value must be ${typeToString(f.params[0])}, got ${typeToString(init)}`,
                );
              }
              if (!assignable(arr.elem, f.params[1])) {
                error(
                  ctx,
                  expr.args[1].pos,
                  "callback-signature-mismatch",
                  `reduce() callback's second parameter must accept ${typeToString(arr.elem)}`,
                );
              }
              if (!assignable(f.ret, f.params[0])) {
                error(
                  ctx,
                  expr.args[1].pos,
                  "callback-signature-mismatch",
                  `reduce() callback must return ${typeToString(f.params[0])} (the accumulator type), got ${typeToString(f.ret)}`,
                );
              }
            }
          } else if (f.kind !== "any") {
            error(
              ctx,
              expr.args[1].pos,
              "builtin-arg-type",
              `reduce() second argument must be a function, got ${typeToString(f)}`,
            );
          }
        } else if (arr.kind !== "any") {
          error(ctx, expr.args[0].pos, "builtin-arg-type", `reduce() requires an array, got ${typeToString(arr)}`);
        }
      }
      const f = args[1];
      if (f?.kind === "fn" && f.params.length === 2) return f.params[0];
      return args[2] ?? ANY;
    }
    default:
      return ANY;
  }
}
