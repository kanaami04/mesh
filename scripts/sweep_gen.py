"""組み合わせスイープ用の .mesh を生成する(scripts/sweep.sh から呼ばれる)。

軸を足したいときはここの辞書へ1行足す。**テンプレートを足したら必ず sweep.sh を回して
SKIP が0であることを確認すること**——パースできない生成物は「一致」に見えるが実際には
何も検証していない(milestone 62 で19件中3件が空振りしていた)。
"""

import itertools
import os
import sys

OUT = sys.argv[1]

HEAD_NARROW = """fn take(n: int) int {
\treturn n
}

fn maybe() int | none {
\treturn 1
}

"""

# 軸1: `int | none` の値の作り方
PRODUCERS = {
    "ann": "x: int | none = 1",
    "call": "x := maybe()",
    "map": 'x := map<string, int>{"a": 1}["a"]',
    "recv": "ch := chan<int>(1)\n\tch <- 1\n\tx := <-ch",
}

# 軸2: 絞り込み条件の書き方(単体ではどれもコーパスにあるが、組み合わせが穴だった)
CONDS = {
    "is": "x is int",
    "not_none": "!(x is none)",
    "and": "x is int && 1 == 1",
    "and_rev": "1 == 1 && x is int",
    "notnot": "!!(x is int)",
    "not_and": "!(x is none && 1 == 2)",
}

# 軸3: 絞り込んだ値の使い方
USES = {
    "call": "print(take(x))",
    "arith": "print(x + 1)",
}

HEAD_EXPR = """struct Box {
\tv: int
}

fn (b: Box) doubled() Box {
\treturn Box{v: b.v * 2}
}

fn fail() int | error {
\treturn error("x")
}

fn ident<T>(x: T) T {
\treturn x
}

"""

TAIL_EXPR = """
fn need(s: string) int {
\treturn len(s)
}
"""

# 軸: 値を作る式 × 型の食い違いを誘発する使い方
EXPRS = {
    "structlit": "Box{v: 1}",
    "method": "Box{v: 1}.doubled()",
    "generic": "ident(Box{v: 1})",
    "index": "[Box{v: 1}][0]",
    "orelse": "(fail() or _ => 0)",
    "fnexpr": "(fn() int { return 1 })()",
}
EXPR_USES = {
    "bad_add": 'print({} + "s")',
    "bad_field": "print(({}).nope)",
    "bad_arg": "print(need({}))",
    "ok": "print(str({}))",
}

TAIL_STMT = """
fn need(s: string) int {
\treturn len(s)
}

fn fail() int | error {
\treturn error("x")
}
"""

# 軸: 文の種類 × その中で起こすエラー(match/select のアームは**式**であってブロックではない)
STMTS = {
    "for": "for i := 0; i < 3; i = i + 1 {{\n\t\t{}\n\t}}",
    "rangefor": "for _, e := range [1, 2] {{\n\t\t{}\n\t}}",
    "forever": "for {{\n\t\t{}\n\t\tbreak\n\t}}",
    "defer": "defer fn() {{\n\t\t{}\n\t}}()",
    "spawn": "spawn fn() {{\n\t\t{}\n\t}}()",
    "if": "if 1 == 1 {{\n\t\t{}\n\t}}",
    "else": "if 1 == 2 {{\n\t\tprint(0)\n\t}} else {{\n\t\t{}\n\t}}",
    "fnexpr": "f := fn() {{\n\t\t{}\n\t}}\n\tf()",
}
STMT_INNER = {
    "undef": "print(nope)",
    "badadd": 'print(1 + "s")',
    "badcall": "print(need(1))",
    "ok": "print(1)",
}

ARMS = {
    "match": "u: int | string = 1\n\tmatch u {{\n\t\tint => {}\n\t\tstring => print(0)\n\t}}",
    "select": "c := chan<int>(1)\n\tc <- 1\n\tselect {{\n\t\tv := <-c => {}\n\t\t_ => print(9)\n\t}}",
    "matchval": "u: int | string = 1\n\tr := match u {{\n\t\tint => {}\n\t\tstring => 0\n\t}}\n\tprint(r)",
    "orelse": "v := fail() or e => {}\n\tprint(v)",
}
ARM_INNER = {"undef": "nope", "badadd": '1 + "s"', "badcall": "need(1)", "ok": "1"}


def write(name, body):
    with open(os.path.join(OUT, name + ".mesh"), "w") as f:
        f.write(body)


n = 0
for (pk, pv), (ck, cv), (uk, uv) in itertools.product(PRODUCERS.items(), CONDS.items(), USES.items()):
    write(f"narrow_{pk}_{ck}_{uk}", f"{HEAD_NARROW}fn main() {{\n\t{pv}\n\tif {cv} {{\n\t\t{uv}\n\t}}\n}}\n")
    n += 1

for (ek, ev), (uk, uv) in itertools.product(EXPRS.items(), EXPR_USES.items()):
    write(f"expr_{ek}_{uk}", f"{HEAD_EXPR}fn main() {{\n\t{uv.format(ev)}\n}}\n{TAIL_EXPR}")
    n += 1

for (sk, sv), (ik, iv) in itertools.product(STMTS.items(), STMT_INNER.items()):
    write(f"stmt_{sk}_{ik}", f"fn main() {{\n\t{sv.format(iv)}\n}}\n{TAIL_STMT}")
    n += 1

for (sk, sv), (ik, iv) in itertools.product(ARMS.items(), ARM_INNER.items()):
    write(f"arm_{sk}_{ik}", f"fn main() {{\n\t{sv.format(iv)}\n}}\n{TAIL_STMT}")
    n += 1

print(f"生成 {n} 件 -> {OUT}")
