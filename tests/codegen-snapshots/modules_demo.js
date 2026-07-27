async function mathutil$add(a, b) {
  return __iarith(a, "+", b, "examples/mathutil/ops.mesh:5:11");
}

async function mathutil$double(n) {
  return __iarith(n, "*", 2, "examples/mathutil/ops.mesh:10:11");
}

async function mathutil$quadruple(n) {
  return (await mathutil$double((await mathutil$double(n))));
}

async function __m_mathutil$Point_magnitudeSq(p) {
  return __iarith(__iarith(p.x, "*", p.x, "examples/mathutil/point.mesh:10:13"), "+", __iarith(p.y, "*", p.y, "examples/mathutil/point.mesh:10:25"), "examples/mathutil/point.mesh:10:19");
}

async function mathutil$origin() {
  return ({ x: 0, y: 0 });
}

async function main() {
  __print((await mathutil$add(1, 2)));
  __print((await mathutil$quadruple(3)));
  const p = ({ x: 3, y: 4 });
  __print((await __m_mathutil$Point_magnitudeSq(p)));
  const q = (await mathutil$origin());
  __print(q.x, q.y);
}

main().catch(__panic);
