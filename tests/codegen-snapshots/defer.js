async function __m_Box_announce(b) {
  __print(("closing " + __fmt(b.label)));
}

async function multiDefer() {
  const __defers = [];
  try {
    {
      const __d0 = "first";
      __defers.push(async () => { __print(__d0); });
    }
    {
      const __d1 = "second";
      __defers.push(async () => { __print(__d1); });
    }
    {
      const __d2 = "third";
      __defers.push(async () => { __print(__d2); });
    }
    __print("body");
  } finally {
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

async function argCapture() {
  const __defers = [];
  try {
    let n = 1;
    {
      const __d3 = ("deferred: " + __fmt(n));
      __defers.push(async () => { __print(__d3); });
    }
    n = 99;
    __print(("live: " + __fmt(n)));
  } finally {
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

async function methodDefer() {
  const __defers = [];
  try {
    let b = ({ label: "first" });
    {
      const __d4 = b;
      __defers.push(async () => { (await __m_Box_announce(__d4)); });
    }
    b = ({ label: "second" });
    __print("done");
  } finally {
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

async function builtinDefer() {
  const __defers = [];
  try {
    const ch = new __Channel(1);
    {
      const __d5 = ch;
      __defers.push(async () => { __d5.close(); });
    }
    (await ch.send(5));
    const v = (await __recv(ch));
    if ((v === __CLOSED)) {
      return (-1);
    }
    return v;
  } finally {
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

async function earlyReturn(fail) {
  const __defers = [];
  try {
    {
      const __d6 = "cleanup";
      __defers.push(async () => { __print(__d6); });
    }
    if (fail) {
      __print("early exit");
      return;
    }
    __print("normal exit");
  } finally {
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

async function loopDefer() {
  const __defers = [];
  try {
    for (let i = 0, __n = 3; i < __n; i++) {
      {
        const __d7 = ("deferred " + __fmt(i));
        __defers.push(async () => { __print(__d7); });
      }
    }
    __print("loop done");
  } finally {
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

async function slow() {
  (await __sleep(5));
  __print("slow done");
  return 1;
}

async function spawnDefer() {
  const __defers = [];
  __waitStack.push([]);
  try {
    {
      const __d8 = "final cleanup";
      __defers.push(async () => { __print(__d8); });
    }
    const ch = __spawn(slow, []);
    const v = (await __recv(ch));
    if ((v === __CLOSED)) {
      return;
    }
    __print(("got " + __fmt(v)));
  } finally {
    await Promise.all(__waitStack.pop());
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

async function main() {
  (await multiDefer());
  (await argCapture());
  (await methodDefer());
  __print((await builtinDefer()));
  (await earlyReturn(true));
  (await loopDefer());
  (await spawnDefer());
  const f = (async () => {
    const __defers = [];
    try {
      {
        const __d9 = "closure cleanup";
        __defers.push(async () => { __print(__d9); });
      }
      __print("closure body");
    } finally {
      for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
    }
  });
  (await f());
  __print("after closure");
}

main().catch(__panic);
