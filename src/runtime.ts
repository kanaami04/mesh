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
// 標準ライブラリ第一弾(配列・map操作)
const __indexOf = (arr, v) => {
  const i = arr.indexOf(v);
  return i === -1 ? null : i;
};
// sort() は非破壊: 元の配列は変えず、並び替えたコピーを返す。
// < / > は int・float・string のどれでも正しく比較できるので単一の比較関数で足りる
const __sorted = (arr) => [...arr].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
// 文字列→int(標準ライブラリ第二弾)。厳密パース: 符号+数字のみ許可(小数点・空白・ゴミは拒否)
// 注意: このファイルは PRELUDE という1つのテンプレートリテラル文字列なので、
// 正規表現の中でバックスラッシュ1つの意味で書きたい箇所は、ソース上ではバックスラッシュを
// 2つ重ねて書く必要がある(外側の文字列リテラルが先に1段エスケープを解決してしまうため。
// 1つのままだと文字が消えて常に不一致になる)。
const __toInt = (s) => {
  if (!/^[+-]?\\d+$/.test(s)) return new Error('"' + s + '" is not a valid int');
  return parseInt(s, 10);
};
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
