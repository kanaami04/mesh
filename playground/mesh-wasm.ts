// Rust版コンパイラ(wasm)をブラウザから呼ぶための薄いglue(TS撤去 段階4)。
//
// playgroundは以前 `import { compile } from "../src/compiler"` でTS実装を直接読んでいた。
// TS版を撤去するとplaygroundが動かなくなるので、Rust版のwasmへ切り替えた。
//
// **`wasm-bindgen`は使っていない**——Rust移植は依存クレート0で来ており、
// playgroundのために依存とビルドツールを増やす釣り合いが取れないため、
// `rust/src/wasm.rs` が手書きのABIを公開している。こちら側はその作法に合わせるだけ:
//
//   1. `mesh_alloc(len)` でwasm線形メモリ上にバッファを取る
//   2. そこへUTF-8のソースを書いて `mesh_compile(ptr, len)` を呼ぶ
//   3. 戻り値は結果文字列の先頭ポインタ。長さは `mesh_last_len()` で取る
//   4. 読み終わったら両方 `mesh_free` する
//
// **`memory.buffer` は毎回取り直す**——wasm側でallocするとメモリが拡張され、
// 以前取得したArrayBufferがdetachされることがあるため(掴んだまま使うと空になる)。

interface MeshExports {
  memory: WebAssembly.Memory;
  mesh_alloc(len: number): number;
  mesh_free(ptr: number, len: number): void;
  mesh_last_len(): number;
  mesh_compile(ptr: number, len: number): number;
}

export interface CompileResult {
  /** 生成されたJS。診断があれば null */
  code: string | null;
  /** 人間可読な診断(1行1件)。成功時は空文字列 */
  diagnostics: string;
}

let exportsPromise: Promise<MeshExports> | null = null;

function load(wasmUrl: string): Promise<MeshExports> {
  exportsPromise ??= WebAssembly.instantiateStreaming(fetch(wasmUrl), {})
    .catch(async () => {
      // content-type が application/wasm でない配信環境へのフォールバック
      const bytes = await fetch(wasmUrl).then((r) => r.arrayBuffer());
      return WebAssembly.instantiate(bytes, {});
    })
    .then((r) => r.instance.exports as unknown as MeshExports);
  return exportsPromise;
}

export async function initCompiler(wasmUrl = "/mesh.wasm"): Promise<(source: string) => CompileResult> {
  const ex = await load(wasmUrl);
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  return (source: string): CompileResult => {
    const bytes = encoder.encode(source);
    const ptr = ex.mesh_alloc(bytes.length);
    new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);

    const outPtr = ex.mesh_compile(ptr, bytes.length);
    const outLen = ex.mesh_last_len();
    const out = decoder.decode(new Uint8Array(ex.memory.buffer, outPtr, outLen));

    ex.mesh_free(ptr, bytes.length);
    ex.mesh_free(outPtr, outLen);

    // 1行目が `ok` / `error`、2行目以降が本体(rust/src/wasm.rs と対の取り決め)
    const nl = out.indexOf("\n");
    const head = nl === -1 ? out : out.slice(0, nl);
    const body = nl === -1 ? "" : out.slice(nl + 1);
    return head === "ok" ? { code: body, diagnostics: "" } : { code: null, diagnostics: body };
  };
}
