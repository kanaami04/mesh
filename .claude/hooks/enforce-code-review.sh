#!/bin/bash
# PreToolUse(Bash) hook: `gh pr merge` の前に、そのPRへ /code-review のレビューコメントが
# 実際に投稿されているかを確認する。無ければ deny して理由を返す。
# それ以外のコマンドは素通り（exit 0 + 出力なし = allow 判定に委ねる）。

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

cmd=$(jq -r '.tool_input.command // ""')

# マージの実行だけを対象にする（判定の詳細と限界は lib.sh の hook_is_pr_merge 参照）
hook_is_pr_merge "$cmd" || exit 0

hook_augment_path

# gh が無ければレビューの有無は確認できない。ここで「コメント0件」として deny すると
# 「レビューを投稿したのに拒否される」という嘘の理由になるので、原因をそのまま伝える。
if ! command -v gh >/dev/null 2>&1; then
  hook_deny "レビューコメントの有無を確認できません: gh コマンドが見つかりません（フックは非対話シェルで動くため ~/.bashrc は読まれません）。docs/setup.md を参照して gh を PATH の通る場所に入れてください。"
  exit 0
fi

# コマンド中のPR番号を拾う（`gh pr merge 38 ...`、`gh pr merge --squash 38` のようにフラグが
# 前後どちらにあっても、"merge" 以降で最初に現れる独立した数値トークンを拾う）。
# 番号が書かれていない場合は現在のブランチに紐づくPRを解決する。
pr_num=$(printf '%s' "$cmd" | grep -oE 'pr[[:space:]]+merge\b.*' | grep -oE '(^|[[:space:]])[0-9]+([[:space:]]|$)' | grep -oE '[0-9]+' | head -1)
if [ -z "$pr_num" ]; then
  pr_num=$(gh pr view --json number -q .number 2>/dev/null)
fi

# PR番号が特定できない場合は誤検知でブロックし続けるより素通りを優先する
if [ -z "$pr_num" ]; then
  exit 0
fi

# gh の失敗（認証切れ・ネットワーク断など）を「コメント0件」と区別する
if ! comments=$(gh pr view "$pr_num" --json comments -q '.comments[].body' 2>&1); then
  hook_deny "PR #$pr_num のレビューコメントを取得できませんでした（gh の実行に失敗）。認証やネットワークを確認してください: $comments"
  exit 0
fi

# /code-review が投稿するコメントは "### Code review" 見出しで始まる（issues found / no issues 共通）
if printf '%s' "$comments" | grep -q '^### Code review'; then
  exit 0
fi

hook_deny "PR #$pr_num にはまだ /code-review のレビューコメントが投稿されていません。マージ前に \`/code-review $pr_num --comment\` を実行してレビューを記録してください（指摘が見つかった場合は対応してから再度レビューを通してください）。"
exit 0
