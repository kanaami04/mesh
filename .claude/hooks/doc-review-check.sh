#!/bin/bash
# Stopフック: このセッションが編集した.mdがレビュー未実施なら停止をブロックし、doc-reviewを強制する。
#
# 検知の材料は PostToolUse フック(record-touched.sh)がセッション別に書く
# .claude/state/touched-<session_id> のみ。共有マーカーのmtime比較は
# 同一作業ツリーで複数セッションが動くとレビューがすり抜けるため廃止した(ADR-0045)。
#
# 既知の限界: Edit/Write/NotebookEdit以外の経路(シェルが生成した.md等)は記録されない。
# ループ防止: フック起因の継続中(stop_hook_active)は素通しする。
input=$(cat)

if printf '%s' "$input" | jq -e '.stop_hook_active == true' >/dev/null 2>&1; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" || exit 0

sid=$(printf '%s' "$input" | jq -r '.session_id // empty' | tr -cd 'A-Za-z0-9_-')
[ -z "$sid" ] && exit 0

list=".claude/state/touched-$sid"
[ -f "$list" ] || exit 0

# 記録のうち、いま実在する.mdだけを対象にする(消された・改名されたものは無視)。
# caseの `)` は $( ) の中で置換を早期終了させるため、grepで絞る。
changed=$(sort -u "$list" | grep -E '\.md$' | while IFS= read -r f; do
  [ -f "$f" ] && printf '%s\n' "$f"
done | head -20)

if [ -n "$changed" ]; then
  # 一発通知方式(ロックアウト防止): 通知したら記録を消す。
  # doc-reviewスキルも完了時に同じ記録を消すので、二重に消えても害はない。
  : > "$list"
  jq -n --arg files "$changed" '{"decision":"block","reason":("mdファイルが更新されたまま doc-review が未実行です。doc-review スキルを実行し、変更ファイルとその関連文書をレビューしてから完了してください。検出された変更:\n" + $files)}'
fi
exit 0
