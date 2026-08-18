#!/bin/bash
# Stopフック: 作業ブランチにレビュー未実施のコード変更(src/ tests/ Cargo.toml)が
# コミットされていたら停止をブロックし、impl-reviewスキルを強制する。
#
# 状態を2種類に分ける(ADR-0045):
#   .claude/state/impl-reviewed-commits … 実際にレビューしたコミット(共有・追記のみ)
#   .claude/state/impl-exempted-commits … ユーザーが免除したコミット(共有・追記のみ)
#   .claude/state/impl-notified-<sid>   … このセッションへ通知済み(ロックアウト防止・セッション別)
#
# 旧方式(「レビュー済み地点」を指すポインタを、ブロックの副作用で前進させる)は
#   (a) 通知を無視すれば未レビューのまま緑になる
#   (b) 同一作業ツリーの他セッションのゲートまで消費する
# の2つの穴があったため廃止した。レビュー済みの記録はレビューを実行した者だけが追記する。
#
# 例外: コード変更なし / mainブランチ / ユーザー免除(スキルの例外手順)。
# ループ防止: フック起因の継続中(stop_hook_active)は素通しする。
input=$(cat)

if printf '%s' "$input" | jq -e '.stop_hook_active == true' >/dev/null 2>&1; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" || exit 0

# mainブランチ・detached HEADは対象外
branch=$(git branch --show-current 2>/dev/null)
if [ -z "$branch" ] || [ "$branch" = "main" ]; then
  exit 0
fi

base=$(git merge-base origin/main HEAD 2>/dev/null || git merge-base main HEAD 2>/dev/null)
[ -z "$base" ] && exit 0

sid=$(printf '%s' "$input" | jq -r '.session_id // empty' | tr -cd 'A-Za-z0-9_-')
[ -z "$sid" ] && exit 0

mkdir -p .claude/state || exit 0
reviewed=".claude/state/impl-reviewed-commits"
exempted=".claude/state/impl-exempted-commits"
notified=".claude/state/impl-notified-$sid"
touch "$reviewed" "$exempted" "$notified" 2>/dev/null || exit 0

# base..HEAD のうちコードを触るコミットで、まだ誰もレビューしておらず、
# このセッションにも未通知のものを集める
pending=$(git rev-list "$base"..HEAD -- src tests Cargo.toml 2>/dev/null | while IFS= read -r sha; do
  [ -z "$sha" ] && continue
  grep -qxF "$sha" "$reviewed" && continue
  grep -qxF "$sha" "$exempted" && continue
  grep -qxF "$sha" "$notified" && continue
  printf '%s\n' "$sha"
done)

if [ -n "$pending" ]; then
  # 通知済みとして記録する(同じコミットで無限にブロックしない)。
  # レビュー済みにはしない——それはimpl-reviewスキルが実行後に追記する。
  printf '%s\n' "$pending" >> "$notified"
  list=$(printf '%s\n' "$pending" | while IFS= read -r sha; do
    [ -n "$sha" ] && git log -1 --format='%h %s' "$sha" 2>/dev/null
  done | head -20)
  jq -n --arg c "$list" '{"decision":"block","reason":("レビュー未実施のコード変更コミットがあります。impl-review スキルを実行してから完了してください(ユーザーがレビュー不要と明示した場合はスキルの例外手順に従う)。対象コミット:\n" + $c)}'
fi
exit 0
