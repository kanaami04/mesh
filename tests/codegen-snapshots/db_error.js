async function find(id) {
  if ((id === 1)) {
    return 100;
  }
  if ((id === 2)) {
    return __errTag({ kind: "timeout", ms: 500 });
  }
  return __errTag({ kind: "notFound", table: "users" });
}

async function useIt(id) {
  try {
    const v = __prop((await find(id)));
    return __iarith(v, "+", 1, "examples/db_error.mesh:19:11");
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function main() {
  __print((await useIt(1)));
  const a = (await __or((await find(1)), async (e) => (await (async (__m) => (typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m)) && (__m.kind === "notFound") ? (-1) : (-2))(e))));
  __print(a);
  const b = (await __or((await find(2)), async (e) => (await (async (__m) => (typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m)) && (__m.kind === "notFound") ? (-1) : (-2))(e))));
  __print(b);
  const c = (await __or((await find(3)), async (e) => (await (async (__m) => (typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m)) && (__m.kind === "notFound") ? (-1) : (-2))(e))));
  __print(c);
}

main().catch(__panic);
