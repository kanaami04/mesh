async function label(s) {
  return (await (async (__m) => (__m === "active") ? "ようこそ" : "アクセス不可")(s));
}

async function main() {
  __print((await label("active")));
  __print((await label("pending")));
}

main().catch(__panic);
