// Mesh CLI:
//   mesh run   file.mesh          コンパイルして即実行
//   mesh build file.mesh [-o out] JavaScript を書き出す
//   mesh check file.mesh          型検査のみ

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { LANGUAGE_CARD } from "./card";
import { buildSubsetCard } from "./card-subset";
import {
  compileModules,
  diagnosticsToJson,
  formatDiagnostics,
  type CompileResult,
  type ModuleSource,
} from "./compiler";
import { DIAGNOSTIC_EXPLANATIONS, type DiagnosticCode } from "./diagnostic-codes";
import { parse } from "./parser";
import { BUILTIN_PACKAGES } from "./stdlib";

const USAGE = `Mesh compiler v0.1.0

Usage:
  mesh run     <file.mesh>            compile and run
  mesh build   <file.mesh> [-o out]   compile to JavaScript
  mesh check   <file.mesh> [--json]   type-check only (--json: AIエージェント向けの構造化出力。
                                       診断ごとに code と、機械適用可能なら fix パッチを含む)
  mesh explain <code>                 診断コードの意味を説明する(引数無しで全コード一覧)
  mesh card                           言語カードを出力(AIのコンテキストに貼る圧縮仕様書)
  mesh card --for <file.mesh>...      渡したソースが使っている機能のセクションだけに絞った
                                       縮小版カードを出力(トークン節約。完全版ではない旨を明記)
`;

// エントリファイルと、そこから(推移的に)importされたパッケージのソースを集める。
// プロジェクトルート = エントリファイルのディレクトリ。パッケージ = ルート直下のディレクトリで、
// その中の全 .mesh ファイルが1パッケージの名前空間を成す(エントリ自身は "main" の1ファイル)。
// import パスの発見のためだけに軽くパースする(構文エラーはここでは無視し、
// compileModules 側で正式に報告させる)
function loadModules(entryFile: string): ModuleSource[] {
  let entrySource: string;
  try {
    entrySource = readFileSync(entryFile, "utf8");
  } catch {
    console.error(`error: cannot read file '${entryFile}'`);
    process.exit(1);
  }
  const root = dirname(entryFile);
  const modules: ModuleSource[] = [{ pkg: "main", file: entryFile, source: entrySource }];

  const importsOf = (source: string): string[] => {
    try {
      return parse(source).imports.map((i) => i.path);
    } catch {
      return []; // 構文エラーは compileModules が位置つきで報告する
    }
  };

  const loaded = new Set<string>();
  const queue = importsOf(entrySource);
  while (queue.length > 0) {
    const path = queue.shift()!;
    if (loaded.has(path)) continue;
    loaded.add(path);
    // F-14: 組み込みパッケージ(mesh/io, mesh/json)は .mesh ソースを持たない —
    // ディスクから読まず、checker側がsrc/stdlib.tsから直接シグネチャを登録する
    if (BUILTIN_PACKAGES.has(path)) continue;
    if (path === "mesh" || path.startsWith("mesh/")) {
      console.error(`error: standard-library module '${path}' is not available yet`);
      process.exit(1);
    }
    if (path.includes("/")) {
      console.error(
        `error: nested package paths ('${path}') are not supported yet — packages are single directories under the project root`,
      );
      process.exit(1);
    }
    const dir = join(root, path);
    if (!existsSync(dir) || !statSync(dir).isDirectory()) {
      console.error(`error: cannot find package '${path}' (expected directory '${dir}' with .mesh files)`);
      process.exit(1);
    }
    const files = readdirSync(dir).filter((f) => f.endsWith(".mesh")).sort();
    if (files.length === 0) {
      console.error(`error: package '${path}' has no .mesh files (in '${dir}')`);
      process.exit(1);
    }
    for (const f of files) {
      const filePath = join(dir, f);
      const source = readFileSync(filePath, "utf8");
      modules.push({ pkg: path, file: filePath, source });
      queue.push(...importsOf(source));
    }
  }
  return modules;
}

function compileEntry(file: string): CompileResult {
  return compileModules(loadModules(file));
}

function compileFile(file: string): string {
  const result = compileEntry(file);
  if (result.code === null) {
    console.error(formatDiagnostics(file, result.diagnostics));
    process.exit(1);
  }
  return result.code;
}

function main() {
  const [command, file, ...rest] = process.argv.slice(2);

  // card は通常ファイル引数を取らない。--for <file...>(F-13後半)のときだけ、
  // 渡したソースが使っている機能のセクションに絞った縮小版を返す
  if (command === "card") {
    if (file === "--for") {
      if (rest.length === 0) {
        console.error("usage: mesh card --for <file.mesh> [<file2.mesh> ...]");
        process.exit(1);
      }
      const sources = rest.map((f) => {
        try {
          return readFileSync(f, "utf8");
        } catch {
          console.error(`error: cannot read file '${f}'`);
          return process.exit(1);
        }
      });
      console.log(buildSubsetCard(sources));
      return;
    }
    console.log(LANGUAGE_CARD);
    return;
  }

  // explain の第2引数はファイルではなく診断コード(F-13)。引数無しなら全コード一覧を出す
  if (command === "explain") {
    const code = file;
    if (!code) {
      const codes = Object.keys(DIAGNOSTIC_EXPLANATIONS).sort();
      console.log(`${codes.length} diagnostic codes. Run 'mesh explain <code>' for details.\n`);
      console.log(codes.join("\n"));
      return;
    }
    if (!Object.hasOwn(DIAGNOSTIC_EXPLANATIONS, code)) {
      console.error(`error: unknown diagnostic code '${code}' (run 'mesh explain' with no code to list them all)`);
      process.exit(1);
    }
    console.log(DIAGNOSTIC_EXPLANATIONS[code as DiagnosticCode]);
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
      // file の後に続く追加引数はプログラム自身の引数として渡す(io.args()で読める。F-14)
      const proc = spawnSync(process.execPath, [outPath, ...rest], { stdio: "inherit" });
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
        const result = compileEntry(file);
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
