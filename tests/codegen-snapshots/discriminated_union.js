async function findUser(id) {
  if ((id === "1")) {
    return ({ name: "alice" });
  }
  return null;
}

async function getUser(id) {
  if ((id === "banned")) {
    return ({ kind: "unauthorized" });
  }
  const u = (await findUser(id));
  if ((u === null)) {
    return ({ kind: "notFound" });
  }
  return ({ kind: "ok", user: u });
}

async function describe(res) {
  return (await (async (__m) => (typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m)) && (__m.kind === "ok") ? ("found: " + __fmt(res.user.name)) : (typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m)) && (__m.kind === "notFound") ? "not found" : "unauthorized")(res));
}

async function main() {
  __print((await describe((await getUser("1")))));
  __print((await describe((await getUser("2")))));
  __print((await describe((await getUser("banned")))));
}

main().catch(__panic);
