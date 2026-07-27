async function main() {
  const ages = new Map([["alice", 30], ["bob", 25]]);
  ages.set("carol", 28);
  const age = (await __or(__mget(ages, "alice"), async () => 0));
  __print(("alice is " + __fmt(age)));
  const missing = __mget(ages, "dave");
  if ((missing === null)) {
    __print("dave is unknown");
  }
  ages.delete("bob");
  __print(("" + __fmt(ages.size) + " people"));
  for (const [k, v] of ages) {
    __print(("" + __fmt(k) + ": " + __fmt(v)));
  }
  const nums = [10, 20, 30];
  let total = 0;
  for (const [, v] of nums.entries()) {
    total = __iarith(total, "+", v, "examples/maps.mesh:26:17");
  }
  __print(("total: " + __fmt(total)));
  for (let i = 0, __n = 3; i < __n; i++) {
    __print(("tick " + __fmt(i)));
  }
}

main().catch(__panic);
