#!/bin/bash
# Stopフック: 前回doc-review以降に.mdが更新されていたら停止をブロックし、doc-reviewを強制する。
# ループ防止: フック起因の継続中(stop_hook_active)は素通しする。
input=$(cat)

if echo "$input" | jq -e '.stop_hook_active == true' >/dev/null 2>&1; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" || exit 0
marker=".claude/.doc-review-marker"

# 未コミットのmd変更が無ければ検査不要(pull/checkout等によるmtime更新の誤検知を防ぐ。
# コミット済みの内容はマージ可能ルールでレビュー済みのため対象外)
if [ -z "$(git status --porcelain -- '*.md' 2>/dev/null)" ]; then
  touch "$marker" 2>/dev/null
  exit 0
fi

# 初回はマーカーを作るだけ(それ以前の変更は対象にしない)
if [ ! -f "$marker" ]; then
  touch "$marker"
  exit 0
fi

changed=$(find . -name '*.md' -not -path './.git/*' -not -path './node_modules/*' -not -path './target/*' -newer "$marker" 2>/dev/null | head -20)

if [ -n "$changed" ]; then
  touch "$marker"
  jq -n --arg files "$changed" '{"decision":"block","reason":("mdファイルが更新されたまま doc-review が未実行です。doc-review スキルを実行し、変更ファイルとその関連文書をレビューしてから完了してください。検出された変更:\n" + $files)}'
fi
exit 0
