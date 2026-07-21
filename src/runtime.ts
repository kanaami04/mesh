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
// F-15フォローアップ: mesh test実行中はspawn/detachの中で起きたpanicも__panicを経由するが、
// ここでプロセスをexitさせると「1件の失敗として隔離する」という約束(runtests参照)を破る。
// __panicSinkが立っている間(mesh testの実行中だけ)はexitせず記録だけする
let __panicSink = null;
let __bgTasks = null;
const __panic = (e) => {
  const msg = e instanceof Error ? e.message : String(e);
  if (__panicSink) {
    __panicSink.push(msg);
    return;
  }
  console.error("panic:", msg);
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
// F-10: intはJSのnumberなので53bitを超えると静かに丸まる。演算結果がsafe integerの
// 範囲を超えたら(範囲外アクセス・ゼロ除算と同じ)即panicする
const __iarith = (a, op, b, at) => {
  const r = op === "+" ? a + b : op === "-" ? a - b : a * b;
  if (!Number.isSafeInteger(r)) {
    throw new __Panic(at + ": integer overflow — result " + r + " exceeds the safe integer range");
  }
  return r;
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
  if (__bgTasks) __bgTasks.push(p); // mesh test実行中: 決着をrunTestsが待てるように登録
  return task;
};
// detach f(x): プログラム所有のタスク。どの wait スコープにも登録されず、呼び出し元は
// 待たずに戻れる。JSのイベントループはタイマー等が残る限りプロセスを生かすので、
// 「プログラム終了時までに完了する」は実行環境が自然に保証する
const __detach = (f, args) => {
  const task = new __Channel();
  const p = Promise.resolve()
    .then(() => f(...args))
    .then((v) => {
      task.send(v);
    }, __panic);
  if (__bgTasks) __bgTasks.push(p); // mesh test実行中: どのwaitにも入らないのでここで拾う
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
// F-9d: 配列の型安全な読み。範囲外は arr[i] のようにpanicせず none(null)を返す
const __get = (arr, i) => (Number.isInteger(i) && i >= 0 && i < arr.length ? arr[i] : null);
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
// float→int(round/floor/ceil。レビュー起点 — 逆方向のtoFloatしか無く、json.Value.nのような
// floatを配列添字/ループ境界のintへ戻す手段が無かった)。F-10と同じ理由で、丸めた結果が
// safe integer範囲を超えていたら無音の精度崩れを許さずpanicする
const __toIntSafe = (n, at) => {
  if (!Number.isSafeInteger(n)) {
    throw new __Panic(at + ": rounded result " + n + " exceeds the safe integer range");
  }
  return n;
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
// F-14: mesh/io — .messソースを持たない組み込みパッケージ(シグネチャはsrc/stdlib.tsに登録)。
// io$readFile はNode専用のfsに依存するので動的importで読む — ブラウザ実行(プレイグラウンドの
// Web Worker)ではモジュール解決自体が失敗し、素直にerror値へ落ちる(T | errorの通常経路)
const io$args = () => globalThis.process?.argv?.slice(2) ?? [];
const io$readFile = async (path) => {
  try {
    const { readFile } = await import("node:fs/promises");
    return await readFile(path, "utf8");
  } catch (e) {
    return new Error(e instanceof Error ? e.message : String(e));
  }
};
// F-14: mesh/json — json.Value(自己参照判別可能union)とJSの素の値とを変換する
const __jsonToValue = (v) => {
  if (v === null) return { kind: "null" };
  if (typeof v === "string") return { kind: "str", s: v };
  if (typeof v === "number") return { kind: "num", n: v };
  if (typeof v === "boolean") return { kind: "bool", b: v };
  if (Array.isArray(v)) return { kind: "arr", items: v.map(__jsonToValue) };
  return { kind: "obj", entries: new Map(Object.entries(v).map(([k, x]) => [k, __jsonToValue(x)])) };
};
const __valueToJson = (v) => {
  switch (v.kind) {
    case "null": return null;
    case "str": return v.s;
    case "num": return v.n;
    case "bool": return v.b;
    case "arr": return v.items.map(__valueToJson);
    case "obj": return Object.fromEntries([...v.entries].map(([k, x]) => [k, __valueToJson(x)]));
  }
};
const json$parse = (text) => {
  try {
    return __jsonToValue(JSON.parse(text));
  } catch (e) {
    return new Error(e instanceof Error ? e.message : String(e));
  }
};
const json$stringify = (v) => JSON.stringify(__valueToJson(v));
// H-2(2026-07-21): 検証つきデコード用の小さなヘルパー群。'json struct'の自動生成デコーダは
// これらを'?'で連結して組み立てる(src/json-decode.ts参照)。手書きデコーダからも直接使える
const json$field = (v, key) => {
  if (v.kind !== "obj") return new Error("expected a JSON object, got " + v.kind);
  if (!v.entries.has(key)) return new Error("missing field '" + key + "'");
  return v.entries.get(key);
};
const json$optField = (v, key) => {
  if (v.kind !== "obj") return null;
  const raw = v.entries.get(key);
  if (raw === undefined || raw.kind === "null") return null;
  return raw;
};
const json$asString = (v) => (v.kind === "str" ? v.s : new Error("expected a string, got " + v.kind));
const json$asInt = (v) => {
  if (v.kind !== "num") return new Error("expected a number, got " + v.kind);
  if (!Number.isSafeInteger(v.n)) return new Error("expected a whole number, got " + v.n);
  return v.n;
};
const json$asFloat = (v) => (v.kind === "num" ? v.n : new Error("expected a number, got " + v.kind));
const json$asBool = (v) => (v.kind === "bool" ? v.b : new Error("expected a boolean, got " + v.kind));
const json$asArray = (v) => (v.kind === "arr" ? v.items : new Error("expected an array, got " + v.kind));
// C-6続き: mesh/http — サーバー専用の組み込みパッケージ。node:httpはNode専用なので
// mesh/ioと同じく動的import(ブラウザ実行では素直にerrorへ落ちる)。
// addrは"host:port"/":port"/"port"のいずれも受け付ける(Go風。ホスト省略=全インターフェース)
const __httpParseAddr = (addr) => {
  const i = addr.lastIndexOf(":");
  const host = i > 0 ? addr.slice(0, i) : undefined;
  const port = parseInt(i === -1 ? addr : addr.slice(i + 1), 10);
  return { host, port };
};
// code review(PR #39): 上限が無いと1リクエストが無限にボディを送り込むだけでプロセス全体を
// メモリ枯渇で落とせてしまい、ハンドラのpanicだけを隔離しても「1リクエストがサーバー全体を
// 道連れにしない」という約束を守れていなかった。上限超過は__HttpBodyTooLargeでreject し、
// __httpDispatch側でハンドラのpanicとは区別して413にする(こちらも隔離の一種)
const MAX_HTTP_BODY_BYTES = 10 * 1024 * 1024; // v1は固定値(設定フックは無し)
class __HttpBodyTooLarge extends Error {}
const __httpReadBody = (req) =>
  new Promise((resolve, reject) => {
    const chunks = [];
    let total = 0;
    req.on("data", (c) => {
      total += c.length;
      if (total > MAX_HTTP_BODY_BYTES) {
        // req.destroy()だと、これから413を書き込むための接続そのものを即座に断ってしまい
        // クライアント側はレスポンスを受け取れずECONNRESETになる。pause()で読み取りだけ止め
        // (残りはTCP受信バッファ止まりでJSヒープは増えない)、接続を閉じるのは
        // __httpDispatch側でConnection: closeを付けたレスポンスを書き終えた後に任せる
        req.pause();
        reject(new __HttpBodyTooLarge("request body exceeds " + MAX_HTTP_BODY_BYTES + " bytes"));
        return;
      }
      chunks.push(c);
    });
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
const __httpBuildRequest = (req, body) => {
  const qIdx = req.url.indexOf("?");
  const path = qIdx === -1 ? req.url : req.url.slice(0, qIdx);
  const query = qIdx === -1 ? "" : req.url.slice(qIdx + 1);
  const headers = new Map();
  for (const [k, v] of Object.entries(req.headers)) headers.set(k, Array.isArray(v) ? v.join(", ") : (v ?? ""));
  return { method: req.method, path, query, headers, body };
};
// F-14実装メモの障害分離方針をここで実地適用: 1リクエストのハンドラがpanicしても
// プロセス全体を落とさず、そのリクエストだけ500にして他のリクエストは通常どおり続行する
// (requirements.md 5.5。Go net/httpが内部でrecoverするのと同じ構図 — Mesh言語自体に
// panic()/recover()は増やさない)。パニックの詳細はサーバー側ログにだけ出し、
// クライアントへは一般的な500だけ返す(内部情報を漏らさない)
const __httpDispatch = async (req, res, handler) => {
  try {
    const body = await __httpReadBody(req);
    const meshReq = __httpBuildRequest(req, body);
    const meshRes = await handler(meshReq);
    res.writeHead(meshRes.status, Object.fromEntries(meshRes.headers));
    res.end(meshRes.body);
  } catch (e) {
    if (e instanceof __HttpBodyTooLarge) {
      // ボディを最後まで読み切っていない(pause()した)ので、このソケットで次のリクエストを
      // 続けるとフレーミングが壊れる。Connection: closeでNodeに正しく閉じさせる
      // (レスポンスを書き終えるより先にソケットを破棄すると413が届かずECONNRESETになる)
      if (!res.headersSent) {
        res.writeHead(413, { "content-type": "text/plain", connection: "close" });
        res.end("request body too large");
      } else {
        res.end();
      }
      return;
    }
    console.error("panic (isolated to this request):", e instanceof Error ? e.message : String(e));
    if (!res.headersSent) {
      res.writeHead(500, { "content-type": "text/plain" });
      res.end("internal server error");
    } else {
      res.end();
    }
  }
};
const http$listen = async (addr, handler) => {
  try {
    const { createServer } = await import("node:http");
    const { host, port } = __httpParseAddr(addr);
    const server = createServer((req, res) => __httpDispatch(req, res, handler));
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, host, () => {
        server.removeListener("error", reject);
        server.on("error", (e) => console.error("http server error:", e.message));
        resolve();
      });
    });
    return null;
  } catch (e) {
    return new Error(e instanceof Error ? e.message : String(e));
  }
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
// F-15: mesh test — 各テスト関数を順に呼ぶ。戻り値がnone(合格)かerror(失敗)かを見る。
// panicも1件の失敗として隔離する(他のテストは続行する — 1つのバグでテストラン全体が
// 落ちないように。requirements.md 5.5「障害分離」方針をここで初めて実地適用した)。
// 結果は常に構造化JSONで標準出力へ1行書く(素の表示にするか--jsonのまま出すかはCLI側の仕事)。
//
// 隔離はtry/catchできる範囲(awaitされたテスト本体)だけでは不十分 — spawn/detachした
// タスクの失敗は別の非同期経路(__panic)を通り、try/catchに一切届かない。
// __panicSink/__bgTasksを立てて(1) __panicにプロセスを落とさせず記録だけさせ、
// (2) spawn/detachされた全タスクの決着をここで待ってから結果を確定させることで、
// 「バックグラウンドで起きたpanicがtrue判定に紛れて消える」ことを防ぐ。
// 個々のテストとの対応は取れない(detachは元々どのスコープにも属さないので不可能)ため、
// 単一の "(background task)" エントリとして可視化する — 隠すより不正確でも見せる方を選ぶ
const __runTests = async (tests) => {
  __panicSink = [];
  __bgTasks = [];
  const results = [];
  for (const t of tests) {
    try {
      const r = await t.fn();
      results.push(
        r === null || r === undefined
          ? { name: t.name, file: t.file, pass: true }
          : { name: t.name, file: t.file, pass: false, message: __fmt(r) },
      );
    } catch (e) {
      results.push({
        name: t.name,
        file: t.file,
        pass: false,
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }
  await Promise.all(__bgTasks); // spawn/detachした背景タスクの決着を待つ(いずれも拒否はしない)
  if (__panicSink.length > 0) {
    results.push({ name: "(background task)", file: "", pass: false, message: __panicSink.join("; ") });
  }
  const ok = results.every((r) => r.pass);
  console.log(JSON.stringify({ ok, tests: results }));
  if (!ok && globalThis.process) globalThis.process.exitCode = 1;
};
// ===== end runtime =====

`;
