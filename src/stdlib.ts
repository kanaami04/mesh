// F-14: 組み込みパッケージ(mesh/io, mesh/json)のシグネチャ定義。
// これらは .mesh ソースを持たない — ディスク上のディレクトリではなく、この定義から
// 直接 checker の registry(PackageSymbols)へ登録する(checkModules参照)。
// 実行時の実体(JS実装)は runtime.ts の PRELUDE 側にある。

import type { PackageSymbols } from "./checker";
import { BOOL, ERROR, FLOAT, STRING, type StructField, type Type, unionOf } from "./types";

function fn(params: Type[], ret: Type): Type {
  return { kind: "fn", params, ret };
}

function anonStruct(fields: StructField[]): Type & { kind: "struct" } {
  return { kind: "struct", name: "(anonymous)", fields };
}

function literal(value: string): Type {
  return { kind: "literal", value };
}

// json.Value = { kind: "str", s: string } | { kind: "num", n: float } | { kind: "bool", b: bool }
//            | { kind: "null" } | { kind: "arr", items: Value[] } | { kind: "obj", entries: map<string, Value> }
// arr/obj は Value 自身を(配列・map越しに)参照する自己参照型なので、resolveAlias の
// knot-tying と同じ要領で「先に器を作ってから後でfieldsを埋める」
function buildJsonValueType(): Type {
  const strMember = anonStruct([
    { name: "kind", type: literal("str") },
    { name: "s", type: STRING },
  ]);
  const numMember = anonStruct([
    { name: "kind", type: literal("num") },
    { name: "n", type: FLOAT },
  ]);
  const boolMember = anonStruct([
    { name: "kind", type: literal("bool") },
    { name: "b", type: BOOL },
  ]);
  const nullMember = anonStruct([{ name: "kind", type: literal("null") }]);
  const arrMember = anonStruct([]); // fields はunion確定後に埋める(自己参照のため)
  const objMember = anonStruct([]);

  const union: Type & { kind: "union" } = {
    kind: "union",
    members: [strMember, numMember, boolMember, nullMember, arrMember, objMember],
    discriminantTag: "kind", // F-7: 判別可能unionのタグ必須化と同じ形。ここは手組みなので直接確定させる
  };

  arrMember.fields = [
    { name: "kind", type: literal("arr") },
    { name: "items", type: { kind: "array", elem: union } },
  ];
  objMember.fields = [
    { name: "kind", type: literal("obj") },
    { name: "entries", type: { kind: "map", key: STRING, value: union } },
  ];
  return union;
}

const JSON_VALUE = buildJsonValueType();

const exportedFn = (type: Type) => ({ type, exported: true });

// パス("mesh/io"等)で引く — checkerのimport検証はimp.path(ディスク上のパス)を見るため。
// registryへ登録するときはエイリアス("io")に付け替える(checkModules参照)
export const BUILTIN_PACKAGES: ReadonlyMap<string, PackageSymbols> = new Map([
  [
    "mesh/io",
    {
      types: new Map(),
      consts: new Map(),
      fns: new Map([
        ["args", exportedFn(fn([], { kind: "array", elem: STRING }))],
        ["readFile", exportedFn(fn([STRING], unionOf([STRING, ERROR])))],
      ]),
    },
  ],
  [
    "mesh/json",
    {
      types: new Map([["Value", { type: JSON_VALUE, exported: true }]]),
      consts: new Map(),
      fns: new Map([
        ["parse", exportedFn(fn([STRING], unionOf([JSON_VALUE, ERROR])))],
        ["stringify", exportedFn(fn([JSON_VALUE], STRING))],
      ]),
    },
  ],
]);
