async function __m_Todo_complete(t) {
  return ({ title: t.title, done: true });
}

async function __m_Todo_render(t) {
  if (t.done) {
    return ("[x] " + t.title);
  }
  return ("[ ] " + t.title);
}

async function __m_User_describe(u) {
  return ("" + __fmt(u.name) + " (" + __fmt(u.age) + ")");
}

async function main() {
  let todo = ({ title: "write the milestone 2 report", done: false });
  __print((await __m_Todo_render(todo)));
  todo = (await __m_Todo_complete(todo));
  __print((await __m_Todo_render(todo)));
  __print((await __m_Todo_render((await __m_Todo_complete(({ title: "chained", done: false }))))));
  const u = ({ name: "alice", age: 30 });
  __print((await __m_User_describe(u)));
  u.age = __iarith(u.age, "+", 1, "examples/struct_methods.mesh:39:16");
  __print(u.age);
  u.age = __iarith(u.age, "+", 1, "examples/struct_methods.mesh:41:2");
  __print(u.age);
}

main().catch(__panic);
