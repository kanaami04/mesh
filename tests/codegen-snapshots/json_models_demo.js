async function jsonmodels$decodeUser(v) {
  try {
    const __f_name = __prop((await json$asString(__prop((await json$field(v, "name"))))));
    return ({ name: __f_name });
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function jsonmodels$encodeUser(x) {
  return ({ kind: "obj", entries: new Map([["name", ({ kind: "str", s: x.name })]]) });
}

async function main() {
  const v = (await __or((await json$parse("{\"name\": \"alice\"}")), async () => ({ kind: "null" })));
  const u = (await jsonmodels$decodeUser(v));
  if ((u instanceof Error)) {
    __print("failed");
    return;
  }
  __print(u.name);
}

main().catch(__panic);
