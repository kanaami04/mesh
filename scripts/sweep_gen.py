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

HEAD_COND = """struct Holder {
\tv: int | none
}

"""

# 軸: **絞り込んだ値を条件式の「右辺」で使う**(2026-07-28追加)。
#
# 上の CONDS は `x is int && 1 == 1` のように**右辺が絞り込みと無関係**な形しか作らない。
# そのため「左の`is`が右辺の式に効く」(F-6でdocs記載済み)が壊れていても直積に出てこなかった
# ——実際に `full_checker::infer_binary` が右辺を絞り込み前のスコープで検査していて
# `narrow-required` の誤検知になる、という穴を長く見逃していた。
#
# subjectを裸の識別子とフィールドパスの両方で作るのは、絞り込みの実装がこの2つで
# 別扱いになっているため(`stable_path`を通るか否か)。
#
# **注意: この軸が作る6件は現在すべて誤検知として記録されている**
# (`incomparable-types` / `invalid-operation`)。`tests/sweep-expected.txt` は
# 「今の出力」を凍結する記録であって「正しい出力」ではない——この6件は**直すべき出力**。
# 誤検知を直せば記録から診断が消えるので、そのときの差分が修正できた証拠になる。
# 追跡は `docs/handoff.md`「性質検査(オラクルの代わり)」節と
# `tests/parity/logical-and-narrow-right-operand/`。
COND_SUBJECTS = {
    "ident": ("x: int | none = 1", "x"),
    "field": ("h := Holder{v: 1}", "h.v"),
}
COND_RIGHT_USES = {
    "and_cmp": "{s} is int && {s} > 0",
    "or_cmp": "!({s} is int) || {s} > 0",
    "and_arith": "{s} is int && {s} + 1 > 0",
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

for (sk, (decl, subj)), (uk, uv) in itertools.product(COND_SUBJECTS.items(), COND_RIGHT_USES.items()):
    cond = uv.format(s=subj)
    write(f"cond_{sk}_{uk}", f"{HEAD_COND}fn main() {{\n\t{decl}\n\tif {cond} {{\n\t\tprint(1)\n\t}}\n}}\n")
    n += 1

print(f"生成 {n} 件 -> {OUT}")
