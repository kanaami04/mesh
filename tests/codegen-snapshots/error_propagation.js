async function divide(a, b) {
  if ((b === 0)) {
    return __error("division by zero");
  }
  return __idiv(a, b, "examples/error_propagation.mesh:12:11");
}

async function lookup(id) {
  if ((id === 1)) {
    return 42;
  }
  return null;
}

async function find(id) {
  if ((id === 1)) {
    return 100;
  }
  return __errTag({ message: ("no user with id " + __fmt(id)) });
}

async function compute(a, b) {
  try {
    const x = __prop((await divide(a, b)));
    return __iarith(x, "+", 1, "examples/error_propagation.mesh:32:11");
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function computeWithContext(a, b) {
  try {
    const x = (await __propCtx((await divide(a, b)), async () => "compute failed"));
    return __iarith(x, "+", 1, "examples/error_propagation.mesh:38:11");
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function useIt(id) {
  try {
    const v = __prop((await find(id)));
    return ("found user " + __fmt(v));
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function main() {
  const a = (await __or((await lookup(1)), async () => 0));
  __print(a);
  const b = (await __or((await lookup(2)), async () => 0));
  __print(b);
  const c = (await __or((await compute(10, 2)), async () => (-1)));
  __print(c);
  const d = (await __or((await compute(10, 0)), async () => (-1)));
  __print(d);
  const e1 = (await __or((await computeWithContext(10, 2)), async (e) => (-1)));
  __print(e1);
  const e2 = (await __or((await computeWithContext(10, 0)), async (e) => (-1)));
  __print(e2);
  const f1 = (await __or((await useIt(1)), async (e) => e.message));
  __print(f1);
  const f2 = (await __or((await useIt(2)), async (e) => e.message));
  __print(f2);
}

main().catch(__panic);
