async function __m_Worker_greet(w, msg) {
  (await __sleep(5));
  return ("worker " + __fmt(w.id) + " says: " + __fmt(msg));
}

async function compute(id, ch) {
  (await __sleep(__iarith(10, "*", id, "examples/concurrency.mesh:15:11")));
  (await ch.send(__iarith(id, "*", 10, "examples/concurrency.mesh:16:11")));
}

async function announce(msg, ch, ms) {
  (await __sleep(ms));
  (await ch.send(msg));
}

async function announceDetached(msg) {
  (await __sleep(5));
  __print(("detached: " + __fmt(msg)));
}

async function main() {
  __waitStack.push([]);
  try {
    const results = new __Channel();
    __waitStack.push([]);
    try {
      for (let i = 1; (i <= 3); i++) {
        __spawn(compute, [i, results]);
      }
      for (let i = 0; (i < 3); i++) {
        const v = (await __recv(results));
        __print(("got: " + __fmt(v)));
      }
    } finally {
      await Promise.all(__waitStack.pop());
    }
    const bounded = new __Channel(2);
    __spawn(announce, ["first", bounded, 10]);
    __spawn(announce, ["second", bounded, 20]);
    __print((await __recv(bounded)));
    __print((await __recv(bounded)));
    const w = ({ id: 7 });
    const greeting = __spawn(__m_Worker_greet, [w, "hello"]);
    __print((await __recv(greeting)));
    __detach(announceDetached, ["bye"]);
    const a = new __Channel();
    const b = new __Channel();
    __spawn(announce, ["from a", a, 10]);
    const msg = (await __select([a, b], [(async (v) => ("a says: " + __fmt(v))), (async (v) => ("b says: " + __fmt(v)))], null));
    __print(msg);
    const empty = new __Channel();
    const other = new __Channel();
    const r = (await __select([empty, other], [(async (v) => ("unexpected: " + __fmt(v))), (async (v) => ("unexpected: " + __fmt(v)))], (async () => "nothing ready")));
    __print(r);
    (await __sleep(50));
  } finally {
    await Promise.all(__waitStack.pop());
  }
}

main().catch(__panic);
