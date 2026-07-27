async function __m_Box_doubled(b) {
  return ({ v: __iarith(b.v, "*", 2, "examples/generics.mesh:10:20") });
}

async function identity(x) {
  return x;
}

async function firstOf(xs) {
  if ((xs.length === 0)) {
    return null;
  }
  return __idx(xs, 0, "examples/generics.mesh:21:9");
}

async function pairUp(a, b) {
  return ("" + __fmt(a) + "/" + __fmt(b));
}

async function twice(cb, v) {
  return (await cb((await cb(v))));
}

async function main() {
  __print((await identity(1)));
  __print((await identity("hello")));
  __print((await __m_Box_doubled((await identity(({ v: 3 }))))).v);
  const found = (await firstOf([10, 20, 30]));
  if (Number.isInteger(found)) {
    __print(found);
  }
  __print((await pairUp(7, "seven")));
  __print((await twice((async (s) => {
    return (s + "!");
  }), "hi")));
}

main().catch(__panic);
