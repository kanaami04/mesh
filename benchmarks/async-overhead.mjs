// 「全関数async+全await」(色なし言語の素朴なコード生成) vs 素の同期関数 のコスト比較
// ケース1: 最悪ケース = 関数呼び出しだらけのCPU計算(fib)
// ケース2: 現実ケース = 1万件のデータ整形(小関数3層パイプライン)

function fibS(n) { return n < 2 ? n : fibS(n - 1) + fibS(n - 2); }
async function fibA(n) { return n < 2 ? n : (await fibA(n - 1)) + (await fibA(n - 2)); }

function subtotalS(p, q) { return p * q; }
function fmtS(x) { return "¥" + x.toLocaleString(); }
function lineS(item) { return item.name + ": " + fmtS(subtotalS(item.price, item.qty)); }
function renderS(items) { const out = []; for (const it of items) out.push(lineS(it)); return out.join("\n"); }

async function subtotalA(p, q) { return p * q; }
async function fmtA(x) { return "¥" + x.toLocaleString(); }
async function lineA(item) { return item.name + ": " + (await fmtA(await subtotalA(item.price, item.qty))); }
async function renderA(items) { const out = []; for (const it of items) out.push(await lineA(it)); return out.join("\n"); }

const items = Array.from({ length: 10_000 }, (_, i) => ({ name: "item" + i, price: 100 + (i % 900), qty: 1 + (i % 5) }));

async function bench(label, fn, runs) {
  await fn(); await fn(); // ウォームアップ
  const t0 = process.hrtime.bigint();
  for (let i = 0; i < runs; i++) await fn();
  const ms = Number(process.hrtime.bigint() - t0) / 1e6 / runs;
  console.log(`${label}: ${ms.toFixed(2)}ms/回`);
  return ms;
}

const runs = 20;
console.log("=== ケース1: fib(25) — 関数呼び出し24万回のCPU計算(最悪ケース) ===");
const s1 = await bench("同期        ", () => fibS(25), runs);
const a1 = await bench("全async+await", () => fibA(25), runs);
console.log(`→ ${(a1 / s1).toFixed(1)}倍遅い\n`);

console.log("=== ケース2: 1万件のカート明細レンダリング(現実のWebアプリ相当) ===");
const s2 = await bench("同期        ", () => renderS(items), runs);
const a2 = await bench("全async+await", () => renderA(items), runs);
console.log(`→ ${(a2 / s2).toFixed(1)}倍遅い(1リクエスト分の絶対差: ${(a2 - s2).toFixed(2)}ms)`);
