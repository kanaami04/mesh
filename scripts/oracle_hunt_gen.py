"""オラクル差分ハンター用の .mesh をランダム生成する(scripts/oracle-hunt.sh から呼ばれる)。

`sweep_gen.py` との違いは**探索の仕方**:

- `sweep_gen.py` は軸の**直積**を決め打ちで作る。毎回同じ126件なので、記録との突き合わせに向く
- こちらは軸を**ランダムに組み合わせる**。毎回違う形を作るので、「まだ見たことのない
  組み合わせ」へ進める。判定は記録ではなく**TS版オラクル**

**シードを必ず出力すること**が生命線。差が出た形を再現できなければ調査に入れない。

**生成物は必ずパースできること**。パースできない生成物(SKIP)は「一致」に見えて実際には
何も検証していない——milestone 62 で19件中3件が空振りしていたのに一致として数えていた前例がある。

**語彙(FAMILIES)を広げるのがこのファイルの主な保守作業**。狭いと「毎回0件」を報告し続けて
読まれなくなる。初版はnarrowingしか作らず800件回して0件だった(その領域は直した直後だったので
当然)——**直した直後の領域だけを探索しても意味がない**、というのが最初の教訓。
"""

import os
import random
import sys

OUT = sys.argv[1]
SEED = int(sys.argv[2])
COUNT = int(sys.argv[3])

rng = random.Random(SEED)

PRELUDE = """struct Box {
\tv: int | none
}

struct Wrap {
\tinner: Box
}

struct Pt {
\tx: int
\ty: int
}

fn (p: Pt) sum() int {
\treturn p.x + p.y
}

error struct Bad {
\tmsg: string
}

type Mode = "on" | "off"

fn takeInt(n: int) int {
\treturn n
}

fn maybeInt() int | none {
\treturn 1
}

fn failInt() int | error {
\treturn error("x")
}

fn failBad() int | Bad {
\treturn Bad{msg: "b"}
}

fn ident<T>(v: T) T {
\treturn v
}

fn resetBox(b: Box) {
\tb.v = none
}

"""

# ---- family: narrowing(絞り込み) ----------------------------------------
NARROW_PRODUCERS = [
    ("x: int | none = 1", "x", "int"),
    ("x := maybeInt()", "x", "int"),
    ('x := map<string, int>{"a": 1}["k"]', "x", "int"),
    ("ch := chan<int>(1)\n\tch <- 1\n\tx := <-ch", "x", "int"),
    ("x := failInt()", "x", "int"),
    ("b := Box{v: 1}", "b.v", "int"),
    ("w := Wrap{inner: Box{v: 1}}", "w.inner.v", "int"),
]
NARROW_CONDS = [
    "{s} is {t}",
    "!({s} is {t}) == false",
    "!!({s} is {t})",
    "{s} is {t} && 1 == 1",
    "1 == 1 && {s} is {t}",
    "{s} is {t} && {s} > 0",
    "!({s} is {t}) || {s} > 0",
]
NARROW_USES = ["print({s} + 1)", "print(takeInt({s}))", "print({s} > 0)", "print(str({s}))", "print({s} * 2 - 1)"]
NARROW_ELSES = ["", " else {\n\t\tprint(9)\n\t}", " else if 1 == 2 {\n\t\tprint(8)\n\t}"]


def gen_narrow():
    decl, spell, ty = rng.choice(NARROW_PRODUCERS)
    cond = rng.choice(NARROW_CONDS).format(s=spell, t=ty)
    use = rng.choice(NARROW_USES).format(s=spell)
    els = rng.choice(NARROW_ELSES)
    inter = [""]
    if "." in spell:
        inter.append(f"{spell} = none")
    if spell == "b.v":
        inter.append("resetBox(b)")
    pre = rng.choice(inter)
    body = f"\t\t{pre}\n\t\t{use}" if pre else f"\t\t{use}"
    return f"\t{decl}\n\tif {cond} {{\n{body}\n\t}}{els}"


# ---- family: 失敗の伝播(`?` / `or`) --------------------------------------
FAIL_SRC = ["failInt()", "failBad()", "maybeInt()"]
FAIL_FORMS = [
    "v := {src} or 0\n\tprint(v)",
    "v := {src} or e => 0\n\tprint(v)",
    'v := {src} or e => len("${{e}}")\n\tprint(v)',
    "print({src} or 0)",
]


def gen_fail():
    src = rng.choice(FAIL_SRC)
    return "\t" + rng.choice(FAIL_FORMS).format(src=src)


def gen_prop():
    # `?` は失敗しうる戻り値型の関数の中でしか書けない
    src = rng.choice(FAIL_SRC)
    inner = rng.choice(["return {src}?", "v := {src}?\n\t\treturn v", 'v := {src}? "ctx"\n\t\treturn v'])
    body = inner.format(src=src)
    return f"\tg := fn() int | error {{\n\t\t{body}\n\t}}\n\tprint(g() or 0)"


# ---- family: コレクション ------------------------------------------------
COLL_FORMS = [
    'xs := [1, 2, 3]\n\tprint(len(xs))',
    'xs := [1, 2, 3]\n\tprint(xs[1])',
    'xs := [1, 2, 3]\n\tpush(xs, 4)\n\tprint(len(xs))',
    'm := map<string, int>{"a": 1}\n\tprint(len(m))',
    'm := map<string, int>{"a": 1}\n\tm["b"] = 2\n\tprint(m["b"] or 0)',
    'm := map<string, int>{"a": 1}\n\tdelete(m, "a")\n\tprint(len(m))',
    'xs := [3, 1, 2]\n\tsort(xs)\n\tprint(xs[0])',
    'xs := [1, 2, 3]\n\tprint(contains(xs, 2))',
    'xs := [1, 2, 3]\n\tys := map(xs, fn(n: int) int { return n * 2 })\n\tprint(ys[0])',
    'xs := [1, 2, 3]\n\tprint(reduce(xs, fn(a: int, b: int) int { return a + b }, 0))',
    'xs := [1, 2, 3]\n\tzs := filter(xs, fn(n: int) bool { return n > 1 })\n\tprint(len(zs))',
    'for i, v := range [10, 20] {\n\t\tprint(i + v)\n\t}',
]


def gen_coll():
    return "\t" + rng.choice(COLL_FORMS)


# ---- family: match / select ----------------------------------------------
MATCH_FORMS = [
    'u: int | string = 1\n\tmatch u {\n\t\tint => print(1)\n\t\tstring => print(2)\n\t}',
    'u: int | string = 1\n\tr := match u {\n\t\tint => 1\n\t\tstring => 2\n\t}\n\tprint(r)',
    'u: int | none = 1\n\tmatch u {\n\t\tint => print(1)\n\t\tnone => print(0)\n\t}',
    'm: Mode = "on"\n\tmatch m {\n\t\t"on" => print(1)\n\t\t"off" => print(0)\n\t}',
    'u: int | string = 1\n\tmatch u {\n\t\tint => print(1)\n\t\t_ => print(0)\n\t}',
    'c := chan<int>(1)\n\tc <- 1\n\tselect {\n\t\tv := <-c => print(v)\n\t\t_ => print(9)\n\t}',
]


def gen_match():
    return "\t" + rng.choice(MATCH_FORMS)


# ---- family: 並行処理 / defer --------------------------------------------
CONC_FORMS = [
    "t := spawn takeInt(1)\n\tprint(<-t)",
    "c := chan<int>(1)\n\tc <- 1\n\tclose(c)\n\tv := <-c\n\tif v is closed {\n\t\tprint(0)\n\t}",
    "defer print(9)\n\tprint(1)",
    "f := fn() {\n\t\tdefer print(8)\n\t\tprint(2)\n\t}\n\tf()",
    "wait {\n\t\tt := spawn takeInt(1)\n\t\tprint(<-t)\n\t}",
]


def gen_conc():
    return "\t" + rng.choice(CONC_FORMS)


# ---- family: ジェネリクス / メソッド / 文字列 -----------------------------
MISC_FORMS = [
    "print(ident(1))",
    'print(ident("s"))',
    "p := Pt{x: 1, y: 2}\n\tprint(p.sum())",
    'p := Pt{x: 1, y: 2}\n\tprint("${p.x} and ${p.sum()}")',
    's := "a,b,c"\n\tprint(len(split(s, ",")))',
    's := " x "\n\tprint(trim(s))',
    'print(join(["a", "b"], "-"))',
    'print(upper("ab") + lower("CD"))',
    'print(toInt("12") or 0)',
    "print(1 / 2)",
    "print(7 % 3)",
    "print(2.0 / 4.0)",
]


def gen_misc():
    return "\t" + rng.choice(MISC_FORMS)


# ---- family: **壊れた入力** ----------------------------------------------
#
# docs/handoff.md「検証の進め方」が繰り返し記録しているとおり、**実装者の検証を通り抜けた
# 不具合はすべて「入力そのものが壊れているケース」だった**(タイポした型名 / 値にならない式 /
# 壊れた宣言の後ろの型 / 同名の宣言)。きれいなプログラムだけを生成しても、そこは踏めない。
#
# ここで見たいのは「エラーになること」ではなく**エラーの出方がオラクルと一致すること**。
# 診断コードの集合で比べるので、位置や文言のズレは対象外(それはparityコーパスの仕事)。
BROKEN_FORMS = [
    "y: Nope = 1\n\tprint(1)",  # 未知の型名
    "y := 1\n\ty := 2\n\tprint(y)",  # 同名の宣言
    "y := takeInt()\n\tprint(y)",  # 引数不足
    'y := takeInt("s")\n\tprint(y)',  # 引数の型違い
    "y := Pt{x: 1}\n\tprint(y.x)",  # フィールド不足
    "y := Pt{x: 1, y: 2, z: 3}\n\tprint(y.x)",  # 未知フィールド
    "y := Pt{x: 1, y: 2}\n\tprint(y.nope)",  # 未知フィールドの参照
    "y := 1\n\ty = 2\n\tprint(y)",  # 不変への再代入
    'print(1 + "s")',  # 型の食い違う演算
    "y := maybeInt()\n\tprint(y + 1)",  # 絞り込み前のunion使用
    "y := maybeInt()\n\tprint(y.nope)",  # union へのフィールドアクセス
    "y := print(1)\n\tprint(y)",  # 値にならない式
    "y := 1\n\tprint(y())",  # 呼び出せない値
    "y := maybeInt()\n\tif y == none {\n\t\tprint(0)\n\t}",  # == none(is へ誘導)
    "y := 1\n\tprint(nope)",  # 未定義名
    'm := map<string, int>{"a": 1}\n\tprint(m["a"] + 1)',  # V | none の直接使用
    "y: int = 1\n\ty = maybeInt()\n\tprint(y)",  # union を非unionへ代入
    "print(takeInt(1), 2)",  # 引数過多
    "y := [1, 2]\n\tprint(y[0].nope)",  # 要素へのフィールドアクセス
    "y := ident()\n\tprint(y)",  # ジェネリックの推論不能
]


def gen_broken():
    return "\t" + rng.choice(BROKEN_FORMS)


FAMILIES = [gen_narrow, gen_fail, gen_prop, gen_coll, gen_match, gen_conc, gen_misc, gen_broken, gen_broken]

# 文脈(生成した本体をどこへ置くか)。絞り込み以外の族にも入れ子の影響を見たいので共通で使う
WRAPPERS = [
    "{body}",
    "for i := range 2 {{\n{body}\n\t}}",
    "h := fn() {{\n{body}\n\t}}\n\th()",
    "if 1 == 1 {{\n{body}\n\t}}",
]

for i in range(COUNT):
    body = rng.choice(FAMILIES)()
    wrapper = rng.choice(WRAPPERS)
    stmt = body if wrapper == "{body}" else wrapper.format(body="\t" + body.replace("\n\t", "\n\t\t"))
    src = f"{PRELUDE}fn main() {{\n{stmt}\n}}\n"
    with open(os.path.join(OUT, f"hunt_{i:04d}.mesh"), "w") as f:
        f.write(src)

print(f"生成 {COUNT} 件 (seed={SEED})")
