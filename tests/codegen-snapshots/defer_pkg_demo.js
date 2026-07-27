async function loggerpkg$flush(msg) {
  __print(("flush: " + __fmt(msg)));
}

async function main() {
  const __defers = [];
  try {
    {
      const __d0 = "done";
      __defers.push(async () => { (await loggerpkg$flush(__d0)); });
    }
    __print("working");
  } finally {
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

main().catch(__panic);
