#!/bin/bash
# PostToolUseフック: このセッションが編集したファイルを記録する。
# doc-review-check.sh が「自分が触った.md」だけを検査するための土台(ADR-0045)。
#
# 記録先をセッション別に分けるのが要点。同一作業ツリーで複数セッションが動いても、
# 片方のレビューがもう片方の検知をリセットしてしまうことがない。
input=$(cat)

# セッションIDはファイル名に使うので、英数・ハイフン・アンダースコアだけに削る
sid=$(printf '%s' "$input" | jq -r '.session_id // empty' | tr -cd 'A-Za-z0-9_-')
[ -z "$sid" ] && exit 0

path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')
[ -z "$path" ] && exit 0

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" || exit 0
proj=$(pwd -P)

# file_pathは絶対パスで来る想定だが、相対で来ても壊れないようにする
case "$path" in
  /*) abs="$path" ;;
  *)  abs="$proj/$path" ;;
esac

# プロジェクト外のファイル(~/.claude/ の設定など)は記録しない
case "$abs" in
  "$proj"/*) ;;
  *) exit 0 ;;
esac

mkdir -p .claude/state || exit 0
printf '%s\n' "${abs#"$proj"/}" >> ".claude/state/touched-$sid"
exit 0
