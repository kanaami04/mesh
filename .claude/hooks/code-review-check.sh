#!/bin/bash
# Stopフック: 作業ブランチにレビュー未実施のコード変更(src/ tests/ Cargo.toml)が
# コミットされていたら停止をブロックし、code-reviewスキルを強制する。
# マーカー(.claude/.code-review-marker)は「どのコミットまでレビュー済みか」のハッシュを保持する。
# 例外: コード変更なし / mainブランチ / ユーザー免除(スキルの例外手順でマーカー更新)。
# ループ防止: フック起因の継続中(stop_hook_active)は素通しする。
input=$(cat)

if echo "$input" | jq -e '.stop_hook_active == true' >/dev/null 2>&1; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" || exit 0
marker=".claude/.code-review-marker"

# mainブランチ・detached HEADは対象外
branch=$(git branch --show-current 2>/dev/null)
if [ -z "$branch" ] || [ "$branch" = "main" ]; then
  exit 0
fi

base=$(git merge-base origin/main HEAD 2>/dev/null || git merge-base main HEAD 2>/dev/null)
[ -z "$base" ] && exit 0

# ブランチにコード変更が無ければ対象外(md等のみの作業)
if [ -z "$(git diff --name-only "$base"..HEAD -- src tests Cargo.toml 2>/dev/null)" ]; then
  exit 0
fi

# 初回・マーカー不正時は現在地を記録して素通り(それ以前の変更は対象にしない)
reviewed=$(head -1 "$marker" 2>/dev/null | tr -d '[:space:]')
if [ -z "$reviewed" ] || ! git cat-file -e "$reviewed" 2>/dev/null; then
  git rev-parse HEAD > "$marker" 2>/dev/null
  exit 0
fi

# レビュー済み地点以降のコード変更コミットを検出
unreviewed=$(git log --oneline "$reviewed"..HEAD -- src tests Cargo.toml 2>/dev/null | head -20)
if [ -n "$unreviewed" ]; then
  # 一度ブロックしたらマーカーを進める(doc-review同様、ロックアウト防止の一発通知方式)
  git rev-parse HEAD > "$marker" 2>/dev/null
  jq -n --arg c "$unreviewed" '{"decision":"block","reason":("レビュー未実施のコード変更コミットがあります。code-review スキルを実行してから完了してください(ユーザーがレビュー不要と明示した場合はスキルの例外手順に従う)。対象コミット:\n" + $c)}'
fi
exit 0
