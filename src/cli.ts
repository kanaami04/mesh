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
import { format } from "./formatter";
import { parse } from "./parser";
import { BUILTIN_PACKAGES } from "./stdlib";
import { CompileError } from "./token";

// F-15: `__runTests`(runtime.ts)がstdoutの最終行に書く構造化結果と対応する形
interface TestReport {
  ok: boolean;
  tests: { name: string; file: string; pass: boolean; message?: string }[];
}

const USAGE = `Mesh compiler v0.1.0

Usage:
  mesh run     <file.mesh>            compile and run
  mesh build   <file.mesh> [-o out]   compile to JavaScript
  mesh check   <file.mesh> [--json]   type-check only (--json: AIエージェント向けの構造化出力。
                                       診断ごとに code と、機械適用可能なら fix パッチを含む)
  mesh test    <file.mesh|dir> [--json]  '_test.mesh' 内の fn test...() を実行する(F-15)。
                                       ディレクトリを渡せばそのパッケージ自身をテストする
  mesh fmt     <file.mesh> [-w]        正規形に整形して標準出力へ(gofmt相当。設定オプション
                                       なし)。-w で元ファイルに書き戻す
  mesh explain <code>                 診断コードの意味を説明する(引数無しで全コード一覧)
  mesh card                           言語カードを出力(AIのコンテキストに貼る圧縮仕様書)
  mesh card --for <file.mesh>...      渡したソースが使っている機能のセクションだけに絞った
                                       縮小版カードを出力(トークン節約。完全版ではない旨を明記)
`;

// import パスの発見のためだけに軽くパースする(構文エラーはここでは無視し、
// compileModules 側で正式に報告させる)
function importsOf(source: string): string[] {
  try {
    return parse(source).imports.map((i) => i.path);
  } catch {
    return [];
  }
}

// F-15: `_test.mesh` はテスト専用 — 通常のimport解決(run/build/check)には含めない。
// includeTests は `mesh test` がテスト対象パッケージ自身を読むときだけ true にする
function readMeshFiles(dir: string, opts: { includeTests: boolean }): string[] {
  return readdirSync(dir)
    .filter((f) => f.endsWith(".mesh") && (opts.includeTests || !f.endsWith("_test.mesh")))
    .sort();
}

// import グラフを再帰的に辿ってパッケージのソースを集める(依存先は常に本体コードのみ —
// テストファイルを含めるのは`mesh test`が直接指定した対象パッケージだけ)
function loadDependencies(root: string, initialQueue: string[]): ModuleSource[] {
  const modules: ModuleSource[] = [];
  const loaded = new Set<string>();
  const queue = [...initialQueue];
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
    const files = readMeshFiles(dir, { includeTests: false });
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

// エントリファイルと、そこから(推移的に)importされたパッケージのソースを集める。
// プロジェクトルート = エントリファイルのディレクトリ。パッケージ = ルート直下のディレクトリで、
// その中の全 .mesh ファイルが1パッケージの名前空間を成す(エントリ自身は "main" の1ファイル)。
function loadModules(entryFile: string): ModuleSource[] {
  let entrySource: string;
  try {
    entrySource = readFileSync(entryFile, "utf8");
  } catch {
    console.error(`error: cannot read file '${entryFile}'`);
    process.exit(1);
  }
  const modules: ModuleSource[] = [{ pkg: "main", file: entryFile, source: entrySource }];
  modules.push(...loadDependencies(dirname(entryFile), importsOf(entrySource)));
  return modules;
}

// F-15: mesh test 用のロード。<path>がファイルなら"main"パッケージ(エントリ自身 +
// 同じディレクトリの`*_test.mesh`を追加)、ディレクトリなら**そのパッケージ自身**
// (本体・テスト両方の.meshファイル)をテスト対象にする。依存先パッケージは通常どおり
// (loadDependencies経由)本体コードのみ読む — 依存先自身のテストは実行しない
function loadModulesForTest(targetPath: string): ModuleSource[] {
  if (!existsSync(targetPath)) {
    console.error(`error: cannot find '${targetPath}'`);
    process.exit(1);
  }
  const isDir = statSync(targetPath).isDirectory();
  const dir = isDir ? targetPath : dirname(targetPath);
  const root = isDir ? dirname(targetPath) : dir; // 依存先解決の基準(=プロジェクトルート)

  const target: ModuleSource[] = [];
  if (isDir) {
    const pkg = basename(targetPath);
    const files = readMeshFiles(dir, { includeTests: true });
    if (files.length === 0) {
      console.error(`error: '${dir}' has no .mesh files`);
      process.exit(1);
    }
    for (const f of files) {
      const filePath = join(dir, f);
      target.push({ pkg, file: filePath, source: readFileSync(filePath, "utf8") });
    }
  } else {
    const entrySource = readFileSync(targetPath, "utf8");
    target.push({ pkg: "main", file: targetPath, source: entrySource });
    const entryName = basename(targetPath);
    for (const f of readMeshFiles(dir, { includeTests: true })) {
      if (f === entryName || !f.endsWith("_test.mesh")) continue; // 追加するのは_test.meshのみ
      const filePath = join(dir, f);
      target.push({ pkg: "main", file: filePath, source: readFileSync(filePath, "utf8") });
    }
  }
  const queue = target.flatMap((m) => importsOf(m.source));
  return [...target, ...loadDependencies(root, queue)];
}

function compileEntry(file: string): { result: CompileResult; sources: Map<string, string> } {
  const modules = loadModules(file);
  return { result: compileModules(modules), sources: new Map(modules.map((m) => [m.file, m.source])) };
}

function compileFile(file: string): string {
  const { result, sources } = compileEntry(file);
  if (result.code === null) {
    console.error(formatDiagnostics(file, result.diagnostics, sources));
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
        const { result } = compileEntry(file);
        console.log(diagnosticsToJson(file, result.diagnostics));
        process.exit(result.diagnostics.length > 0 ? 1 : 0);
      }
      compileFile(file);
      console.log(`${file}: no errors`);
      break;
    }
    case "fmt": {
      let source: string;
      try {
        source = readFileSync(file, "utf8");
      } catch {
        console.error(`error: cannot read file '${file}'`);
        process.exit(1);
      }
      let formatted: string;
      try {
        formatted = format(source);
      } catch (e) {
        if (e instanceof CompileError) {
          console.error(formatDiagnostics(file, [{ pos: e.pos, code: e.code, message: e.message, file, fix: e.fix }]));
          process.exit(1);
        }
        throw e;
      }
      if (rest.includes("-w")) {
        writeFileSync(file, formatted);
      } else {
        process.stdout.write(formatted);
      }
      break;
    }
    case "test": {
      const jsonMode = rest.includes("--json");
      const testModules = loadModulesForTest(file);
      const result = compileModules(testModules, { testMode: true });
      if (result.code === null) {
        if (jsonMode) {
          console.log(diagnosticsToJson(file, result.diagnostics));
        } else {
          const sources = new Map(testModules.map((m) => [m.file, m.source]));
          console.error(formatDiagnostics(file, result.diagnostics, sources));
        }
        process.exit(1);
      }
      const dir = mkdtempSync(join(tmpdir(), "mesh-test-"));
      const outPath = join(dir, "test.mjs");
      writeFileSync(outPath, result.code);
      const proc = spawnSync(process.execPath, [outPath], { encoding: "utf8" });
      // __runTests は常に最後の行として構造化JSONを1つ書く({ok, tests: [...]})。
      // テスト自身がprint()した出力がそれより前に混ざっていてもよい(最後の行だけ見ればよい)
      const lastLine = proc.stdout.trim().split("\n").pop() ?? "";
      let report: TestReport | null = null;
      try {
        report = JSON.parse(lastLine);
      } catch {
        // ならない想定(harness自体のバグ)だが、念のため生の出力を見せる
      }
      if (!report) {
        console.error(`mesh test: internal error — could not read test results\n${proc.stdout}${proc.stderr}`);
        process.exit(1);
      }
      // 防衛的な二重チェック: __runTests側の隔離ロジックに将来また穴があっても、
      // JSONの ok を盲信してプロセスの実際の終了コードと食い違うまま緑判定にはしない
      if (report.ok && proc.status !== 0) {
        report.ok = false;
        report.tests.push({
          name: "(process)",
          file,
          pass: false,
          message: `test process exited with code ${proc.status} despite reporting success` +
            (proc.stderr.trim() ? ` — stderr: ${proc.stderr.trim()}` : ""),
        });
      }
      if (jsonMode) {
        console.log(JSON.stringify(report));
      } else if (report.tests.length === 0) {
        console.log(`no tests found (looked for 'fn test...() none | error' in *_test.mesh files)`);
      } else {
        for (const t of report.tests) {
          console.log(`${t.pass ? "ok  " : "FAIL"} ${t.name} (${t.file})`);
          if (!t.pass && t.message) console.log(`     ${t.message}`);
        }
        const passed = report.tests.filter((t) => t.pass).length;
        console.log(`\n${passed}/${report.tests.length} passed`);
      }
      process.exit(report.ok ? 0 : 1);
    }
    default:
      console.error(USAGE);
      process.exit(1);
  }
}

main();
