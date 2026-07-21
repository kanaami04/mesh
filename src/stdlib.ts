// F-14: 組み込みパッケージ(mesh/io, mesh/json)のシグネチャ定義。
// これらは .mesh ソースを持たない — ディスク上のディレクトリではなく、この定義から
// 直接 checker の registry(PackageSymbols)へ登録する(checkModules参照)。
// 実行時の実体(JS実装)は runtime.ts の PRELUDE 側にある。

import type { PackageSymbols } from "./checker";
import { BOOL, ERROR, FLOAT, INT, NONE, STRING, type StructField, type Type, unionOf } from "./types";

function fn(params: Type[], ret: Type): Type {
  return { kind: "fn", params, ret };
}

function anonStruct(fields: StructField[]): Type & { kind: "struct" } {
  return { kind: "struct", name: "(anonymous)", fields };
}

function namedStruct(name: string, fields: StructField[]): Type & { kind: "struct" } {
  return { kind: "struct", name, fields };
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

// C-6続き: mesh/http — サーバー専用(クライアント機能は無い)。v1は「生ハンドラ1本+
// http.listen」のみ(Go net/httpの最小形。ルーティングは無い — req.path/req.methodを
// 見てユーザーが自分でif分岐する)。メソッド別登録(http.get/post等)は将来のv2で
// mesh/http自身に追加する構想(design-agenda.md C-6参照。フレームワークに切り出さない
// 理由: MeshにはまだサードパーティパッケージエコシステムがなくQ2が未決着のため)
const STRING_MAP: Type = { kind: "map", key: STRING, value: STRING };

const HTTP_REQUEST = namedStruct("Request", [
  { name: "method", type: STRING },
  { name: "path", type: STRING }, // クエリを含まないURLパスのみ
  { name: "query", type: STRING }, // 生のクエリ文字列(無ければ空文字列。未パース)
  { name: "headers", type: STRING_MAP },
  { name: "body", type: STRING },
]);

const HTTP_RESPONSE = namedStruct("Response", [
  { name: "status", type: INT },
  { name: "body", type: STRING },
  { name: "headers", type: STRING_MAP },
]);

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
        // H-2(2026-07-21): 検証つきデコード用の小さなヘルパー群。`json struct`の自動生成
        // デコーダはこれらを`?`で連結して組み立てる(下のjson-decode.ts参照)。素の`json.Value`を
        // own hand-written デコーダから直接使うことも想定している(自動生成が対応しない
        // 形〈union・mapフィールド等〉のフォールバック手段)
        ["field", exportedFn(fn([JSON_VALUE, STRING], unionOf([JSON_VALUE, ERROR])))],
        ["optField", exportedFn(fn([JSON_VALUE, STRING], unionOf([JSON_VALUE, NONE])))],
        ["asString", exportedFn(fn([JSON_VALUE], unionOf([STRING, ERROR])))],
        ["asInt", exportedFn(fn([JSON_VALUE], unionOf([INT, ERROR])))],
        ["asFloat", exportedFn(fn([JSON_VALUE], unionOf([FLOAT, ERROR])))],
        ["asBool", exportedFn(fn([JSON_VALUE], unionOf([BOOL, ERROR])))],
        ["asArray", exportedFn(fn([JSON_VALUE], unionOf([{ kind: "array", elem: JSON_VALUE }, ERROR])))],
      ]),
    },
  ],
  [
    "mesh/http",
    {
      types: new Map([
        ["Request", { type: HTTP_REQUEST, exported: true }],
        ["Response", { type: HTTP_RESPONSE, exported: true }],
      ]),
      consts: new Map(),
      fns: new Map([
        // 起動できたら(bindが成功したら)ほぼ即座に none を返す — 「サーバーが止まるまで
        // 待つ」わけではない。プロセスがそのまま生き続けるのはNodeのイベントループが
        // listen中のソケットを保持するため(mesh/http自体に「待つ」機構は無い)
        ["listen", exportedFn(fn([STRING, fn([HTTP_REQUEST], HTTP_RESPONSE)], unionOf([NONE, ERROR])))],
      ]),
    },
  ],
]);
