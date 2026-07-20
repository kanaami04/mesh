// コンパイルパイプライン: lex → parse → check → codegen
// (lex は parse の中で呼ばれる)
// ファイルシステムには依存しない(ブラウザのプレイグラウンドでも動く)。
// 複数ファイルの読み込みは CLI 側の仕事で、ここはソース文字列の列を受け取るだけ

import { checkModules, type Diagnostic, type ParsedModule, type TestInfo } from "./checker";
import { generateModules } from "./codegen";
import { parse } from "./parser";
import { CompileError } from "./token";

export interface CompileResult {
  code: string | null; // エラーがあれば null
  diagnostics: Diagnostic[];
  tests: TestInfo[]; // F-15: 発見したテスト関数(mesh testが使う。通常のcompileでは常に空配列)
}

export interface ModuleSource {
  pkg: string; // パッケージ名(エントリは "main"、それ以外はディレクトリ名)
  file: string;
  source: string;
}

// 単一ファイル(従来のAPI)。"main" パッケージ1ファイルとしてコンパイルする
export function compile(source: string, file = "main.mesh"): CompileResult {
  return compileModules([{ pkg: "main", file, source }]);
}

export function compileModules(modules: ModuleSource[], opts?: { testMode?: boolean }): CompileResult {
  const parsed: ParsedModule[] = [];
  const parseErrors: Diagnostic[] = [];
  for (const m of modules) {
    try {
      parsed.push({ pkg: m.pkg, file: m.file, program: parse(m.source) });
    } catch (e) {
      if (e instanceof CompileError) {
        parseErrors.push({ pos: e.pos, code: e.code, message: e.message, file: m.file, fix: e.fix });
      } else {
        throw e;
      }
    }
  }
  if (parseErrors.length > 0) return { code: null, diagnostics: parseErrors, tests: [] };

  const { diagnostics, tests } = checkModules(parsed, opts);
  if (diagnostics.length > 0) return { code: null, diagnostics, tests: [] };
  const code = generateModules(parsed, { tests: opts?.testMode ? tests : undefined });
  return { code, diagnostics: [], tests };
}

export function formatDiagnostics(file: string, diagnostics: Diagnostic[]): string {
  return diagnostics
    .map((d) => `${d.file ?? file}:${d.pos.line}:${d.pos.col}: error[${d.code}]: ${d.message}`)
    .join("\n");
}

// AIエージェント向けの構造化出力(mesh check --json)。
// 安定した機械可読フォーマットとして、フィールドの削除・改名はしない方針(F-13でcode/fixを追加。
// 既存フィールドはそのままなので既存の消費者への破壊的変更ではない)。
// code は `mesh explain <code>` の入力。fix は機械適用可能な単一range置換(無ければundefined —
// 安全に自動化できない診断は無理にfixを作らない)
export function diagnosticsToJson(file: string, diagnostics: Diagnostic[]): string {
  return JSON.stringify(
    {
      file,
      ok: diagnostics.length === 0,
      diagnostics: diagnostics.map((d) => ({
        file: d.file ?? file,
        line: d.pos.line,
        col: d.pos.col,
        severity: "error",
        code: d.code,
        message: d.message,
        fix: d.fix,
      })),
    },
    null,
    2,
  );
}
