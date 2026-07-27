async function divide(a, b) {
  if ((b === 0)) {
    return __error("division by zero");
  }
  return __idiv(a, b, "examples/errors.mesh:9:11");
}

async function main() {
  const result = (await divide(10, 3));
  if ((result instanceof Error)) {
    __print("error:", result);
    return;
  }
  __print("10 / 3 =", result);
  const fallback = (await __or((await divide(1, 0)), async () => 0));
  __print(("fallback: " + __fmt(fallback)));
  const logged = (await __or((await divide(1, 0)), async (e) => ("" + __fmt(e)).length));
  __print(("logged: " + __fmt(logged)));
  const bad = (await divide(1, 0));
  if ((bad instanceof Error)) {
    __print(("caught: " + __fmt(bad)));
  }
  const r = (await divide(9, 3));
  __print((await (async (__m) => (__m instanceof Error) ? ("failed: " + __fmt(r)) : ("match says: " + __fmt(r)))(r)));
}

main().catch(__panic);
