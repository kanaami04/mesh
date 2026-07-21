#!/bin/bash
# PreToolUse(Bash) hook: `gh pr merge` の前に、そのPRへ /code-review のレビューコメントが
# 実際に投稿されているかを確認する。無ければ deny して理由を返す。
# それ以外のコマンドは素通り（exit 0 + 出力なし = allow 判定に委ねる）。

cmd=$(jq -r '.tool_input.command // ""')

# gh pr merge 系コマンドだけを対象にする
if ! printf '%s' "$cmd" | grep -Eq '\bgh\b.+\bpr\b.+\bmerge\b'; then
  exit 0
fi

# コマンド中のPR番号を拾う（`gh pr merge 38 ...`のような形）。
# 番号が書かれていない場合は現在のブランチに紐づくPRを解決する。
pr_num=$(printf '%s' "$cmd" | grep -oE 'pr[[:space:]]+merge[[:space:]]+[0-9]+' | grep -oE '[0-9]+$')
if [ -z "$pr_num" ]; then
  pr_num=$(gh pr view --json number -q .number 2>/dev/null)
fi

# PR番号が特定できない場合は誤検知でブロックし続けるより素通りを優先する
if [ -z "$pr_num" ]; then
  exit 0
fi

# /code-review が投稿するコメントは "### Code review" 見出しで始まる（issues found / no issues 共通）
if gh pr view "$pr_num" --json comments -q '.comments[].body' 2>/dev/null | grep -q '^### Code review'; then
  exit 0
fi

cat <<JSON
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"PR #$pr_num にはまだ /code-review のレビューコメントが投稿されていません。マージ前に \`/code-review $pr_num --comment\` を実行してレビューを記録してください（指摘が見つかった場合は対応してから再度レビューを通してください）。"}}
JSON
exit 0
