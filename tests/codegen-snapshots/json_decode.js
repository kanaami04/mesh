async function rawDestructure() {
  const r = (await json$parse("{\"a\": 1, \"b\": [true, null, \"x\"]}"));
  if ((r instanceof Error)) {
    __print("parse failed");
    return;
  }
  const v = r;
  if ((typeof v === "object" && v !== null && !(v instanceof Error) && !Array.isArray(v)) && (v.kind === "obj")) {
    __print(v.entries.size);
  }
  __print((await json$stringify(v)));
}

async function main() {
  (await rawDestructure());
  const v = (await __or((await json$parse("{\"name\": \"alice\", \"age\": 30}")), async () => ({ kind: "null" })));
  const u = (await decodeUser(v));
  if ((u instanceof Error)) {
    __print("failed");
    return;
  }
  __print(u.name, u.age);
  const missing = (await __or((await json$parse("{\"name\": \"bob\"}")), async () => ({ kind: "null" })));
  const u1 = (await decodeUser(missing));
  if ((u1 instanceof Error)) {
    __print(u1);
  }
  const wrongType = (await __or((await json$parse("{\"name\": \"bob\", \"age\": \"nope\"}")), async () => ({ kind: "null" })));
  const u2 = (await decodeUser(wrongType));
  if ((u2 instanceof Error)) {
    __print(u2);
  }
  const notObj = (await __or((await json$parse("[1,2,3]")), async () => ({ kind: "null" })));
  const u3 = (await decodeUser(notObj));
  if ((u3 instanceof Error)) {
    __print(u3);
  }
  const text = "{\"name\": \"alice\", \"address\": {\"city\": \"Tokyo\", \"zip\": \"100-0001\"}, \"tags\": [\"a\", \"b\"], \"nickname\": \"al\"}";
  const pv = (await __or((await json$parse(text)), async () => ({ kind: "null" })));
  const p = (await decodePerson(pv));
  if ((p instanceof Error)) {
    __print(("failed: " + __fmt(p)));
    return;
  }
  __print(p.name, p.address.city, p.address.zip, p.tags, p.nickname);
  const textNoNick = "{\"name\": \"bob\", \"address\": {\"city\": \"Osaka\", \"zip\": \"500-0001\"}, \"tags\": []}";
  const pv2 = (await __or((await json$parse(textNoNick)), async () => ({ kind: "null" })));
  const p2 = (await decodePerson(pv2));
  if ((p2 instanceof Error)) {
    __print(("failed2: " + __fmt(p2)));
    return;
  }
  __print(p2.name, p2.nickname, p2.tags.length);
  const cartText = "{\"items\": [{\"sku\": \"a1\", \"qty\": 2}, {\"sku\": \"b2\", \"qty\": 5}], \"notes\": [\"fragile\"]}";
  const cv = (await __or((await json$parse(cartText)), async () => ({ kind: "null" })));
  const c = (await decodeCart(cv));
  if ((c instanceof Error)) {
    __print(("failed: " + __fmt(c)));
    return;
  }
  __print(c.items.length, __idx(c.items, 0, "examples/json_decode.mesh:87:22").sku, __idx(c.items, 1, "examples/json_decode.mesh:87:38").qty, c.notes);
  const cartTextNoNotes = "{\"items\": []}";
  const cv2 = (await __or((await json$parse(cartTextNoNotes)), async () => ({ kind: "null" })));
  const c2 = (await decodeCart(cv2));
  if ((c2 instanceof Error)) {
    __print(("failed2: " + __fmt(c2)));
    return;
  }
  __print(c2.items.length, c2.notes);
  const badAge = (await __or((await json$parse("{\"name\": \"a\", \"age\": 30.5}")), async () => ({ kind: "null" })));
  const u4 = (await decodeUser(badAge));
  if ((u4 instanceof Error)) {
    __print(u4);
  }
}

async function decodeUser(v) {
  try {
    const __f_name = __prop((await json$asString(__prop((await json$field(v, "name"))))));
    const __f_age = __prop((await json$asInt(__prop((await json$field(v, "age"))))));
    return ({ name: __f_name, age: __f_age });
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function decodeAddress(v) {
  try {
    const __f_city = __prop((await json$asString(__prop((await json$field(v, "city"))))));
    const __f_zip = __prop((await json$asString(__prop((await json$field(v, "zip"))))));
    return ({ city: __f_city, zip: __f_zip });
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function decodePerson(v) {
  try {
    const __f_name = __prop((await json$asString(__prop((await json$field(v, "name"))))));
    const __f_address = __prop((await decodeAddress(__prop((await json$field(v, "address"))))));
    const __raw_arr_tags = __prop((await json$asArray(__prop((await json$field(v, "tags"))))));
    let __f_tags = [];
    for (const [, __item_tags] of __raw_arr_tags.entries()) {
      const __decoded_tags = __prop((await json$asString(__item_tags)));
      __f_tags.push(__decoded_tags);
    }
    const __raw_nickname = (await json$optField(v, "nickname"));
    let __f_nickname = null;
    if ((!(__raw_nickname === null))) {
      __f_nickname = __prop((await json$asString(__raw_nickname)));
    }
    return ({ name: __f_name, address: __f_address, tags: __f_tags, nickname: __f_nickname });
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function decodeItem(v) {
  try {
    const __f_sku = __prop((await json$asString(__prop((await json$field(v, "sku"))))));
    const __f_qty = __prop((await json$asInt(__prop((await json$field(v, "qty"))))));
    return ({ sku: __f_sku, qty: __f_qty });
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function decodeCart(v) {
  try {
    const __raw_arr_items = __prop((await json$asArray(__prop((await json$field(v, "items"))))));
    let __f_items = [];
    for (const [, __item_items] of __raw_arr_items.entries()) {
      const __decoded_items = __prop((await decodeItem(__item_items)));
      __f_items.push(__decoded_items);
    }
    const __raw_notes = (await json$optField(v, "notes"));
    let __f_notes = null;
    if ((!(__raw_notes === null))) {
      const __raw_arr_notes = __prop((await json$asArray(__raw_notes)));
      let __acc_notes = [];
      for (const [, __item_notes] of __raw_arr_notes.entries()) {
        const __decoded_notes = __prop((await json$asString(__item_notes)));
        __acc_notes.push(__decoded_notes);
      }
      __f_notes = __acc_notes;
    }
    return ({ items: __f_items, notes: __f_notes });
  } catch (e) {
    if (e instanceof __Propagate) return e.value;
    throw e;
  }
}

async function encodeUser(x) {
  return ({ kind: "obj", entries: new Map([["name", ({ kind: "str", s: x.name })], ["age", ({ kind: "num", n: x.age })]]) });
}

async function encodeAddress(x) {
  return ({ kind: "obj", entries: new Map([["city", ({ kind: "str", s: x.city })], ["zip", ({ kind: "str", s: x.zip })]]) });
}

async function encodePerson(x) {
  let __earr_tags = [];
  for (const [, __eitem_tags] of x.tags.entries()) {
    __earr_tags.push(({ kind: "str", s: __eitem_tags }));
  }
  const __efv_nickname = x.nickname;
  let __ef_nickname = ({ kind: "null" });
  if ((!(__efv_nickname === null))) {
    __ef_nickname = ({ kind: "str", s: __efv_nickname });
  }
  return ({ kind: "obj", entries: new Map([["name", ({ kind: "str", s: x.name })], ["address", (await encodeAddress(x.address))], ["tags", ({ kind: "arr", items: __earr_tags })], ["nickname", __ef_nickname]]) });
}

async function encodeItem(x) {
  return ({ kind: "obj", entries: new Map([["sku", ({ kind: "str", s: x.sku })], ["qty", ({ kind: "num", n: x.qty })]]) });
}

async function encodeCart(x) {
  let __earr_items = [];
  for (const [, __eitem_items] of x.items.entries()) {
    __earr_items.push((await encodeItem(__eitem_items)));
  }
  const __efv_notes = x.notes;
  let __ef_notes = ({ kind: "null" });
  if ((!(__efv_notes === null))) {
    let __earr_notes = [];
    for (const [, __eitem_notes] of __efv_notes.entries()) {
      __earr_notes.push(({ kind: "str", s: __eitem_notes }));
    }
    __ef_notes = ({ kind: "arr", items: __earr_notes });
  }
  return ({ kind: "obj", entries: new Map([["items", ({ kind: "arr", items: __earr_items })], ["notes", __ef_notes]]) });
}

main().catch(__panic);
