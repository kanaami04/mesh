// Mesh CLI:
//   mesh run   file.mesh          コンパイルして即実行
//   mesh build file.mesh [-o out] JavaScript を書き出す
//   mesh check file.mesh          型検査のみ

import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { LANGUAGE_CARD } from "./card";
import { compile, diagnosticsToJson, formatDiagnostics } from "./compiler";

const USAGE = `Mesh compiler v0.1.0

Usage:
  mesh run   <file.mesh>            compile and run
  mesh build <file.mesh> [-o out]   compile to JavaScript
  mesh check <file.mesh> [--json]   type-check only (--json: AIエージェント向けの構造化出力)
  mesh card                         言語カードを出力(AIのコンテキストに貼る圧縮仕様書)
`;

function compileFile(file: string): string {
  let source: string;
  try {
    source = readFileSync(file, "utf8");
  } catch {
    console.error(`error: cannot read file '${file}'`);
    process.exit(1);
  }
  const result = compile(source, file);
  if (result.code === null) {
    console.error(formatDiagnostics(file, result.diagnostics));
    process.exit(1);
  }
  return result.code;
}

function main() {
  const [command, file, ...rest] = process.argv.slice(2);

  // card はファイル引数を取らない
  if (command === "card") {
    console.log(LANGUAGE_CARD);
    return;
  }

  if (!command || !file) {
    console.error(USAGE);
    process.exit(1);
  }

  switch (command) {
    case "run": {
      const code = compileFile(file);
      const dir = mkdtempSync(join(tmpdir(), "mesh-"));
      const outPath = join(dir, basename(file).replace(/\.mesh$/, "") + ".mjs");
      writeFileSync(outPath, code);
      const proc = spawnSync(process.execPath, [outPath], { stdio: "inherit" });
      process.exit(proc.status ?? 0);
    }
    case "build": {
      const code = compileFile(file);
      const outIndex = rest.indexOf("-o");
      const outPath =
        outIndex !== -1 && rest[outIndex + 1]
          ? rest[outIndex + 1]
          : file.replace(/\.mesh$/, "") + ".mjs";
      writeFileSync(outPath, code);
      console.log(`wrote ${outPath}`);
      break;
    }
    case "check": {
      if (rest.includes("--json")) {
        // AIエージェント向け: 成否にかかわらず構造化JSONを stdout に出す
        let source: string;
        try {
          source = readFileSync(file, "utf8");
        } catch {
          console.error(`error: cannot read file '${file}'`);
          process.exit(1);
        }
        const result = compile(source, file);
        console.log(diagnosticsToJson(file, result.diagnostics));
        process.exit(result.diagnostics.length > 0 ? 1 : 0);
      }
      compileFile(file);
      console.log(`${file}: no errors`);
      break;
    }
    default:
      console.error(USAGE);
      process.exit(1);
  }
}

main();
