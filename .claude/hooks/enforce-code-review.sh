#!/bin/bash
# PreToolUse(Bash) hook: `gh pr merge` の前に、そのPRへ /code-review のレビューコメントが
# 実際に投稿されているかを確認する。無ければ deny して理由を返す。
# それ以外のコマンドは素通り（exit 0 + 出力なし = allow 判定に委ねる）。
#
# 原則: **確認できないときは deny する**。フックは「効いているはず」と信じて運用する
# ものなので、壊れたときに無言で全許可に転ぶのが最も避けたい壊れ方。

# deny 応答を出す最小限の実装。lib.sh を読む前に失敗した場合でも理由を返せるようにする。
# JSON エスケープは lib.sh の hook_json_escape と同じロジックをここに複製している
# （bail はまさに lib.sh が読めなかったときのためのものなので、lib.sh には依存できない）。
bail() {
  local s=$1
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\t'/\\t}
  s=${s//$'\r'/\\r}
  s=${s//$'\n'/\\n}
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$s"
  exit 0
}

# dirname ではなくパラメータ展開で親ディレクトリを得る。dirname は外部コマンドなので、
# PATH が痩せた状態（まさにこのフックが備えたい状況）では解決できず、lib.sh を
# 見失って「ライブラリが無い」という的外れな理由で拒否してしまう。
lib="${BASH_SOURCE[0]%/*}/lib.sh"
# source の失敗を検査しないと、lib.sh が欠けたときに未定義関数が127を返し、
# それが `|| exit 0` に吸われて「無言で全許可」になる。
# shellcheck source=./lib.sh
if [ ! -r "$lib" ] || ! source "$lib"; then
  bail "フックの共通ライブラリ .claude/hooks/lib.sh を読み込めません。レビューの有無を確認できないためマージを拒否します。"
fi

# jq より先に PATH を補う（jq 自体が mise 管理下にしかないことがある）
hook_augment_path

# grep も対象に含める: hook_segment_is_merge 等が内部で使っており、無ければ
# 「マージ呼び出しではない」と誤判定して無言で全許可に転ぶ（jq/gh だけを見ていた
# ときに実際踏んだ穴と同じ形）。
for tool in jq gh grep; do
  command -v "$tool" >/dev/null 2>&1 || bail "レビューコメントの有無を確認できません: $tool コマンドが見つかりません（フックは非対話シェルで動くため ~/.bashrc は読まれません）。mise 等で $tool をインストールしてください。"
done

cmd=$(jq -r '.tool_input.command // ""')
if [ $? -ne 0 ]; then
  bail "マージ対象のコマンドを解析できませんでした（jq の実行に失敗）。レビューの有無を確認できないため拒否します。"
fi

# コマンドを断片に分割し、断片ごとに「実際のマージ呼び出しか」「PR番号は何か」を見る。
# 文字列全体を1回grepして「最初に現れた数値」を拾う方式だと、無関係な言及を拾ったり
# （例: マージ手順を説明する文字列が先に出てくる場合）、`&&` で連結された2件目以降の
# マージを見落としたりする（実際に発生した不具合）。判定・分割の詳細は lib.sh 参照。
declare -a pr_nums=()
found_merge=0
current_branch_pr=""
while IFS= read -r seg; do
  hook_segment_is_merge "$seg" || continue
  found_merge=1
  n=$(hook_segment_pr_num "$seg")
  if [ -z "$n" ]; then
    # 番号が書かれていない呼び出し（現在のブランチのPRを指す）は複数断片にまたがっても
    # 一度だけ解決すればよいのでキャッシュする
    if [ -z "$current_branch_pr" ]; then
      current_branch_pr=$(gh pr view --json number -q .number 2>/dev/null)
      if [ -z "$current_branch_pr" ]; then
        bail "マージ対象のPR番号を特定できませんでした。レビューの有無を確認できないため拒否します。番号を明示してください（例: gh pr merge 12 --squash）。"
      fi
    fi
    n=$current_branch_pr
  fi
  pr_nums+=("$n")
done <<< "$(hook_split_segments "$cmd")"

# どの断片もマージ呼び出しでなければ、このコマンドはそもそも対象外
[ "$found_merge" -eq 0 ] && exit 0

# 同じPRが複数回登場しても確認は1回でよい
declare -A seen=()
for n in "${pr_nums[@]}"; do seen[$n]=1; done

for n in "${!seen[@]}"; do
  # gh の失敗（認証切れ・ネットワーク断など）を「コメント0件」と区別する
  if ! comments=$(gh pr view "$n" --json comments -q '.comments[].body' 2>&1); then
    bail "PR #$n のレビューコメントを取得できませんでした（gh の実行に失敗）。認証やネットワークを確認してください: $comments"
  fi
  # /code-review が投稿するコメントは "### Code review" 見出しで始まる（issues found / no issues 共通）
  if ! printf '%s' "$comments" | grep -q '^### Code review'; then
    hook_deny "PR #$n にはまだ /code-review のレビューコメントが投稿されていません。マージ前に \`/code-review $n --comment\` を実行してレビューを記録してください（指摘が見つかった場合は対応してから再度レビューを通してください）。"
    exit 0
  fi
done

exit 0
