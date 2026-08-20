#!/bin/bash
# PreToolUseフック: これから触るファイルに紐づく申し送り(docs/adr/DEFERRED.md)を提示する。
#
# 先送りした決定は、着手の瞬間に目に入らなければ放置される(ADR-0049)。
# 台帳のトリガー欄が `file:<パス>` の行を、そのファイルのEdit/Write直前に出す。
#
# ブロックはしない。additionalContext で知らせるだけで作業は続行できる
# (PreToolUseでは素の標準出力はモデルに届かないためJSONで返す)。
# 同じ(セッション, ID)の組では1回だけ出す——毎回出すと通知が雑音になる。
#
# 注意: case のパターンにある `)` は $( ) の中で置換を早期終了させるため、
# 行の絞り込みには grep を使う(doc-review-check.sh で踏んだ罠)。
input=$(cat)

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" || exit 0
ledger="docs/adr/DEFERRED.md"
[ -f "$ledger" ] || exit 0

path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')
[ -z "$path" ] && exit 0

sid=$(printf '%s' "$input" | jq -r '.session_id // empty' | tr -cd 'A-Za-z0-9_-')
[ -z "$sid" ] && exit 0

proj=$(pwd -P)
rel="${path#"$proj"/}"

mkdir -p .claude/state || exit 0
shown=".claude/state/deferred-shown-$sid"
touch "$shown" 2>/dev/null || exit 0

# 台帳から「file:<rel>」をトリガーに持つ未解決の行を集める。
# 状態が「解決」の行は対象外(歴史として残してあるが対応済み)。
rows=$(grep '^| D-' "$ledger" | grep -F "file:$rel" | grep -v '| *解決')
[ -z "$rows" ] && exit 0

# このセッションで未通知のIDだけに絞る
new_ids=$(printf '%s\n' "$rows" | sed -E 's/^\| *(D-[0-9]+) *\|.*/\1/' | while IFS= read -r id; do
  [ -n "$id" ] && ! grep -qxF "$id" "$shown" && printf '%s\n' "$id"
done)
[ -z "$new_ids" ] && exit 0

body=$(printf '%s\n' "$new_ids" | while IFS= read -r id; do
  printf '%s\n' "$rows" | grep -F "| $id |"
done)
printf '%s\n' "$new_ids" >> "$shown"

jq -n --arg p "$rel" --arg b "$body" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "allow",
    additionalContext: ("申し送り台帳(docs/adr/DEFERRED.md)に、これから触る " + $p + " をトリガーにした先送り事項があります。着手前に扱いを判断し、対応するか「いまはやらない」と決めた理由を台帳に記録してください(作業は続行できます)。\n\n" + $b)
  }
}'
exit 0
