// 生成される JavaScript の先頭に埋め込むランタイム。
// Mesh の実行に必要な最小限の道具(チャネル・goroutine 起動・print など)。

export const PRELUDE = `// ===== Mesh runtime =====
class __Channel {
  #values = [];
  #waiters = [];
  send(v) {
    const w = this.#waiters.shift();
    if (w) w(v);
    else this.#values.push(v);
  }
  recv() {
    if (this.#values.length > 0) return Promise.resolve(this.#values.shift());
    return new Promise((resolve) => this.#waiters.push(resolve));
  }
}
const __panic = (e) => {
  console.error("panic:", e instanceof Error ? e.message : e);
  globalThis.process?.exit?.(1);
};
// ランタイム検査(層1): バグは黙って進まず、位置つきで即停止する
class __Panic extends Error {}
const __idx = (target, i, at) => {
  if (!Number.isInteger(i) || i < 0 || i >= target.length) {
    throw new __Panic(at + ": index " + i + " out of range (length " + target.length + ")");
  }
  return target[i];
};
const __idxset = (target, i, value, at) => {
  if (!Number.isInteger(i) || i < 0 || i >= target.length) {
    throw new __Panic(at + ": index " + i + " out of range (length " + target.length + ")");
  }
  target[i] = value;
};
const __idiv = (a, b, at) => {
  if (b === 0) throw new __Panic(at + ": integer division by zero");
  return Math.trunc(a / b);
};
const __imod = (a, b, at) => {
  if (b === 0) throw new __Panic(at + ": integer modulo by zero");
  return a % b;
};
// union路線の道具:
// __prop: f()! — none/error なら __Propagate を投げて呼び出し元へ即 return させる
// __or:   f() or fallback — none/error なら fallback の値(遅延評価)
class __Propagate {
  constructor(value) {
    this.value = value;
  }
}
const __prop = (v) => {
  if (v === null || v instanceof Error) throw new __Propagate(v);
  return v;
};
const __or = async (v, fallback) => (v === null || v instanceof Error ? fallback() : v);
// spawn f(x): await せずに起動し、結果の受取口(channel)を返す。
// wait ブロックの中なら、そのブロックの完了待ちリストにも登録される
const __waitStack = [];
const __spawn = (f, args) => {
  const task = new __Channel();
  const p = Promise.resolve()
    .then(() => f(...args))
    .then((v) => {
      task.send(v);
    }, __panic);
  if (__waitStack.length > 0) __waitStack[__waitStack.length - 1].push(p);
  return task;
};
const __sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
// map の読み取り: 無いキーは none(null)を返す — V | none 型に対応
const __mget = (m, k) => (m.has(k) ? m.get(k) : null);
const __fmt = (v) =>
  v === null || v === undefined ? "none"
  : v instanceof Error ? v.message
  : Array.isArray(v) ? "[" + v.map(__fmt).join(" ") + "]"
  : v instanceof Map
    ? "map{" + [...v].map(([k, x]) => __fmt(k) + ": " + __fmt(x)).join(", ") + "}"
    : typeof v === "object"
      ? "{" + Object.entries(v).map(([k, x]) => k + ": " + __fmt(x)).join(", ") + "}"
      : String(v);
const __print = (...args) => console.log(args.map(__fmt).join(" "));
const __error = (msg) => new Error(msg);
// ===== end runtime =====

`;
