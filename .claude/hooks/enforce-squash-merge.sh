#!/bin/bash
# PreToolUse(Bash) hook: `gh pr merge` は必ずスカッシュマージに統一する。
# --squash / -s が無い gh pr merge を deny して理由を返す。
# それ以外のコマンドは素通り（exit 0 + 出力なし = allow 判定に委ねる）。

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

cmd=$(jq -r '.tool_input.command // ""')

# マージの実行だけを対象にする（判定の詳細と限界は lib.sh の hook_is_pr_merge 参照）
hook_is_pr_merge "$cmd" || exit 0

# --squash、単独トークンの -s、または -sd のような結合ショートフラグに -s が含まれていれば OK
if printf '%s' "$cmd" | grep -Eq -- '(--squash|(^|[[:space:]])-[A-Za-z]*s[A-Za-z]*([[:space:]]|$))'; then
  exit 0
fi

hook_deny "このリポジトリのマージはスカッシュに統一しています。gh pr merge には必ず --squash を付けてください（例: gh pr merge <PR番号> --squash --delete-branch）。"
exit 0
