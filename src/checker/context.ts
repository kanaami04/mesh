// Checkerの共有状態と基盤(スコープ/宣言/エラー報告)。
// TypeScriptにpartial classが無いため、checker.tsの分割は「1つのCheckerクラス」ではなく
// 「共有コンテキストオブジェクト(CheckerCtx)を受け取る素の関数群」という形を取る
// (this.foo(x) だった呼び出しは foo(ctx, x) になる)。ファイルはESM循環import可能だが、
// すべての相互参照は関数本体の中(実行時)にしか無いので安全 — モジュール初期化順には無関係

import type { Expr, Program, TypeNode } from "../ast";
import type { Diagnostic, DiagnosticCode, Fix } from "../diagnostic-codes";
import type { Pos } from "../token";
import type { Type } from "../types";

export type { Diagnostic, DiagnosticCode, Fix } from "../diagnostic-codes";

// 組み込みの型名(type 宣言でこれらの名前は使えない)
export const BUILTIN_TYPE_NAMES = new Set([
  "int", "float", "string", "bool", "void", "error", "none", "closed", "any",
]);

// 組み込み関数。特殊な検査(可変長引数など)は checkCall 内で行う。
export const BUILTINS = new Set([
  "print", "len", "push", "str", "error", "sleep", "delete",
  "contains", "indexOf", "get", "keys", "values", "sort",
  "split", "join", "trim", "upper", "lower", "toInt",
  "toFloat", "round", "floor", "ceil", // int/floatの片道変換しか無かった穴を埋める(レビュー起点)
  "filter", "map", "reduce", // F-8: transform → map(文脈依存キーワード化により 'map' の予約と両立)
  "close",
]);

// 生成される JavaScript で意味を持ってしまう名前は変数名として禁止する
export const RESERVED = new Set([
  "await", "async", "function", "const", "let", "var", "class", "new", "this",
  "typeof", "instanceof", "in", "of", "yield", "delete", "void", "switch",
  "case", "default", "do", "while", "with", "export", "import", "extends",
  "super", "null", "undefined", "try", "catch", "finally", "throw",
  "eval", "arguments",
]);

// ---- 複数パッケージのコンパイル単位 ----

export interface ParsedModule {
  pkg: string; // パッケージ名(= ディレクトリ名。エントリは "main")
  file: string;
  program: Program;
}

// パッケージが外へ見せる(または隠している)シンボル。exported フラグごと持つことで
// 「存在しない」と「exportされていない」を別のエラーメッセージにできる(P4)
export interface PackageSymbols {
  types: Map<string, { type: Type; exported: boolean }>;
  fns: Map<string, { type: Type; exported: boolean }>;
  consts: Map<string, { type: Type; exported: boolean }>; // F-9c: トップレベル定数
}

// F-15: `mesh test` が発見したテスト関数。`_test.mesh` ファイル内のトップレベル fn で、
// 名前が "test" で始まり、シグネチャが `() none | error` のものだけが対象(declareの時点で検証済み)
export interface TestInfo {
  name: string; // Mesh上の名前(例: testAddition)
  jsName: string; // codegen後のJS名(pkgマングリング込み。mainならnameと同じ)
  file: string;
  pos: Pos;
}

export interface CheckResult {
  diagnostics: Diagnostic[];
  tests: TestInfo[]; // F-15
}

// 変数1つ分の情報。mutable は「mut 宣言されたか」(デフォルト不変、B-4決定)
export interface Binding {
  type: Type;
  mutable: boolean;
}

// 元は Checker クラスの private フィールド群だった状態。1パッケージの検査ごとに
// createCheckerCtx() で1つ作る
export interface CheckerCtx {
  diagnostics: Diagnostic[];
  // narrowing(F-6): 変数の型と同じ scope スタックに、フィールドパス("n.next"のような
  // ドット区切りの文字列キー。識別子には"."を含められないので実変数と衝突しない)を積んで
  // 絞り込みを表す。ブロックを抜ければ他の変数と同様スコープごと消える
  scopes: Map<string, Binding>[];
  // is式のtarget型(resolveType結果)のメモ化。narrowing事実の再計算(collectFacts)で
  // resolveTypeを二度呼ぶと診断が重複するため、is式を検査した時点で1回だけキャッシュする
  isTargetTypes: WeakMap<Expr, Type>;
  // 今チェックしている関数の戻り値型(無名関数でネストするのでスタック)
  retStack: Type[];
  // ジェネリック関数(F-1後半)の型パラメータ名。今そのシグネチャ/本体を解決している間だけ
  // resolveTypeが"typeParam"として認識する(ネストは実際には起きないが retStack と同じ形で安全に)
  typeParamStack: Set<string>[];
  // ジェネリック関数の宣言: 名前 → (型パラメータ名の並び, typeParamを含んだ抽象fn型)。
  // 呼び出し側はここを引いて unifyTypeParam → substituteTypeParams で具体化する
  genericFns: Map<string, { typeParams: string[]; type: Type }>;
  // type 宣言: 名前 → 構文ノード。解決結果は resolvedAliases にメモ化
  typeTable: Map<string, TypeNode>;
  resolvedAliases: Map<string, Type>;
  resolvingAliases: Set<string>; // 循環検出用
  // 今検査中のファイル(診断のfile属性づけ用)
  currentFile: string;
  // このパッケージのトップレベル宣言(exports収集用): 名前 → exportedフラグ
  typeExported: Map<string, boolean>;
  fnDecls: Map<string, { type: Type; exported: boolean }>;
  constDecls: Map<string, { type: Type; exported: boolean }>; // F-9c
  discoveredTests: TestInfo[]; // F-15: `mesh test`が実行する対象
  // error type X = ... / error struct X { ... }(F-2後半)で宣言された名前の集合。
  // resolveAliasがこれを見て、そのエイリアスの構造体メンバーに isErrorType を立てる
  errorTypeNames: Set<string>;

  // このcheckerが検査するパッケージ名("main" 以外のstruct名は pkg.Name に修飾される)
  readonly pkg: string;
  // 検査済みパッケージのシンボル表(依存順に検査するので、importする側から常に見える)
  readonly registry: Map<string, PackageSymbols>;
  // メソッド: struct名(修飾済み) → (メソッド名 → 関数型 [レシーバを含む])。
  // 自由関数のグローバル scope とは別の名前空間(P1: recv.method() と method(recv) が両方
  // 使える「二通りの書き方」を作らないため、メソッド名はここにしか登録しない)。
  // 全パッケージで共有し、exportされたstructのメソッドをパッケージ越しに呼べるようにする
  readonly methodTable: Map<string, Map<string, Type>>;
  // このパッケージのファイル群がimportしたパッケージ名(修飾アクセスの解決に使う)
  readonly importAliases: Set<string>;
}

export function createCheckerCtx(
  pkg: string,
  registry: Map<string, PackageSymbols>,
  methodTable: Map<string, Map<string, Type>>,
  importAliases: Set<string>,
): CheckerCtx {
  return {
    diagnostics: [],
    scopes: [new Map()],
    isTargetTypes: new WeakMap(),
    retStack: [],
    typeParamStack: [],
    genericFns: new Map(),
    typeTable: new Map(),
    resolvedAliases: new Map(),
    resolvingAliases: new Set(),
    currentFile: "main.mesh",
    typeExported: new Map(),
    fnDecls: new Map(),
    constDecls: new Map(),
    discoveredTests: [],
    errorTypeNames: new Set(),
    pkg,
    registry,
    methodTable,
    importAliases,
  };
}

// ---- ユーティリティ ----

// code は必須(F-13): 呼び出し箇所ごとに省略できてしまうと、いずれ一部の診断だけ
// コード無しのまま漏れて「安定した機械可読フォーマット」の契約が崩れるため
export function error(ctx: CheckerCtx, pos: Pos, code: DiagnosticCode, message: string, fix?: Fix) {
  ctx.diagnostics.push({ pos, code, message, file: ctx.currentFile, fix });
}

export function pushScope(ctx: CheckerCtx) {
  ctx.scopes.push(new Map());
}

export function popScope(ctx: CheckerCtx) {
  ctx.scopes.pop();
}

export function pushTypeParams(ctx: CheckerCtx, names: string[]) {
  ctx.typeParamStack.push(new Set(names));
}

export function popTypeParams(ctx: CheckerCtx) {
  ctx.typeParamStack.pop();
}

export function isTypeParam(ctx: CheckerCtx, name: string): boolean {
  return ctx.typeParamStack.some((s) => s.has(name));
}

// 名前は declareBinding(declareではない): 'declare' はTS/JSの予約語的な文脈依存キーワードで、
// `declare(ctx, name, ...)` のように文頭で裸のまま関数呼び出しすると、tscは正しく型検査
// する一方でbunのトランスパイラは"declare"文(アンビエント宣言)と誤認して実行時に文ごと
// 消してしまう(実際に踏んだバグ — tscは通るのにbun実行だけ全ての宣言が効かなくなった)
export function declareBinding(ctx: CheckerCtx, name: string, type: Type, pos: Pos, mutable = false) {
  if (name === "_") return; // ブランク識別子は捨てる用
  if (RESERVED.has(name)) {
    error(ctx, pos, "reserved-word", `'${name}' is a reserved word and cannot be used as a name`);
    return;
  }
  if (BUILTINS.has(name)) {
    error(ctx, pos, "builtin-redeclared", `'${name}' is a builtin function and cannot be redeclared`);
    return;
  }
  if (ctx.importAliases.has(name)) {
    error(ctx, pos, "name-conflicts-with-package", `'${name}' conflicts with an imported package name`);
    return;
  }
  const scope = ctx.scopes[ctx.scopes.length - 1];
  if (scope.has(name)) {
    error(ctx, pos, "already-declared", `'${name}' is already declared in this scope`);
    return;
  }
  // シャドーイング禁止(2026-07-17決定): 外側スコープ(関数名を含む)に同名があれば
  // 「隠しただけで更新していない」バグの温床になるので拒否する。更新したいなら '=' を使う。
  if (lookup(ctx, name) !== undefined) {
    error(
      ctx,
      pos,
      "shadowing",
      `'${name}' shadows an outer binding — use '=' to update it, or pick a different name`,
    );
    return;
  }
  scope.set(name, { type, mutable });
}

export function lookup(ctx: CheckerCtx, name: string): Binding | undefined {
  for (let i = ctx.scopes.length - 1; i >= 0; i--) {
    const b = ctx.scopes[i].get(name);
    if (b) return b;
  }
  return undefined;
}

// struct フィールド名の予約チェック。codegen は `{ name: value }` という素のJSオブジェクト
// リテラルへ直訳するため、'__proto__' だけは他のフィールドと違って特別扱いされ
// (代入ではなくprototypeの差し替えになる)、値が黙って消える(レビュー起点)
export function checkFieldName(ctx: CheckerCtx, name: string, pos: Pos) {
  if (name === "__proto__") {
    error(
      ctx,
      pos,
      "reserved-field-name",
      `'__proto__' can't be used as a field name — pick a different name`,
    );
  }
}
