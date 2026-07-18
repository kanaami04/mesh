// コンパイルパイプライン: lex → parse → check → codegen
// (lex は parse の中で呼ばれる)

import { check, type Diagnostic } from "./checker";
import { generate } from "./codegen";
import { parse } from "./parser";
import { CompileError } from "./token";

export interface CompileResult {
  code: string | null; // エラーがあれば null
  diagnostics: Diagnostic[];
}

export function compile(source: string, file = "main.mesh"): CompileResult {
  try {
    const program = parse(source);
    const diagnostics = check(program);
    if (diagnostics.length > 0) return { code: null, diagnostics };
    return { code: generate(program, file), diagnostics: [] };
  } catch (e) {
    if (e instanceof CompileError) {
      return { code: null, diagnostics: [{ pos: e.pos, message: e.message }] };
    }
    throw e;
  }
}

export function formatDiagnostics(file: string, diagnostics: Diagnostic[]): string {
  return diagnostics
    .map((d) => `${file}:${d.pos.line}:${d.pos.col}: error: ${d.message}`)
    .join("\n");
}

// AIエージェント向けの構造化出力(mesh check --json)。
// 安定した機械可読フォーマットとして、フィールドの削除・改名はしない方針
export function diagnosticsToJson(file: string, diagnostics: Diagnostic[]): string {
  return JSON.stringify(
    {
      file,
      ok: diagnostics.length === 0,
      diagnostics: diagnostics.map((d) => ({
        file,
        line: d.pos.line,
        col: d.pos.col,
        severity: "error",
        message: d.message,
      })),
    },
    null,
    2,
  );
}
