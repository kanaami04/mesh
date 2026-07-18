// Playground 開発サーバー。
// /bundle.js はリクエストのたびに main.ts をバンドルし直すので、
// コンパイラ(src/)を変更してもブラウザをリロードするだけで反映される。

import { join } from "node:path";

const root = import.meta.dir;
const PORT = 8765;

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

    if (path === "/" || path === "/index.html") {
      return new Response(Bun.file(join(root, "index.html")));
    }

    return new Response("not found", { status: 404 });
  },
});

console.log(`Mesh playground: http://localhost:${PORT}`);
