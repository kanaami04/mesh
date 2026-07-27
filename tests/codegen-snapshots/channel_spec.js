async function produce(ch) {
  for (let i = 1; (i <= 3); i++) {
    (await ch.send(i));
  }
  ch.close();
}

async function slowSend(ch, msg, ms) {
  (await __sleep(ms));
  (await ch.send(msg));
}

async function main() {
  __waitStack.push([]);
  try {
    const ch = new __Channel();
    __spawn(produce, [ch]);
    let total = 0;
    for (; ; ) {
      const v = (await __recv(ch));
      if ((v === __CLOSED)) {
        break;
      }
      total = __iarith(total, "+", v, "examples/channel_spec.mesh:26:17");
    }
    __print(("total: " + __fmt(total)));
    const a = new __Channel();
    const b = new __Channel();
    __spawn(slowSend, [a, "from a", 15]);
    __spawn(slowSend, [b, "from b", 60]);
    const msg = (await __select([a, b], [(async (v) => ("got: " + __fmt(v))), (async (v) => ("got: " + __fmt(v)))], null));
    __print(msg);
    const empty = new __Channel();
    const r = (await __select([empty], [(async (v) => ("unexpected: " + __fmt(v)))], (async () => "nothing ready")));
    __print(r);
  } finally {
    await Promise.all(__waitStack.pop());
  }
}

main().catch(__panic);
