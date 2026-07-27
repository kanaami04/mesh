async function isEven(n) {
  return (__imod(n, 2, "examples/filter_map_reduce.mesh:7:11") === 0);
}

async function double(n) {
  return __iarith(n, "*", 2, "examples/filter_map_reduce.mesh:11:11");
}

async function sum(acc, n) {
  return __iarith(acc, "+", n, "examples/filter_map_reduce.mesh:15:13");
}

async function main() {
  const nums = [1, 2, 3, 4, 5, 6];
  const evens = (await __filter(nums, isEven));
  __print(evens);
  __print(nums);
  let threshold = 3;
  const big = (await __filter(nums, (async (n) => {
    return (n > threshold);
  })));
  __print(big);
  threshold = 1;
  __print((await __filter(nums, (async (n) => {
    return (n > threshold);
  }))));
  const labels = (await __map(nums, (async (n) => {
    return ("n" + __fmt(n));
  })));
  __print(labels);
  const total = (await __reduce(nums, (async (acc, n) => {
    return __iarith(acc, "+", n, "examples/filter_map_reduce.mesh:38:62");
  }), 0));
  __print(total);
  const joined = (await __reduce(nums, (async (acc, n) => {
    return (acc + __fmt(n));
  }), ""));
  __print(joined);
  const result = (await __reduce((await __map((await __filter(nums, isEven)), double)), sum, 0));
  __print(result);
  const inc = (async (x) => {
    return __iarith(x, "+", 1, "examples/filter_map_reduce.mesh:49:35");
  });
  __print(__iarith((await inc(5)), "*", 2, "examples/filter_map_reduce.mesh:50:15"));
  const nested = (await __map(nums, (async (n) => {
    const doubled = (await __map([n], (async (m) => {
      return __iarith(m, "*", 2, "examples/filter_map_reduce.mesh:55:49");
    })));
    return __idx(doubled, 0, "examples/filter_map_reduce.mesh:56:10");
  })));
  __print(nested);
  const parsed = (await __or(__toInt("42"), async () => (-1)));
  __print(parsed);
}

main().catch(__panic);
