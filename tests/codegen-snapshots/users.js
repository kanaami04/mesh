async function find(id) {
  if ((id < 0)) {
    return __error(("invalid id: " + __fmt(id)));
  }
  if ((id === 1)) {
    return ({ name: "alice", age: 30 });
  }
  return null;
}

async function label(id) {
  const u = (await find(id));
  return (await (async (__m) => (typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m)) ? ("hello " + __fmt(u.name) + " (" + __fmt(u.age) + ")") : (__m === null) ? "404 not found" : ("500: " + __fmt(u)))(u));
}

async function main() {
  __print((await label(1)));
  __print((await label(2)));
  __print((await label((-1))));
}

main().catch(__panic);
