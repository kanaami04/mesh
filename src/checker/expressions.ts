// 式の型推論の本体。checker.ts分割後もここが一番大きい — 型検査は「式を見てその場で
// 型を決め、周りの式もその型を前提に検査する」という相互再帰の塊で、機能ごとに割ると
// 呼び出しがファイルを跨ぎまくるだけなので、式推論はまとめて1ファイルに置く

import type { Expr } from "../ast";
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
  assignable,
  isNumeric,
  isStringy,
  typeEquals,
  typeToString,
  unionOf,
  unionWithout,
  widenLiteral,
  type StructField,
  type Type,
} from "../types";
import type { Pos } from "../token";
import { declareBinding, error, lookup, popScope, pushScope, BUILTINS, type CheckerCtx } from "./context";
import { fnType, isFailureType, resolveAlias, resolvePackageType, resolveType, findDiscriminantTag } from "./types-resolve";
import { collectFacts, stablePath, structPatternMatches } from "./narrowing";
import { checkFn } from "./functions";
import { inferMatch, inferSelect } from "./match-select";
import { inferCall, memberFieldType, tryPackageMember } from "./calls";
import { applyFacts } from "./statements";

// 「単一の値」が必要な場所用: void が来たらエラー
export function checkExprSingle(ctx: CheckerCtx, expr: Expr): Type {
  const t = checkExpr(ctx, expr);
  if (t.kind === "prim" && t.name === "void") {
    error(ctx, expr.pos, "void-used-as-value", "this function has no return value");
    return ANY;
  }
  return t;
}

export function checkExpr(ctx: CheckerCtx, expr: Expr): Type {
  const t = inferExpr(ctx, expr);
  expr.resolvedType = t;
  return t;
}

export function inferExpr(ctx: CheckerCtx, expr: Expr): Type {
  switch (expr.kind) {
    case "int": {
      // F-10フォローアップ: safe-integer検査は演算結果(__iarith)だけを見ており、
      // リテラルそのものが既に範囲外なら無検査ですり抜けていた(9007199254740993 等)。
      // 実行するまでもなく分かるバグなので、リテラル0除算と同じくコンパイル時に検出する
      if (!Number.isSafeInteger(Number(expr.value))) {
        error(
          ctx,
          expr.pos,
          "int-literal-overflow",
          `integer literal ${expr.value} exceeds the safe integer range ` +
            `(±${Number.MAX_SAFE_INTEGER}) and would silently lose precision`,
        );
      }
      return INT;
    }
    case "float": return FLOAT;
    case "string":
      // 文字列リテラルはリテラル型として推論する("active" は型 "active")。
      // string が必要な場所へは部分型として入る。mut 宣言では string に widening される
      return { kind: "literal", value: expr.value };
    case "bool": return BOOL;
    case "none": return NONE;

    case "is": {
      // is のパターンは match と同じ(型名・文字列リテラル・部分構造 { kind: "ok" })。
      // 部分構造は structPatternMatches で「一致するメンバーがあるか」を判定する
      const t = checkExprSingle(ctx, expr.operand);
      const target = resolveType(ctx, expr.target);
      ctx.isTargetTypes.set(expr, target); // collectFacts が resolveType を再度呼ばずに済むように
      if (t.kind === "union") {
        if (!t.members.some((m) => structPatternMatches(m, target))) {
          error(ctx, expr.pos, "impossible-pattern", `${typeToString(t)} can never be ${typeToString(target)}`);
        }
      } else if (t.kind !== "any") {
        error(
          ctx,
          expr.operand.pos,
          "union-required",
          `'is' needs a union-typed value, got ${typeToString(t)}`,
        );
      }
      return BOOL;
    }

    case "prop": {
      const t = checkExprSingle(ctx, expr.operand);
      // 文脈つき: f() ? "line ${i}: bad" — 文脈は文字列(補間可)。失敗はすべて error に
      // 変換して伝播する(none も error("文脈") へ昇格)ので、戻り値型には error が要る
      if (expr.context) {
        const contextType = checkExprSingle(ctx, expr.context);
        if (!isStringy(contextType)) {
          error(ctx, expr.context.pos, "prop-context-not-string", `'?' context must be a string, got ${typeToString(contextType)}`);
        }
      }
      if (t.kind === "any") return ANY;
      if (t.kind !== "union") {
        error(
          ctx,
          expr.pos,
          "prop-requires-failure-union",
          `'?' needs a union with none/error/an error type, got ${typeToString(t)}`,
        );
        return t;
      }
      const failures = t.members.filter((m) => isFailureType(m));
      if (failures.length === 0) {
        error(
          ctx,
          expr.pos,
          "prop-nothing-to-propagate",
          `'?' has nothing to propagate — ${typeToString(t)} has no none/error/error type`,
        );
      }
      // 文脈つきは常に error に変換して伝播するので、メッセージを持たない構造化エラー
      // (F-2後半)は文脈つきでは伝播できない — 素の '?' か 'is'/'match' での分岐に誘導する。
      // ここで弾いたら戻り値型との突き合わせ(下)はスキップする — ERRORへの変換自体が
      // 成立しないので「'error'を戻り値型に足せ」という誘導は的外れになるため
      const structured = failures.filter((f) => f.kind === "struct" && f.isErrorType);
      if (expr.context && structured.length > 0) {
        error(
          ctx,
          expr.pos,
          "prop-context-structured-error",
          `'?' with context can't convert ${structured.map((t) => typeToString(t)).join(" | ")} to a message` +
            ` — use plain '?' (no context) to propagate it as-is, or handle it with 'is'/'match' first`,
        );
      } else {
        const ret = ctx.retStack[ctx.retStack.length - 1] ?? VOID;
        // 文脈つきなら伝播するのは常に error。素の ? は失敗メンバーをそのまま伝播
        const propagated = expr.context ? (failures.length > 0 ? [ERROR] : []) : failures;
        for (const f of propagated) {
          if (!assignable(f, ret)) {
            error(
              ctx,
              expr.pos,
              "prop-return-type-mismatch",
              `'?' propagates ${typeToString(f)}, but this function returns ${typeToString(ret)}` +
                ` — add '${typeToString(f)}' to the return type or handle it with 'is'`,
            );
          }
        }
      }
      return unionWithout(t, (m) => isFailureType(m)); // 成功だけが残る(空ならvoid=文としてのみ使える)
    }

    case "match":
      return inferMatch(ctx, expr);

    case "spawn": {
      const ret = checkExpr(ctx, expr.call);
      // 戻り値なしの関数の spawn は「起動するだけ」(受取口なし=文としてのみ意味を持つ)
      if (typeEquals(ret, VOID)) return VOID;
      return { kind: "chan", elem: ret };
    }

    case "orElse": {
      const t = checkExprSingle(ctx, expr.left);
      const checkRight = (): Type => {
        // 束縛形 or e => ... は失敗値(none/error/error型のunion)を e に束縛して右辺を評価する
        if (expr.binding !== undefined && expr.binding !== "_") {
          const failures = t.kind === "union" ? t.members.filter((m) => isFailureType(m)) : [];
          pushScope(ctx);
          declareBinding(ctx, expr.binding, failures.length > 0 ? unionOf(failures) : ANY, expr.pos);
          const r = checkExprSingle(ctx, expr.right);
          popScope(ctx);
          return r;
        }
        return checkExprSingle(ctx, expr.right);
      };
      if (t.kind === "any") {
        checkRight();
        return ANY;
      }
      if (t.kind !== "union" || !t.members.some((m) => isFailureType(m))) {
        error(ctx, expr.pos, "or-never-fails", `left side of 'or' never fails — it is ${typeToString(t)}`);
        checkRight();
        return t;
      }
      // Go式の明示性(2026-07-19決定): error(組み込み/F-2後半の構造化error型)を含む union の
      // フォールバックは束縛形が必須。捨てる場合も `or _ => ...` と書かせて
      // 「握りつぶし」を字面(grep可能)に残す
      const hasNonNoneFailure = t.members.some((m) => m.kind !== "none" && isFailureType(m));
      if (hasNonNoneFailure && expr.binding === undefined) {
        error(
          ctx,
          expr.pos,
          "or-requires-binding",
          `'or' would silently discard an error — bind it ('or e => ...') or discard it explicitly ('or _ => ...')`,
        );
      }
      const rest = unionWithout(t, (m) => isFailureType(m));
      if (typeEquals(rest, VOID)) {
        error(
          ctx,
          expr.pos,
          "or-no-success-value",
          "left side of 'or' has no success value — handle it with 'is' instead",
        );
        checkRight();
        return ANY;
      }
      const r = checkRight();
      if (!assignable(r, rest)) {
        error(
          ctx,
          expr.right.pos,
          "or-fallback-type-mismatch",
          `'or' fallback must be ${typeToString(rest)}, got ${typeToString(r)}`,
        );
      }
      return rest;
    }

    case "interp": {
      // 補間される式は(printと同じく)どの型でもよい。結果は常に string
      for (const seg of expr.segments) {
        if (seg.kind === "expr") checkExprSingle(ctx, seg.expr);
      }
      return STRING;
    }

    case "ident": {
      const binding = lookup(ctx, expr.name);
      if (!binding) {
        if (BUILTINS.has(expr.name)) {
          error(
            ctx,
            expr.pos,
            "builtin-as-value",
            `'${expr.name}' is a builtin function — call it like ${expr.name}(...)`,
          );
        } else if (ctx.importAliases.has(expr.name)) {
          error(
            ctx,
            expr.pos,
            "package-as-value",
            `'${expr.name}' is a package — use it as a qualifier like ${expr.name}.something`,
          );
        } else {
          error(ctx, expr.pos, "undefined-name", `undefined: '${expr.name}'`);
        }
        return ANY;
      }
      return binding.type;
    }

    case "arrayLit": {
      // Todo[]{...} / int[]{...} — 要素型が明示された配列リテラル(空にできる)
      if (expr.elemType) {
        const elem = resolveType(ctx, expr.elemType);
        for (const e of expr.elems) {
          const t = checkExprSingle(ctx, e);
          if (!assignable(t, elem)) {
            error(ctx, e.pos, "type-mismatch", `array element must be ${typeToString(elem)}, got ${typeToString(t)}`);
          }
        }
        return { kind: "array", elem };
      }
      if (expr.elems.length === 0) return { kind: "array", elem: ANY };
      // 要素のリテラル型は widening する(["a", "b"] は "a"[] ではなく string[])
      const elem = widenLiteral(checkExprSingle(ctx, expr.elems[0]));
      for (let i = 1; i < expr.elems.length; i++) {
        const t = checkExprSingle(ctx, expr.elems[i]);
        if (!assignable(t, elem)) {
          error(
            ctx,
            expr.elems[i].pos,
            "type-mismatch",
            `array element must be ${typeToString(elem)}, got ${typeToString(t)}`,
          );
        }
      }
      return { kind: "array", elem };
    }

    case "binary":
      return inferBinary(ctx, expr);

    case "unary": {
      const t = checkExprSingle(ctx, expr.operand);
      if (expr.op === "!") {
        if (!typeEquals(t, BOOL) && t.kind !== "any") {
          error(ctx, expr.pos, "not-bool", `'!' requires bool, got ${typeToString(t)}`);
        }
        return BOOL;
      }
      if (!isNumeric(t)) {
        error(ctx, expr.pos, "invalid-operation", `unary '-' requires int or float, got ${typeToString(t)}`);
      }
      return t;
    }

    case "recv": {
      const ch = checkExprSingle(ctx, expr.channel);
      // 受信は常に T | closed(mapの V | none と同じ理由: closeされうることを型で強制する)
      if (ch.kind === "chan") return unionOf([ch.elem, CLOSED]);
      if (ch.kind !== "any") {
        error(ctx, expr.pos, "not-a-channel", `cannot receive from non-channel type ${typeToString(ch)}`);
      }
      return ANY;
    }

    case "select":
      return inferSelect(ctx, expr);

    case "call":
      return inferCall(ctx, expr);

    case "index": {
      const target = checkExprSingle(ctx, expr.target);
      const index = checkExprSingle(ctx, expr.index);
      // map の読み取りは V | none を返す(無いキーを無視できない。union路線の帰結)
      if (target.kind === "map") {
        if (!assignable(index, target.key)) {
          error(
            ctx,
            expr.index.pos,
            "type-mismatch",
            `map key must be ${typeToString(target.key)}, got ${typeToString(index)}`,
          );
        }
        return unionOf([target.value, NONE]);
      }
      if (!isNumeric(index) || (index.kind === "prim" && index.name === "float")) {
        if (index.kind !== "any") {
          error(ctx, expr.index.pos, "invalid-index-type", `index must be int, got ${typeToString(index)}`);
        }
      }
      if (target.kind === "array") return target.elem;
      if (isStringy(target)) return STRING;
      if (target.kind !== "any") {
        error(ctx, expr.pos, "not-indexable", `cannot index into ${typeToString(target)}`);
      }
      return ANY;
    }

    case "mapLit": {
      const key = resolveType(ctx, expr.key);
      const value = resolveType(ctx, expr.value);
      for (const e of expr.entries) {
        const kt = checkExprSingle(ctx, e.key);
        const vt = checkExprSingle(ctx, e.value);
        if (!assignable(kt, key)) {
          error(ctx, e.key.pos, "type-mismatch", `map key must be ${typeToString(key)}, got ${typeToString(kt)}`);
        }
        if (!assignable(vt, value)) {
          error(
            ctx,
            e.value.pos,
            "type-mismatch",
            `map value must be ${typeToString(value)}, got ${typeToString(vt)}`,
          );
        }
      }
      return { kind: "map", key, value };
    }

    case "member": {
      // math.add のようなパッケージ修飾参照(関数値としての参照)を先に解決する
      const pkgFn = tryPackageMember(ctx, expr);
      if (pkgFn) return pkgFn;
      const t = checkExprSingle(ctx, expr.target);
      // narrowing(F-6): このフィールドパス自体が絞り込み済みなら(`if n.next is none`等)
      // それを使う。無ければ通常のフィールド型
      const path = stablePath(ctx, expr);
      const override = path === null ? undefined : lookup(ctx, path);
      return override ? override.type : memberFieldType(ctx, expr, t);
    }

    case "structLit": {
      // math.Point{...} はimportしたパッケージのexported型から、User{...} は自パッケージから
      const resolved = expr.pkg
        ? resolvePackageType(ctx, expr.pkg, expr.name, expr.pos)
        : resolveAlias(ctx, expr.name, expr.pos);
      // フィールド値は先に1回だけ検査する(候補メンバーの絞り込みにも使うので二重評価しない)
      const fieldTypes = expr.fields.map((f) => checkExprSingle(ctx, f.value));

      let t = resolved;
      // 判別可能union(C-1、F-7でタグ必須化): GetUserResponse{kind: "ok", user: u} のように、
      // union型の名前をそのまま struct リテラルの名前として使う。
      // 無名{...}メンバー(union自身の名前でしか構築できない — フィールド集合の遠隔作用が
      // 起きるのはここだけ)が2個以上あれば、書かれたタグフィールドの値だけを見てメンバーを
      // 特定する(フィールド集合は一切見ない)。名前付きstruct同士のunion(type Shape = Circle
      // | Square)はそれぞれ自分の名前で構築されるので対象外 — 従来どおりフィールド集合で解決する
      if (resolved.kind === "union") {
        const structMembers = resolved.members.filter((m): m is Type & { kind: "struct" } => m.kind === "struct");
        const anonymousMembers = structMembers.filter((m) => m.name === "(anonymous)");
        if (anonymousMembers.length >= 2) {
          if (!resolved.discriminantTag) {
            // 型宣言自体がタグ不足で既にエラー報告済み(discriminated-union-tag-required)。
            // ここで二重にエラーを出さず黙って諦める
            return ANY;
          }
          const tagName = resolved.discriminantTag;
          const tagIndex = expr.fields.findIndex((f) => f.name === tagName);
          const tagType = tagIndex === -1 ? null : fieldTypes[tagIndex];
          if (tagType === null || tagType.kind !== "literal") {
            error(
              ctx,
              expr.pos,
              "discriminated-union-tag-missing",
              `'${expr.name}{...}' needs its tag field '${tagName}' set to select a member ` +
                `(e.g. ${expr.name}{ ${tagName}: "...", ... })`,
            );
            return ANY;
          }
          const match = anonymousMembers.find((m) => {
            const f = m.fields.find((f) => f.name === tagName);
            return f?.type.kind === "literal" && f.type.value === tagType.value;
          });
          if (!match) {
            const validValues = anonymousMembers
              .map((m) => m.fields.find((f) => f.name === tagName))
              .filter((f): f is StructField & { type: Type & { kind: "literal" } } =>
                f !== undefined && f.type.kind === "literal",
              )
              .map((f) => `"${f.type.value}"`)
              .join(" | ");
            error(
              ctx,
              expr.pos,
              "discriminated-union-no-match",
              `no member of '${expr.name}' has ${tagName}: "${tagType.value}" (valid ${tagName} values: ${validValues})`,
            );
            return ANY;
          }
          t = match;
        } else if (structMembers.length <= 1) {
          t = structMembers[0] ?? ANY;
        } else {
          // 名前付きstruct同士のunion(無名メンバーは1個以下): 従来どおりフィールド集合で解決
          const fieldNameSet = new Set(expr.fields.map((f) => f.name));
          let candidates = structMembers.filter((m) => {
            const memberNames = new Set(m.fields.map((f) => f.name));
            return memberNames.size === fieldNameSet.size && [...fieldNameSet].every((n) => memberNames.has(n));
          });
          if (candidates.length > 1) {
            candidates = candidates.filter((m) =>
              expr.fields.every((f, i) => {
                const decl = m.fields.find((d) => d.name === f.name);
                return decl !== undefined && assignable(fieldTypes[i], decl.type);
              }),
            );
          }
          if (candidates.length !== 1) {
            const shapes = structMembers.map((m) => `{ ${m.fields.map((f) => f.name).join(", ")} }`).join(" | ");
            error(
              ctx,
              expr.pos,
              candidates.length === 0 ? "discriminated-union-no-match" : "discriminated-union-ambiguous",
              candidates.length === 0
                ? `no member of '${expr.name}' matches the field(s) {${[...fieldNameSet].join(", ")}}` +
                    (shapes ? ` (union members: ${shapes})` : "")
                : `ambiguous — multiple members of '${expr.name}' match the field(s) {${[...fieldNameSet].join(", ")}}`,
            );
            return ANY;
          }
          t = candidates[0];
        }
      }
      if (t.kind === "any") return ANY; // 解決自体が失敗(未知/未export)— エラーは報告済み
      if (t.kind !== "struct") {
        error(ctx, expr.pos, "not-a-struct", `'${expr.name}' is not a struct`);
        return ANY;
      }
      // エラーメッセージ上の名前: union経由なら union の名前(メンバーは無名なので)、
      // 普通の struct ならそのまま struct 名
      const structType = t; // const に束縛し直して、以降 struct であることの絞り込みを効かせる
      const displayName = resolved.kind === "union" ? expr.name : structType.name;
      const seen = new Set<string>();
      expr.fields.forEach((f, i) => {
        if (seen.has(f.name)) {
          error(ctx, f.pos, "duplicate-field", `duplicate field '${f.name}'`);
          return;
        }
        seen.add(f.name);
        const decl = structType.fields.find((d) => d.name === f.name);
        if (!decl) {
          error(
            ctx,
            f.pos,
            "unknown-field",
            `${displayName} has no field '${f.name}' (fields: ${structType.fields.map((d) => d.name).join(", ")})`,
          );
          return;
        }
        if (!assignable(fieldTypes[i], decl.type)) {
          error(
            ctx,
            f.value.pos,
            "type-mismatch",
            `field '${f.name}': cannot use ${typeToString(fieldTypes[i])} as ${typeToString(decl.type)}`,
          );
        }
      });
      // 全フィールド必須(v1。ゼロ値・デフォルト値は導入しない)
      const missing = structType.fields.filter((d) => !seen.has(d.name));
      if (missing.length > 0) {
        error(
          ctx,
          expr.pos,
          "missing-fields",
          `missing field(s) in ${displayName}: ${missing.map((d) => d.name).join(", ")}`,
        );
      }
      // F-2後半: このリテラルの具体的な形(union経由なら絞り込んだメンバー)がerror type
      // としてタグ付けされていれば、codegenが実行時マーカーを埋め込めるようにAST側へ残す
      expr.isErrorInstance = structType.isErrorType === true;
      // 式全体の型は union 自体(narrow なメンバー型ではない)。match/is で絞り込むまでは
      // 常に union として扱う(mut var再代入・widening等を新規に考えなくて済むようにする)
      return resolved.kind === "union" ? resolved : t;
    }

    case "fnExpr": {
      const t = fnType(ctx, expr.params, expr.ret);
      checkFn(ctx, expr);
      return t;
    }

    case "chanExpr": {
      // F-11: capacityは常に必須。'none'なら無制限、それ以外はintでなければならない
      if (expr.capacity.kind !== "none") {
        const cap = checkExprSingle(ctx, expr.capacity);
        if (!typeEquals(cap, INT) && cap.kind !== "any") {
          error(
            ctx,
            expr.capacity.pos,
            "type-mismatch",
            `channel capacity must be int or none, got ${typeToString(cap)}`,
          );
        }
      }
      return { kind: "chan", elem: resolveType(ctx, expr.elem) };
    }
  }
}

export function inferBinary(ctx: CheckerCtx, expr: Expr & { kind: "binary" }): Type {
  const { op } = expr;

  // narrowing(F-6): 右辺を検査する前に左辺の事実を適用する(&&は左が真のときだけ右を評価
  // するので then側の事実、||は左が偽のときだけ右を評価するので else側の事実が右辺で使える)
  if (op === "&&" || op === "||") {
    const left = checkExprSingle(ctx, expr.left);
    if (!typeEquals(left, BOOL) && left.kind !== "any") {
      error(ctx, expr.left.pos, "not-bool", `'${op}' requires bool operands, got ${typeToString(left)}`);
    }
    const leftFacts = collectFacts(ctx, expr.left);
    pushScope(ctx);
    applyFacts(ctx, op === "&&" ? leftFacts.then : leftFacts.else);
    const right = checkExprSingle(ctx, expr.right);
    popScope(ctx);
    if (!typeEquals(right, BOOL) && right.kind !== "any") {
      error(ctx, expr.right.pos, "not-bool", `'${op}' requires bool operands, got ${typeToString(right)}`);
    }
    return BOOL;
  }

  const left = checkExprSingle(ctx, expr.left);
  const right = checkExprSingle(ctx, expr.right);

  if (op === "==" || op === "!=") {
    // none との比較は narrowing が効く 'is' に一本化する(P1)
    if (expr.left.kind === "none" || expr.right.kind === "none") {
      // fix: `x == none` は演算子を'is'に置き換えるだけで `x is none` になる(none側はそのまま)。
      // 左辺がnoneや、!=の場合は単純なトークン置換で表現できないのでfix無し
      const canAutoFix = op === "==" && expr.right.kind === "none";
      error(
        ctx,
        expr.pos,
        "use-is-none",
        `use 'is none' to test for none (== does not narrow the type)`,
        canAutoFix
          ? {
              description: "replace '==' with 'is'",
              range: { start: expr.pos, end: { line: expr.pos.line, col: expr.pos.col + 2 } },
              replacement: "is",
            }
          : undefined,
      );
      return BOOL;
    }
    if (!assignable(left, right) && !assignable(right, left)) {
      error(ctx, expr.pos, "incomparable-types", `cannot compare ${typeToString(left)} with ${typeToString(right)}`);
    }
    return BOOL;
  }

  if (op === "<" || op === "<=" || op === ">" || op === ">=") {
    const ok = (isNumeric(left) && isNumeric(right)) ||
      (isStringy(left) && isStringy(right)) ||
      left.kind === "any" || right.kind === "any";
    if (!ok) {
      error(ctx, expr.pos, "incomparable-types", `cannot compare ${typeToString(left)} with ${typeToString(right)}`);
    }
    return BOOL;
  }

  // 算術演算: + - * / %(binary式とF-9bの複合代入 += 等で共有 — checkArithOp参照)
  const arith = checkArithOp(ctx, op as "+" | "-" | "*" | "/" | "%", left, expr.right, right, expr.pos);
  if (arith.intDiv) expr.intDiv = true;
  if (arith.intMod) expr.intMod = true;
  if (arith.intArith) expr.intArith = true;
  return arith.type;
}

// 算術演算(+ - * / %)の型検査。binary式の算術分岐とF-9bの複合代入(+=など)が共有する。
// rightExpr は0除算リテラル検査に使う実際のAST(型だけでは "0" という値まで分からない)。
// フラグ(intDiv/intMod/intArith)は呼び出し側のASTノード(binary式 or Assign文)へ立てる
export function checkArithOp(
  ctx: CheckerCtx,
  op: "+" | "-" | "*" | "/" | "%",
  left: Type,
  rightExpr: Expr,
  right: Type,
  pos: Pos,
): { type: Type; intDiv?: boolean; intMod?: boolean; intArith?: boolean } {
  if (op === "+" && isStringy(left) && isStringy(right)) {
    return { type: STRING };
  }
  if (isNumeric(left) && isNumeric(right)) {
    const isInt = typeEquals(left, INT) && typeEquals(right, INT);
    // リテラルの 0 で割るのは実行するまでもなくバグ。コンパイル時に弾く
    if (isInt && (op === "/" || op === "%") && rightExpr.kind === "int" && rightExpr.value === "0") {
      error(ctx, rightExpr.pos, "division-by-zero", `integer ${op === "/" ? "division" : "modulo"} by zero`);
    }
    if (left.kind === "any" || right.kind === "any") return { type: ANY };
    return {
      type: isInt ? INT : FLOAT,
      intDiv: op === "/" && isInt, // int同士の除算は切り捨て+ゼロ検査
      intMod: op === "%" && isInt, // int同士の剰余はゼロ検査
      intArith: isInt && (op === "+" || op === "-" || op === "*"), // F-10: safe-integer検査
    };
  }
  if (left.kind === "any" || right.kind === "any") return { type: ANY };
  error(
    ctx,
    pos,
    "invalid-operation",
    `invalid operation: ${typeToString(left)} ${op} ${typeToString(right)}` +
      (op === "+" && (typeEquals(left, STRING) || typeEquals(right, STRING))
        ? " (hint: use str() to convert values to string)"
        : ""),
  );
  return { type: ANY };
}
