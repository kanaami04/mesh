async function work() {
  const __defers = [];
  try {
    {
      const __d0 = "cleanup ran";
      __defers.push(async () => { __print(__d0); });
    }
    const nums = [1, 2, 3];
    __print(__idx(nums, 10, "examples/defer_panic.mesh:9:8"));
  } finally {
    for (let __i = __defers.length - 1; __i >= 0; __i--) await __defers[__i]();
  }
}

async function main() {
  (await work());
}

main().catch(__panic);
