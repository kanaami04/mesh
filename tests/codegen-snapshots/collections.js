async function main() {
  const ages = new Map([["alice", 30], ["bob", 25]]);
  ages.set("carol", 28);
  const age = (await __or(__mget(ages, "alice"), async () => 0));
  __print(("alice is " + __fmt(age)));
  const missing = (await __or(__mget(ages, "dave"), async () => (-1)));
  __print(("dave is " + __fmt(missing)));
  ages.delete("bob");
  __print(("" + __fmt(ages.size) + " people"));
  for (const [k, v] of ages) {
    __print(("" + __fmt(k) + ": " + __fmt(v)));
  }
  const nums = [30, 10, 20];
  let total = 0;
  for (const [, v] of nums.entries()) {
    total = __iarith(total, "+", v, "examples/collections.mesh:21:17");
  }
  __print(("total: " + __fmt(total)));
  nums.push(40);
  __print(("first: " + __fmt(__get(nums, 0))));
  __print(("has 10: " + __fmt(nums.includes(10))));
  __print(("index of 20: " + __fmt(__indexOf(nums, 20))));
  const sorted = __sorted(nums);
  for (const [i, v] of sorted.entries()) {
    __print(("" + __fmt(i) + " -> " + __fmt(v)));
  }
  for (let i = 0, __n = 3; i < __n; i++) {
    __print(("tick " + __fmt(i)));
  }
}

main().catch(__panic);
