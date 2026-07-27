// Playground 開発サーバー。
// /bundle.js はリクエストのたびに main.ts をバンドルし直すので、
// 配線(playground/)を変更してもブラウザをリロードするだけで反映される。
//
// **コンパイラ本体はRust版のwasm**(TS撤去 段階4)。`/mesh.wasm` として配信する。
// 起動時に見つからなければビルド方法を案内して落とす——黙って404を返すと
// ブラウザ側で「読み込みに失敗した」としか分からず、原因に辿り着きにくいため。

import { existsSync } from "node:fs";
import { join } from "node:path";

const root = import.meta.dir;
const repoRoot = join(root, "..");
const PORT = 8765;

const WASM_PATH = join(repoRoot, "rust/target/wasm32-unknown-unknown/release/mesh.wasm");

if (!existsSync(WASM_PATH)) {
  console.error(
    `コンパイラのwasmが無い: ${WASM_PATH}\n` +
      "先にビルドすること:\n" +
      "  rustup target add wasm32-unknown-unknown\n" +
      "  cargo build --manifest-path rust/Cargo.toml --target wasm32-unknown-unknown --lib --release\n" +
      "(`mise run playground` はこれを自動で行う)",
  );
  process.exit(1);
}

Bun.serve({
  port: PORT,
  async fetch(req) {
    const path = new URL(req.url).pathname;

    if (path === "/bundle.js") {
      const result = await Bun.build({
        entrypoints: [join(root, "main.ts")],
        target: "browser",
      });
      if (!result.success) {
        console.error(result.logs.join("\n"));
        return new Response("build failed — see server logs", { status: 500 });
      }
      return new Response(result.outputs[0], {
        headers: { "content-type": "text/javascript; charset=utf-8" },
      });
    }

    // `WebAssembly.instantiateStreaming` を効かせるため content-type を明示する
    if (path === "/mesh.wasm") {
      return new Response(Bun.file(WASM_PATH), {
        headers: { "content-type": "application/wasm" },
      });
    }

    if (path === "/" || path === "/index.html") {
      return new Response(Bun.file(join(root, "index.html")));
    }

    return new Response("not found", { status: 404 });
  },
});

console.log(`Mesh playground: http://localhost:${PORT}`);
