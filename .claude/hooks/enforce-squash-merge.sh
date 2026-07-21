#!/bin/bash
# PreToolUse(Bash) hook: `gh pr merge` は必ずスカッシュマージに統一する。
# --squash / -s が無い gh pr merge を deny して理由を返す。
# それ以外のコマンドは素通り（exit 0 + 出力なし = allow 判定に委ねる）。

cmd=$(jq -r '.tool_input.command // ""')

# gh pr merge 系コマンドだけを対象にする（実際の呼び出しにアンカーし、コメントやecho文字列への誤マッチを避ける）
if printf '%s' "$cmd" | grep -Eq '\bgh[[:space:]]+pr[[:space:]]+merge\b'; then
  # --squash、単独トークンの -s、または -sd のような結合ショートフラグに -s が含まれていれば OK
  if printf '%s' "$cmd" | grep -Eq -- '(--squash|(^|[[:space:]])-[A-Za-z]*s[A-Za-z]*([[:space:]]|$))'; then
    exit 0
  fi

  cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"このリポジトリのマージはスカッシュに統一しています。gh pr merge には必ず --squash を付けてください（例: gh pr merge <PR番号> --squash --delete-branch）。"}}
JSON
  exit 0
fi

exit 0
