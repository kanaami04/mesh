#!/bin/bash
# PreToolUseフック: これから触るファイルに紐づく申し送り(docs/adr/DEFERRED.md)を提示する。
#
# 先送りした決定は、着手の瞬間に目に入らなければ放置される(ADR-0049)。
# 台帳のトリガー欄が `file:<パス>` の行を、そのファイルのEdit/Write直前に出す。
#
# ブロックも許可もしない。additionalContext で知らせるだけで、許可判断は通常のフローに委ねる
# (PreToolUseでは素の標準出力はモデルに届かないためJSONで返す)。
# 同じ(セッション, ID)の組では1回だけ出す——毎回出すと通知が雑音になる。
#
# 注意: case のパターンにある `)` は $( ) の中で置換を早期終了させるため、
# 行の絞り込みには grep を使う(doc-review-check.sh で踏んだ罠)。
input=$(cat)

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" || exit 0
path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')
[ -z "$path" ] && exit 0

sid=$(printf '%s' "$input" | jq -r '.session_id // empty' | tr -cd 'A-Za-z0-9_-')
[ -z "$sid" ] && exit 0

# パスをプロジェクトルート相対に正規化する。symlink差を吸収するため実体パスで比較する。
# 存在しないディレクトリへの新規Write(`src/parser/mod.rs` を作る瞬間など)でも
# 効くよう、実在する最上位の祖先まで遡って解決する。
proj=$(pwd -P)
abs="$path"
case "$abs" in
  /*) ;;
  *) abs="$proj/$abs" ;;
esac
tail_part=""
probe=$(dirname "$abs")
base=$(basename "$abs")
while [ "$probe" != "/" ]; do
  if resolved=$(cd "$probe" 2>/dev/null && pwd -P); then
    abs="$resolved${tail_part}/$base"
    break
  fi
  tail_part="/$(basename "$probe")${tail_part}"
  probe=$(dirname "$probe")
done
rel="${abs#"$proj"/}"

# worktree(<repo>/.claude/worktrees/<name>/)配下なら、**台帳と状態ファイルの根も**
# そのworktreeへ付け替える。パスだけ読み替えて根を本体に残すと、worktreeで台帳を
# 更新しても本体の古い台帳から撃たれる(impl-review 2026-08-20の指摘)。
root="$proj"
wt=$(printf '%s' "$rel" | sed -nE 's|^(\.claude/worktrees/[^/]+)/.*|\1|p')
if [ -n "$wt" ] && [ -d "$proj/$wt" ]; then
  root="$proj/$wt"
  rel="${rel#"$wt"/}"
fi

ledger="$root/docs/adr/DEFERRED.md"
[ -f "$ledger" ] || exit 0

mkdir -p "$root/.claude/state" || exit 0
shown="$root/.claude/state/deferred-shown-$sid"
touch "$shown" 2>/dev/null || exit 0

# 台帳から「file:<rel>」をトリガーに持つ行を集め、1行ずつ判定する。
# 行の再取得(grep で引き直す)はしない——ID欄の空白のゆらぎで本文が引けず、
# 「中身が空の通知を1回出して以後永久に沈黙する」事故になるため(impl-review 2026-08-20)。
rows=$(grep '^|[[:space:]]*D-' "$ledger" | grep -F "file:$rel")
[ -z "$rows" ] && exit 0

body=""
new_ids=""
while IFS= read -r row; do
  [ -z "$row" ] && continue
  id=$(printf '%s' "$row" | awk -F'|' '{print $2}' | tr -d '[:space:]*')
  # ID欄がD-<数字>の形でない行は台帳の書式が崩れている。黙って落とさず知らせる。
  printf '%s' "$id" | grep -qE '^D-[0-9]+$' || {
    # 書式不正も1セッション1回に抑える。毎回出すと雑音になり、
    # 正常な行の通知まで埋もれる(impl-review 2026-08-20)。
    bad_key="BAD:$(printf '%s' "$row" | cksum | tr -d ' ')"
    grep -qxF "$bad_key" "$shown" && continue
    new_ids="${new_ids}${bad_key}
"
    body="${body}(書式が不正な行があります。ID欄を確認してください): ${row}
"
    continue
  }
  # 状態欄(最終列)だけを見る。行のどこかに「解決」の語があるだけで沈黙しないように。
  # 状態欄=最後の非空フィールド。GFMは末尾の `|` を省略できるため、
  # $(NF-1) 固定だと省略時に1列ずれて「解決」を読み落とす(impl-review 2026-08-20)。
  state=$(printf '%s' "$row" | awk -F'|' '{for(i=NF;i>=1;i--){gsub(/^[ \t]+|[ \t]+$/,"",$i); if($i!=""){print $i; exit}}}' | tr -d '*')
  printf '%s' "$state" | grep -q '^解決' && continue
  grep -qxF "$id" "$shown" && continue
  new_ids="${new_ids}${id}
"
  body="${body}${row}
"
done <<ROWS
$rows
ROWS

[ -z "$body" ] && exit 0
[ -n "$new_ids" ] && printf '%s' "$new_ids" | grep -E '^(D-[0-9]+|BAD:[0-9]+)$' >> "$shown"

# permissionDecision は返さない。"allow" は許可プロンプトを飛ばすため、
# 「着手前に人が判断すべき」と印を付けたファイルに限って許可判断を奪ってしまう
# (impl-review 2026-08-20の指摘)。決定を省けば通常の許可フローがそのまま働き、
# additionalContext だけがモデルに届く。
jq -n --arg p "$rel" --arg b "$body" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    additionalContext: ("申し送り台帳(docs/adr/DEFERRED.md)に、これから触る " + $p + " をトリガーにした先送り事項があります。着手前に扱いを判断し、対応するか「いまはやらない」と決めた理由を台帳に記録してください(作業は続行できます)。\n\n" + $b)
  }
}'
exit 0
