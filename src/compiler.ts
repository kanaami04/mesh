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
