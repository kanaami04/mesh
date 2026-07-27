async function leaf(v) {
  return ({ kind: "leaf", value: v });
}

async function node(l, r) {
  return ({ kind: "node", left: l, right: r });
}

async function sumTree(t) {
  return (await (async (__m) => (typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m)) && (__m.kind === "leaf") ? t.value : __iarith((await sumTree(t.left)), "+", (await sumTree(t.right)), "examples/tree.mesh:18:39"))(t));
}

async function depth(t) {
  return (await (async (__m) => (typeof __m === "object" && __m !== null && !(__m instanceof Error) && !Array.isArray(__m)) && (__m.kind === "leaf") ? 1 : __iarith(1, "+", (await max((await depth(t.left)), (await depth(t.right)))), "examples/tree.mesh:25:25"))(t));
}

async function max(a, b) {
  if ((a > b)) {
    return a;
  }
  return b;
}

async function main() {
  const tree = (await node((await node((await leaf(1)), (await leaf(2)))), (await leaf(3))));
  __print((await sumTree(tree)));
  __print((await depth(tree)));
}

main().catch(__panic);
