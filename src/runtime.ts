// 生成される JavaScript の先頭に埋め込むランタイム。
// Mesh の実行に必要な最小限の道具(チャネル・goroutine 起動・print など)。

export const PRELUDE = `// ===== Mesh runtime =====
// channel: capacity未指定は無制限バッファ(送信は常に即完了)。capacity指定時はGo互換の
// 本物のブロッキング送信(送信は空きができるまで待つ)。close後の受信は __CLOSED を返し続ける
class __Channel {
  #capacity;
  #closed = false;
  #buf = [];
  #recvQueue = []; // 値待ちの callback: (result) => void
  #sendQueue = []; // 空き待ちの送信: { value, resolve }
  constructor(capacity) {
    this.#capacity = capacity === undefined ? Infinity : capacity;
  }
  // 消費せずに「今すぐ受信できるか」だけを確認する(selectの非破壊スキャン用)
  isReady() {
    return this.#buf.length > 0 || this.#sendQueue.length > 0 || this.#closed;
  }
  // 即座に受信できるときだけ消費して返す。無ければ null
  tryRecv() {
    if (this.#buf.length > 0) {
      const value = this.#buf.shift();
      const pending = this.#sendQueue.shift();
      if (pending) {
        this.#buf.push(pending.value);
        pending.resolve();
      }
      return { value, closed: false };
    }
    const pending = this.#sendQueue.shift();
    if (pending) {
      pending.resolve();
      return { value: pending.value, closed: false };
    }
    if (this.#closed) return { value: null, closed: true };
    return null;
  }
  // callback を受信待ちに登録し、登録解除する関数を返す(select用)
  waitRecv(callback) {
    this.#recvQueue.push(callback);
    return () => {
      const i = this.#recvQueue.indexOf(callback);
      if (i !== -1) this.#recvQueue.splice(i, 1);
    };
  }
  send(v) {
    if (this.#closed) throw new __Panic("send on closed channel");
    const waiter = this.#recvQueue.shift();
    if (waiter) {
      waiter({ value: v, closed: false });
      return Promise.resolve();
    }
    if (this.#buf.length < this.#capacity) {
      this.#buf.push(v);
      return Promise.resolve();
    }
    // 満杯かつ受信待ちが居ない: 空きができるまで本当にブロックする
    return new Promise((resolve) => {
      this.#sendQueue.push({ value: v, resolve });
    });
  }
  recv() {
    const immediate = this.tryRecv();
    if (immediate) return Promise.resolve(immediate);
    return new Promise((resolve) => this.waitRecv(resolve));
  }
  close() {
    if (this.#closed) throw new __Panic("close of closed channel");
    this.#closed = true;
    for (const cb of this.#recvQueue) cb({ value: null, closed: true });
    this.#recvQueue = [];
    for (const pending of this.#sendQueue) pending.resolve();
    this.#sendQueue = [];
  }
}
// "channelがcloseされた" を表す一意な値。null(none)やErrorと絶対に衝突しない
const __CLOSED = Symbol("closed");
// <-ch は常に T | closed。__Channel.recv() の {value, closed} を Mesh の値に変換する
const __recv = async (ch) => {
  const r = await ch.recv();
  return r.closed ? __CLOSED : r.value;
};
// select: 準備できているチャネルがあれば(複数なら擬似ランダムに選び、Goと同じくcase飢餓を防ぐ)
// 即座にそのハンドラを実行。無ければ default、それも無ければ最初に準備できたチャネルまで待つ
const __select = async (channels, handlers, defaultHandler) => {
  const ready = [];
  for (let i = 0; i < channels.length; i++) {
    if (channels[i].isReady()) ready.push(i);
  }
  if (ready.length > 0) {
    const i = ready[Math.floor(Math.random() * ready.length)];
    const result = channels[i].tryRecv();
    return handlers[i](result.closed ? __CLOSED : result.value);
  }
  if (defaultHandler) return defaultHandler();
  return new Promise((resolve, reject) => {
    const unregisters = [];
    let settled = false;
    channels.forEach((ch, i) => {
      unregisters.push(
        ch.waitRecv((result) => {
          if (settled) return;
          settled = true;
          for (const u of unregisters) u();
          Promise.resolve(handlers[i](result.closed ? __CLOSED : result.value)).then(resolve, reject);
        }),
      );
    });
  });
};
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
// __prop:    f()? — none/error/構造化error型 なら __Propagate を投げて呼び出し元へ即 return させる
// __propCtx: f() ? "ctx" — 失敗を error("ctx[: 元メッセージ]") に包んで伝播(noneも昇格。
//            構造化error型はメッセージを持たないのでcheckerがこの形自体を弾く)
// __or:      f() or fallback — 失敗なら fallback の値(遅延評価。束縛形は失敗値を引数で受ける)
class __Propagate {
  constructor(value) {
    this.value = value;
  }
}
// F-2後半: error type/struct で宣言された構造化エラーの実体マーカー。struct リテラルの生成時に
// codegen が埋め込む(__errTag参照)。none(null)/組み込みerror(instanceof Error)と違って
// 構造化エラーはただのオブジェクトなので、Symbolキーで「失敗である」ことを実行時にも残す
const __ERR = Symbol("meshErrorType");
const __errTag = (obj) => ((obj[__ERR] = true), obj);
const __isFailureValue = (v) => v === null || v instanceof Error || (typeof v === "object" && v !== null && v[__ERR] === true);
const __prop = (v) => {
  if (__isFailureValue(v)) throw new __Propagate(v);
  return v;
};
// ctx は失敗時にだけ評価する(or の右辺と同じ遅延評価。補間に関数呼び出しがあっても
// 成功パスでは走らない)
const __propCtx = async (v, ctx) => {
  if (v === null) throw new __Propagate(new Error(await ctx()));
  if (v instanceof Error) throw new __Propagate(new Error((await ctx()) + ": " + v.message));
  return v;
};
const __or = async (v, fallback) => (__isFailureValue(v) ? fallback(v) : v);
// spawn f(x): await せずに起動し、結果の受取口(channel)を返す。
// 2段スコープ設計(2026-07-18): spawn は最も内側の wait スコープ
// (囲む関数の本体、または wait ブロック)に登録され、そのスコープを抜けるとき必ず待たれる。
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
// detach f(x): プログラム所有のタスク。どの wait スコープにも登録されず、呼び出し元は
// 待たずに戻れる。JSのイベントループはタイマー等が残る限りプロセスを生かすので、
// 「プログラム終了時までに完了する」は実行環境が自然に保証する
const __detach = (f, args) => {
  const task = new __Channel();
  Promise.resolve()
    .then(() => f(...args))
    .then((v) => {
      task.send(v);
    }, __panic);
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
// 高階関数(標準ライブラリ第三弾)。渡される関数値は Mesh の関数(すべて async)なので、
// 呼び出すたびに await する。JS のヘルパー自体も async にする
const __filter = async (arr, pred) => {
  const out = [];
  for (const x of arr) {
    if (await pred(x)) out.push(x);
  }
  return out;
};
const __map = async (arr, f) => {
  const out = [];
  for (const x of arr) out.push(await f(x));
  return out;
};
const __reduce = async (arr, f, init) => {
  let acc = init;
  for (const x of arr) acc = await f(acc, x);
  return acc;
};
const __fmt = (v) =>
  v === null || v === undefined ? "none"
  : v === __CLOSED ? "closed"
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
