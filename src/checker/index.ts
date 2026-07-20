// Checker: AST を歩いて型の矛盾を探す。ここが「TypeScriptらしさ」の心臓部。
// - `:=` の右辺から型を推論して変数に記録する
// - 関数呼び出しの引数の数と型を照合する
// - 検査しながら式に resolvedType を書き込み、Codegen へ引き継ぐ
//
// 実装は関心事ごとに複数ファイルへ分割されている(2551行の単一クラスだった頃から移行)。
// TypeScriptにpartial classが無いため、「1つのCheckerクラス」ではなく「共有コンテキスト
// オブジェクト(CheckerCtx)を受け取る素の関数群」という形を取る。式推論・文検査・
// 型解決などは元々1回の再帰で行き来する強結合なアルゴリズムなので、ファイル間の
// 循環importが多数あるが、すべての相互参照は関数本体の中(実行時)にしかないので安全
// (モジュール初期化順には無関係)。このファイルは外部から見える唯一の入口(public API)。
//
//   context.ts       — 定数(BUILTINS/RESERVED)・CheckerCtx・スコープ/宣言の基盤
//   types-resolve.ts — 型注釈の解決、type宣言のエイリアス解決、判別可能unionのタグ判定
//   narrowing.ts     — is/matchのnarrowing facts エンジン
//   expressions.ts   — 式推論の本体(inferExpr)
//   match-select.ts  — match式・select式
//   calls.ts         — 関数・メソッド呼び出しの解決
//   generics.ts      — ジェネリック関数(F-1後半)
//   builtins.ts      — 組み込み関数(print/len/push/...)の検査
//   functions.ts     — 関数/メソッド宣言・本体検査の入口
//   statements.ts    — 文の検査
//   modules.ts       — 複数パッケージの依存グラフ検証・パッケージ全体の検査

export { check, checkModules } from "./modules";
export { BUILTINS, RESERVED } from "./context";
export type {
  CheckResult,
  Diagnostic,
  DiagnosticCode,
  Fix,
  PackageSymbols,
  ParsedModule,
  TestInfo,
} from "./context";
