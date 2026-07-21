#!/bin/bash
# PreToolUse(Bash) hook: `gh pr merge` の前に、そのPRへ /code-review のレビューコメントが
# 実際に投稿されているかを確認する。無ければ deny して理由を返す。
# それ以外のコマンドは素通り（exit 0 + 出力なし = allow 判定に委ねる）。
#
# 原則: **確認できないときは deny する**。フックは「効いているはず」と信じて運用する
# ものなので、壊れたときに無言で全許可に転ぶのが最も避けたい壊れ方。

# deny 応答を出す最小限の実装。lib.sh を読む前に失敗した場合でも理由を返せるようにする。
bail() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$1"
  exit 0
}

# dirname ではなくパラメータ展開で親ディレクトリを得る。dirname は外部コマンドなので、
# PATH が痩せた状態（まさにこのフックが備えたい状況）では解決できず、lib.sh を
# 見失って「ライブラリが無い」という的外れな理由で拒否してしまう。
lib="${BASH_SOURCE[0]%/*}/lib.sh"
# source の失敗を検査しないと、lib.sh が欠けたときに未定義関数が 127 を返し、
# それが `|| exit 0` に吸われて「無言で全許可」になる。
# shellcheck source=./lib.sh
if [ ! -r "$lib" ] || ! source "$lib"; then
  bail "フックの共通ライブラリ .claude/hooks/lib.sh を読み込めません。レビューの有無を確認できないためマージを拒否します。"
fi

# jq より先に PATH を補う（jq 自体が mise 管理下にしかないことがある）
hook_augment_path

for tool in jq gh; do
  command -v "$tool" >/dev/null 2>&1 || bail "レビューコメントの有無を確認できません: $tool コマンドが見つかりません（フックは非対話シェルで動くため ~/.bashrc は読まれません）。docs/setup.md を参照してください。"
done

cmd=$(jq -r '.tool_input.command // ""')

# マージの実行だけを対象にする（判定の詳細と限界は lib.sh の hook_is_pr_merge 参照）
hook_is_pr_merge "$cmd" || exit 0

# コマンド中のPR番号を拾う（`gh pr merge 38 ...`、`gh pr merge --squash 38` のようにフラグが
# 前後どちらにあっても、"merge" 以降で最初に現れる独立した数値トークンを拾う）。
# 番号が書かれていない場合は現在のブランチに紐づくPRを解決する。
pr_num=$(printf '%s' "$cmd" | grep -oE 'pr[[:space:]]+merge\b.*' | grep -oE '(^|[[:space:]])[0-9]+([[:space:]]|$)' | grep -oE '[0-9]+' | head -1)
if [ -z "$pr_num" ]; then
  pr_num=$(gh pr view --json number -q .number 2>/dev/null)
fi

if [ -z "$pr_num" ]; then
  bail "マージ対象のPR番号を特定できませんでした。レビューの有無を確認できないため拒否します。番号を明示してください（例: gh pr merge 12 --squash）。"
fi

# gh の失敗（認証切れ・ネットワーク断など）を「コメント0件」と区別する
if ! comments=$(gh pr view "$pr_num" --json comments -q '.comments[].body' 2>&1); then
  bail "PR #$pr_num のレビューコメントを取得できませんでした（gh の実行に失敗）。認証やネットワークを確認してください: $comments"
fi

# /code-review が投稿するコメントは "### Code review" 見出しで始まる（issues found / no issues 共通）
if printf '%s' "$comments" | grep -q '^### Code review'; then
  exit 0
fi

hook_deny "PR #$pr_num にはまだ /code-review のレビューコメントが投稿されていません。マージ前に \`/code-review $pr_num --comment\` を実行してレビューを記録してください（指摘が見つかった場合は対応してから再度レビューを通してください）。"
exit 0
