#!/usr/bin/env bash
# 「自分の変更が偽にしたコメント」の**候補**を機械的に出す。
#
# milestone 46〜51で**7回連続**、code review指摘の大半がこれだった(実装は正しいのに、
# 周りのコメントが「その診断は未移植」「〜は効かない」と言ったまま残る)。毎回grepの
# 検索語を思いつきで決めていて取りこぼしたので、検索語の作り方を固定するのが目的。
#
#   scripts/drift.sh [base]   # base(既定 origin/main)からの差分を見る
#
# **判定はしない**。「今も正しいか」はコメントを読まないと分からないので、候補を出すまで。
# 出た行を1件ずつ潰すのは人間(とClaude)の仕事。
set -uo pipefail
cd "$(dirname "$0")/.."

BASE="${1:-origin/main}"
git rev-parse --verify -q "$BASE" >/dev/null || {
	echo "error: '$BASE' が解決できない" >&2
	exit 2
}

PATHS=(rust/src src)

# 検索語を3種類つくる(docs/handoff.md「検証の進め方」2''と同じ分類)。
#   (a) 追加した診断コード名 … `Xxx => "kebab-case"` の追加行
#   (b) **変更が入った関数の名前** … hunkヘッダ(`@@ ... @@ fn foo(`)から取る。
#       追加/削除行から`fn name`を拾う方式だと、**本体だけ変えた関数**を取りこぼす
#       ——milestone 51で7件中4件がそれ(`resolve_type_ann`等)だった
#   (c) 触った構造の名前 … `*_table`/`*_registry`/`scopes[0]` のような仕組みの名前
added_codes=$(git diff "$BASE"...HEAD -- "${PATHS[@]}" | grep '^+' | grep -oE '=> "[a-z][a-z-]+"' | tr -d '=>" ' | sort -u)
changed_fns=$(git diff "$BASE"...HEAD -U0 -- "${PATHS[@]}" |
	grep -oE '^@@[^@]*@@ *(pub )?(pub\(crate\) )?fn [a-z_0-9]+' | grep -oE 'fn [a-z_0-9]+$' | sed 's/^fn //' | sort -u)
touched_idents=$(git diff "$BASE"...HEAD -- "${PATHS[@]}" | grep -E '^[+-]' |
	grep -oE '\b(scopes\[0\]|[a-z_]{3,}_(table|registry|types|aliases|exports|decls))\b' | sort -u)

# 一般名詞すぎて意味のあるヒットにならない語は落とす(`add`/`go`/`main`/`new`等)
terms=$(printf '%s\n%s\n%s\n' "$added_codes" "$changed_fns" "$touched_idents" |
	grep -v '^$' | awk 'length($0) >= 6' |
	grep -vxE 'main|new|check|value|types|format|insert|expect|result' | sort -u)

if [ -z "$terms" ]; then
	echo "検索語が取れなかった($BASE からの ${PATHS[*]} の差分が無い?)"
	exit 0
fi

echo "=== 検索語(a:追加した診断コード b:変更が入った関数 c:触った仕組みの名前)"
printf '%s\n' "$terms" | tr '\n' ' '
echo
echo
echo "=== 候補: これらの語と「未移植/対象外/次段階/効かない/潰れる/未対応」が同じ行にある箇所"
echo "    (todo.md は日付つきの作業ログなので対象外——現在地を書いているのは下記だけ)"
hits=0
for t in $terms; do
	out=$(grep -rn -- "$t" "${PATHS[@]}" docs/handoff.md docs/features.md 2>/dev/null |
		grep -E '未移植|対象外|次段階|効かない|潰れ|未対応|ANYのまま|ANYへ' |
		grep -v '^docs/handoff.md:[0-9]*: *[(（][abc][)）]')
	if [ -n "$out" ]; then
		echo "--- $t"
		printf '%s\n' "$out" | sed 's/^/    /'
		hits=$((hits + $(printf '%s\n' "$out" | wc -l)))
	fi
done

echo
echo "候補 ${hits} 件。**このスクリプトは判定しない**——古い記述かどうかは読んで判断する。"
echo "あわせて上記(b)の関数の**docコメントを必ず読み直すこと**——grepの語では拾えない"
echo "「〜は効かない」という能力の説明が、変更した関数自身の説明に残りやすい。"
