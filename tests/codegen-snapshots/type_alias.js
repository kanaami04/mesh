async function describe(u) {
  return (((("user #" + __fmt(u.id)) + " (") + __fmt(u.tags.length)) + " tags)");
}

async function apply(n, f) {
  return (await f(n));
}

async function double(n) {
  return __iarith(n, "*", 2, "examples/type_alias.mesh:26:11");
}

async function main() {
  const u = ({ id: 7, tags: ["admin", "beta"] });
  __print((await describe(u)));
  __print((await apply(u.id, double)));
  __print((await apply(3, (async (n) => {
    return __iarith(n, "+", 1, "examples/type_alias.mesh:34:43");
  }))));
  const scores = new Map([["alice", 30], ["bob", 25]]);
  __print(scores.size);
  __print((await sum([1, 2, 3])));
  const a = ({ id: 9, tags: [] });
  __print((await describe(a)));
}

async function sum(ns) {
  let total = 0;
  for (const [, n] of ns.entries()) {
    total = __iarith(total, "+", n, "examples/type_alias.mesh:50:17");
  }
  return total;
}

main().catch(__panic);
