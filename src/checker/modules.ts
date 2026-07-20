// パッケージ全体の検査: 複数パッケージの依存グラフ検証(checkModules)と、
// 1パッケージ内の複数ファイルをフラットな1名前空間として検査する本体(checkPackage)

import type { Program } from "../ast";
import type { Diagnostic } from "../diagnostic-codes";
import { BUILTIN_PACKAGES } from "../stdlib";
import { ANY, ERROR, NONE, assignable, typeEquals, typeToString, unionOf, type Type } from "../types";
import {
  BUILTIN_TYPE_NAMES,
  createCheckerCtx,
  declareBinding,
  error,
  popTypeParams,
  pushTypeParams,
  type CheckResult,
  type CheckerCtx,
  type PackageSymbols,
  type ParsedModule,
  type TestInfo,
} from "./context";
import { declareMethod, checkFn } from "./functions";
import { fnType, resolveAlias, resolveType } from "./types-resolve";
import { validateTypeParams } from "./generics";
import { checkExprSingle } from "./expressions";

// 単一ファイル(従来のAPI)。"main" パッケージ1ファイルとして検査する
export function check(program: Program): Diagnostic[] {
  return checkModules([{ pkg: "main", file: "main.mesh", program }]).diagnostics;
}

export function checkModules(modules: ParsedModule[], opts?: { testMode?: boolean }): CheckResult {
  const diagnostics: Diagnostic[] = [];
  const tests: TestInfo[] = []; // F-15

  // パッケージごとにファイルをまとめる(同一パッケージ内はimport不要のフラット名前空間)
  const packages = new Map<string, ParsedModule[]>();
  for (const m of modules) {
    const list = packages.get(m.pkg) ?? [];
    list.push(m);
    packages.set(m.pkg, list);
  }

  // 組み込みパッケージ(mesh/io, mesh/json)のエイリアス名と同じ名前のユーザーパッケージを検出する。
  // registryはエイリアス名だけをキーにした単一のMapなので、検出しないと検査は素通りし、
  // 生成JSが同名関数の二重宣言でロード時に壊れる(検査が通ったのにcrashするP4違反)
  const builtinPathByAlias = new Map([...BUILTIN_PACKAGES.keys()].map((path) => [path.split("/").pop()!, path]));
  for (const [pkgName, files] of packages) {
    const builtinPath = builtinPathByAlias.get(pkgName);
    if (builtinPath) {
      diagnostics.push({
        pos: { line: 1, col: 1 },
        code: "package-name-reserved",
        file: files[0]?.file,
        message: `package name '${pkgName}' collides with the built-in package '${builtinPath}' — ` +
          `rename the '${pkgName}/' directory (built-in package names are reserved)`,
      });
    }
  }
  if (diagnostics.length > 0) return { diagnostics, tests: [] };

  // import グラフの検証: 未知のパッケージ・不正なパッケージ名・循環
  const deps = new Map<string, Set<string>>();
  for (const [pkg, files] of packages) {
    const set = new Set<string>();
    for (const { file, program } of files) {
      for (const imp of program.imports) {
        if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(imp.alias)) {
          diagnostics.push({
            pos: imp.pos,
            code: "invalid-package-name",
            file,
            message: `package name '${imp.alias}' cannot be used as an identifier — rename the directory`,
          });
          continue;
        }
        if (imp.alias === pkg) {
          diagnostics.push({
            pos: imp.pos,
            code: "self-import",
            file,
            message: `package '${pkg}' cannot import itself`,
          });
          continue;
        }
        if (!packages.has(imp.alias) && !BUILTIN_PACKAGES.has(imp.path)) {
          diagnostics.push({
            pos: imp.pos,
            code: "unknown-package",
            file,
            message: `unknown package '${imp.path}'`,
          });
          continue;
        }
        // F-14: 組み込みパッケージ(mesh/io, mesh/json)は.messソースを持たないので
        // ユーザーパッケージの依存グラフ(循環検出・検査順)には含めない — registryへは
        // このループの外で事前に登録済み
        if (packages.has(imp.alias)) set.add(imp.alias);
      }
    }
    deps.set(pkg, set);
  }
  if (diagnostics.length > 0) return { diagnostics, tests: [] };

  // 依存順(importされる側が先)に並べる + 循環検出
  const order: string[] = [];
  const state = new Map<string, "visiting" | "done">();
  const visit = (pkg: string, chain: string[]): boolean => {
    if (state.get(pkg) === "done") return true;
    if (state.get(pkg) === "visiting") {
      const cycle = [...chain.slice(chain.indexOf(pkg)), pkg].join(" -> ");
      diagnostics.push({
        pos: { line: 1, col: 1 },
        code: "import-cycle",
        file: packages.get(pkg)?.[0]?.file,
        message: `import cycle: ${cycle}`,
      });
      return false;
    }
    state.set(pkg, "visiting");
    for (const dep of deps.get(pkg) ?? []) {
      if (!visit(dep, [...chain, pkg])) return false;
    }
    state.set(pkg, "done");
    order.push(pkg);
    return true;
  };
  for (const pkg of packages.keys()) {
    if (!visit(pkg, [])) return { diagnostics, tests: [] };
  }

  // 依存順に検査。メソッド表は全パッケージ共有(struct名はパッケージ修飾済みで衝突しない)。
  // F-14: 組み込みパッケージ(.messソースを持たない)は事前にエイリアス名でregistryへ登録しておく
  // (path "mesh/io" → alias "io"。checkPackageは通さない — 検査すべきMeshソースが無いため)
  const registry = new Map<string, PackageSymbols>();
  for (const [path, symbols] of BUILTIN_PACKAGES) {
    registry.set(path.split("/").pop()!, symbols);
  }
  const sharedMethods = new Map<string, Map<string, Type>>();
  for (const pkg of order) {
    const files = packages.get(pkg)!;
    const importAliases = new Set<string>();
    for (const { program } of files) {
      for (const imp of program.imports) importAliases.add(imp.alias);
    }
    const ctx = createCheckerCtx(pkg, registry, sharedMethods, importAliases);
    // F-15: mesh test はmain()の存在を要求しない(TDD的にテストだけ先に書ける。
    // ライブラリパッケージ単体のテストにもmain()は無いのが普通)
    const result = checkPackage(ctx, files, { requireMain: pkg === "main" && !opts?.testMode });
    diagnostics.push(...result.diagnostics);
    tests.push(...result.tests);
  }
  return { diagnostics, tests };
}

// ---- パッケージ全体 ----
// 同一パッケージ内の複数ファイルはフラットな1名前空間として検査する。
// フェーズ順は単一ファイル時代と同じ(型登録→エイリアス解決→関数登録→本体)を
// 全ファイル横断に広げただけ — ファイルをまたぐ前方参照・相互再帰も自然に許される

export function checkPackage(
  ctx: CheckerCtx,
  files: { file: string; program: Program }[],
  opts: { requireMain: boolean },
): CheckResult {
  // 先に type 宣言を登録する(関数シグネチャがエイリアスを参照できるように)
  for (const { file, program } of files) {
    ctx.currentFile = file;
    for (const td of program.types) {
      if (BUILTIN_TYPE_NAMES.has(td.name)) {
        error(ctx, td.pos, "builtin-type-redeclared", `'${td.name}' is a builtin type and cannot be redeclared`);
        continue;
      }
      if (ctx.importAliases.has(td.name)) {
        error(
          ctx,
          td.pos,
          "name-conflicts-with-package",
          `'${td.name}' conflicts with an imported package name`,
        );
        continue;
      }
      if (ctx.typeTable.has(td.name)) {
        error(ctx, td.pos, "already-declared", `type '${td.name}' is already declared`);
        continue;
      }
      ctx.typeTable.set(td.name, td.node);
      ctx.typeExported.set(td.name, td.exported);
      if (td.isError) ctx.errorTypeNames.add(td.name);
    }
  }
  // 全エイリアスを解決しておく(未使用でも循環や未知型を報告するため)
  for (const { file, program } of files) {
    ctx.currentFile = file;
    for (const td of program.types) {
      if (ctx.typeTable.get(td.name) === td.node) resolveAlias(ctx, td.name, td.pos);
    }
  }

  // 先に全関数/メソッドのシグネチャを登録する(前方参照・相互再帰を許すため)
  for (const { file, program } of files) {
    ctx.currentFile = file;
    for (const fn of program.fns) {
      if (fn.receiver) {
        declareMethod(ctx, fn);
      } else {
        pushTypeParams(ctx, fn.typeParams); // <T>のTをこのシグネチャ解決の間だけ型として認識
        const t = fnType(ctx, fn.params, fn.ret);
        popTypeParams(ctx);
        if (fn.typeParams.length > 0) {
          validateTypeParams(ctx, fn, t);
          ctx.genericFns.set(fn.name, { typeParams: fn.typeParams, type: t });
        }
        declareBinding(ctx, fn.name, t, fn.pos);
        ctx.fnDecls.set(fn.name, { type: t, exported: fn.exported });
        // F-15: `_test.mesh` 内の "test" で始まるトップレベル fn は `mesh test` が実行対象にする。
        // シグネチャは常に `() none | error`(P1: テストの合否表現をunion路線から増やさない —
        // 既存の absence/failure 表現をそのまま流用する。none=合格、error=失敗)
        if (file.endsWith("_test.mesh") && fn.name.startsWith("test")) {
          const ret = t.kind === "fn" ? t.ret : ANY; // fnTypeは常にkind:"fn"を返す(到達しない分岐)
          if (fn.params.length > 0 || !typeEquals(ret, unionOf([NONE, ERROR]))) {
            error(
              ctx,
              fn.pos,
              "invalid-test-signature",
              `test function '${fn.name}' must take no parameters and return 'none | error', got ` +
                `(${fn.params.map((p) => typeToString(resolveType(ctx, p.type))).join(", ")}) ${typeToString(ret)}`,
            );
          } else {
            ctx.discoveredTests.push({
              name: fn.name,
              jsName: ctx.pkg === "main" ? fn.name : `${ctx.pkg}$${fn.name}`,
              file,
              pos: fn.pos,
            });
          }
        }
      }
    }
  }

  // F-9c: トップレベル定数。関数シグネチャの後・関数本体の検査より前に登録するので、
  // 関数本体からは宣言順に関係なく参照できる(関数同士の相互参照と同じ扱い)。
  // 定数が他の定数を参照する場合はJSのconst文になる都合上、先に書かれている必要がある
  // (このfor自体がファイル順・宣言順に処理するので、その順序がそのまま要求になる)
  for (const { file, program } of files) {
    ctx.currentFile = file;
    for (const c of program.consts) {
      const declared = c.typeNode ? resolveType(ctx, c.typeNode) : null;
      const valueType = checkExprSingle(ctx, c.value);
      if (declared && !assignable(valueType, declared)) {
        error(
          ctx,
          c.value.pos,
          "type-mismatch",
          `cannot use ${typeToString(valueType)} as ${typeToString(declared)}`,
        );
      }
      const finalType = declared ?? valueType;
      declareBinding(ctx, c.name, finalType, c.pos, false);
      ctx.constDecls.set(c.name, { type: finalType, exported: c.exported });
    }
  }

  if (opts.requireMain) {
    const withMain = files.find(({ program }) =>
      program.fns.some((f) => f.name === "main" && !f.receiver),
    );
    if (!withMain) {
      ctx.currentFile = files[0]?.file ?? ctx.currentFile;
      error(
        ctx,
        { line: 1, col: 1 },
        "missing-main",
        "missing 'fn main()' — Mesh programs start from main",
      );
    } else {
      const main = withMain.program.fns.find((f) => f.name === "main" && !f.receiver)!;
      if (main.params.length > 0 || main.ret !== null) {
        ctx.currentFile = withMain.file;
        error(ctx, main.pos, "invalid-main-signature", "'fn main()' must take no parameters and return nothing");
      }
    }
  }

  for (const { file, program } of files) {
    ctx.currentFile = file;
    for (const fn of program.fns) checkFn(ctx, fn);
  }

  // このパッケージのシンボル表を登録(後続のパッケージから import で参照される)
  const types = new Map<string, { type: Type; exported: boolean }>();
  for (const [name, exported] of ctx.typeExported) {
    const type = ctx.resolvedAliases.get(name);
    if (type) types.set(name, { type, exported });
  }
  ctx.registry.set(ctx.pkg, { types, fns: ctx.fnDecls, consts: ctx.constDecls });
  return { diagnostics: ctx.diagnostics, tests: ctx.discoveredTests };
}
