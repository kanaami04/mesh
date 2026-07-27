async function worker(id, ch) {
  (await __sleep(__iarith(10, "*", id, "examples/channels.mesh:5:11")));
  (await ch.send(("worker " + __fmt(id) + " done")));
}

async function main() {
  __waitStack.push([]);
  try {
    const ch = new __Channel();
    for (let i = 1; (i <= 3); i++) {
      __spawn(worker, [i, ch]);
    }
    for (let i = 0; (i < 3); i++) {
      const msg = (await __recv(ch));
      __print(msg);
    }
  } finally {
    await Promise.all(__waitStack.pop());
  }
}

main().catch(__panic);
