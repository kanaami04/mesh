#!/bin/bash
# push 前に回した code review の記録を残す。
# `.claude/hooks/enforce-review-before-push.sh` がこの記録を見て `git push` を通す。
#
# 使い方:
#   scripts/record-review.sh "<レビュー結果の要約>"      # レビューを回した
#   scripts/record-review.sh --skip "<理由>"             # レビュー不要と判断した
#
# 記録は **HEAD の sha** に紐づく。コミットを積み直したら記録は当たらなくなり、
# 再レビューが要求される（直した内容は誰もレビューしていないので、これが正しい）。
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
使い方:
  scripts/record-review.sh "<レビュー結果の要約>"
  scripts/record-review.sh --skip "<レビュー不要と判断した理由>"

要約・理由は必須です（空の記録は「レビューを回した記録」として意味を成さないため、
フック側も中身が空なら通しません）。
EOF
  exit 2
}

kind=review
case "${1-}" in
  --skip) kind=skip; shift ;;
  -h | --help) usage ;;
  -*) printf 'error: 知らないオプションです: %s\n' "$1" >&2; usage ;;
esac

note=${1-}
[ $# -le 1 ] || { printf 'error: 引数が多すぎます（要約は1つの文字列にまとめてください）\n' >&2; usage; }
# 空白だけの要約は空と同じ
[ -n "${note//[[:space:]]/}" ] || usage

root=$(git rev-parse --show-toplevel)
sha=$(git rev-parse HEAD)
branch=$(git rev-parse --abbrev-ref HEAD)
# フックと同じ場所を使う（片方だけ直る事故を防ぐため定義は lib.sh に一本化してある）。
# lib.sh は **このスクリプトからの相対**で読む——`$root` 基準にすると、別のリポジトリを
# カレントにして実行したときに読めずに落ちる（実際に踏んだ）
# shellcheck source=../.claude/hooks/lib.sh
source "${BASH_SOURCE[0]%/*}/../.claude/hooks/lib.sh"
log_dir=$(hook_review_log_dir "$root")
mkdir -p "$log_dir"

# 記録は HEAD に紐づくので、未コミットの変更が残っているとレビューした内容と
# push される内容がずれる。止めはしないが気づけるようにする
if ! git diff --quiet HEAD -- 2>/dev/null || [ -n "$(git ls-files --others --exclude-standard)" ]; then
  printf 'warning: 未コミットの変更があります。記録は HEAD (%s) に紐づくので、\n' "${sha:0:12}" >&2
  printf '         レビューした内容がコミットに入っているか確認してください。\n' >&2
fi

{
  printf 'sha: %s\n' "$sha"
  printf 'branch: %s\n' "$branch"
  printf 'kind: %s\n' "$kind"
  printf 'date: %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  # note は最後に置く。フックは `^note:` の後ろが空でないことを見る
  printf 'note: %s\n' "$note"
} > "$log_dir/$sha"

# 記録は gitignore されたローカルの作業ファイル。放っておくと際限なく溜まるので、
# 新しい50件だけ残す（sha 以外のファイル名は作られないので空白の心配は無い）
ls -1t "$log_dir" 2>/dev/null | tail -n +51 | while IFS= read -r old; do
  rm -f "$log_dir/$old"
done

printf 'code review を記録しました: %s (%s)\n' "$log_dir/$sha" "$kind"
