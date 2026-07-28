#!/bin/bash
# PreToolUse(Bash) hook: `git push` の前に、これから送るコミットに対する code review の
# 記録（.claude/review-log/<sha>）があるかを確認する。無ければ deny して理由を返す。
# それ以外のコマンドは素通り（exit 0 + 出力なし = allow 判定に委ねる）。
#
# **なぜ push 前なのか**: レビューをマージ直前に置くと、指摘が出るのは PR を公開して
# CI を回した後になる。レビューを push の手前に移すと、直す前に PR に載ることが無くなる。
# マージ側のゲート（enforce-code-review.sh）は残してあり、そちらは「push 前に回した
# レビューの結果が PR に記録されたか」を見る（記録はレビュー1回ぶん、二度は回さない）。
#
# 原則は enforce-code-review.sh と同じで **確認できないときは deny する**。
# レビュー済みかどうかを確かめられないのに素通りさせると、フックが効いていないことに
# 誰も気づけない。
#
# **既知の限界**（マージ側と同じ形の穴。fail-open なので把握しておく）:
# - このフックは .claude/settings.json の `if: "Bash(git push*)"` で前段の絞り込みを
#   受けてから起動する。ハーネス側の判定が拾わない形（`sudo git push` など）では
#   そもそも起動しない。**前段の絞り込みは外さない**——外すと jq が無い環境で
#   *すべての* Bash コマンドが deny されてしまい、被害が push の範囲を超える
# - `/usr/bin/git push` のようにフルパスで呼ぶと、行頭アンカーの判定に一致しない
#
# **記録は自己申告**（Claude がレビューを回してから scripts/record-review.sh を実行する）。
# GitHub 上のコメントを見るマージ側と違って外形的な証拠は無い——が、push 前には PR が
# 存在せず、ローカルの `/code-review` は結果を画面に返すだけでどこにも痕跡を残さない
# ため、記録を残す以外に確認する手立てがない。

# deny 応答を出す最小限の実装（lib.sh を読む前に失敗しても理由を返せるようにする）。
# JSON エスケープのロジックを lib.sh から複製している理由は enforce-code-review.sh 参照。
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

lib="${BASH_SOURCE[0]%/*}/lib.sh"
# shellcheck source=./lib.sh
if [ ! -r "$lib" ] || ! source "$lib"; then
  bail "フックの共通ライブラリ .claude/hooks/lib.sh を読み込めません。レビューの有無を確認できないため push を拒否します。"
fi

hook_augment_path

# grep も対象に含める理由は enforce-code-review.sh と同じ（判定関数が内部で使っており、
# 無ければ「push ではない」と誤判定して無言で全許可に転ぶ）
for tool in jq git grep; do
  command -v "$tool" >/dev/null 2>&1 || bail "code review の記録を確認できません: $tool コマンドが見つかりません（フックは非対話シェルで動くため ~/.bashrc は読まれません）。mise 等で $tool をインストールしてください。"
done

input=$(cat)
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
if [ $? -ne 0 ]; then
  bail "push しようとしているコマンドを解析できませんでした（jq の実行に失敗）。レビューの有無を確認できないため拒否します。"
fi

# 送られる ref を集める。判定（is_push）はクォートを落とした断片で行い、引数の解析は
# **落としていない断片**で行う——落とすと refspec が消えて「引数なし = HEAD」に化ける
declare -a srcs=()
found_push=0
while IFS= read -r seg; do
  hook_segment_is_push "$(hook_strip_quoted "$seg")" || continue
  found_push=1
  while IFS= read -r src; do
    [ -n "$src" ] && srcs+=("$src")
  done <<< "$(hook_push_srcs "$seg")"
done <<< "$(hook_split_segments "$cmd")"

# push 呼び出しが無ければ対象外
[ "$found_push" -eq 0 ] && exit 0
# 何も送らない push（--dry-run / --delete / `:branch`）はレビューの記録を要求しない
[ "${#srcs[@]}" -eq 0 ] && exit 0

# 作業対象のリポジトリ。**cwd を先に見る**——`git worktree` の中から push する場合、
# CLAUDE_PROJECT_DIR（=メインの作業ツリー）を基準にすると別のコミットの記録を見て
# しまい、メイン側がレビュー済みなら worktree の未レビューのコミットが通ってしまう。
# cwd 基準なら worktree のルートに記録が無いので deny 側に倒れる。
# cwd がリポジトリ外のときだけ CLAUDE_PROJECT_DIR にフォールバックする
# （どちらも「コマンドが実際に走るディレクトリ」の近似でしかない——`cd` を含む
# コマンドまでは追えないが、その場合も記録が見つからず deny 側に倒れる）
root=""
for candidate in "$(printf '%s' "$input" | jq -r '.cwd // ""')" "${CLAUDE_PROJECT_DIR:-}"; do
  [ -n "$candidate" ] || continue
  root=$(git -C "$candidate" rev-parse --show-toplevel 2>/dev/null) && [ -n "$root" ] && break
  root=""
done
if [ -z "$root" ]; then
  bail "リポジトリのルートを特定できませんでした。code review の記録を確認できないため push を拒否します。"
fi
log_dir=$(hook_review_log_dir "$root")

# ref を commit の sha に解決する。同じ sha が複数回出てきても確認は1回でよい
declare -A shas=()
for src in "${srcs[@]}"; do
  if [ "$src" = '!unresolvable' ]; then
    hook_deny "この push が送るコミットを特定できませんでした（--all / --mirror / --tags や別リポジトリを指す -C など）。code review の記録を確認できないため拒否します。送るブランチを明示してください（例: git push -u origin <branch>）。"
    exit 0
  fi
  sha=$(git -C "$root" rev-parse --verify --quiet "${src}^{commit}" 2>/dev/null)
  if [ -z "$sha" ]; then
    hook_deny "push しようとしている ref「$src」をコミットに解決できませんでした。code review の記録を確認できないため拒否します。"
    exit 0
  fi
  shas[$sha]=$src
done

for sha in "${!shas[@]}"; do
  record="$log_dir/$sha"
  # 記録の有無だけでなく中身も見る: 空ファイルを置けば通る作りだと、
  # 「レビューを回した」という記録として意味を成さない（マージ側でスキップの
  # 理由を必須にしているのと同じ考え方）
  if [ -f "$record" ] && grep -qE '^note:[[:space:]]*[^[:space:]]' "$record"; then
    continue
  fi
  hook_deny "$(printf 'push しようとしているコミット %s には code review の記録がありません（%s の ref「%s」）。

push の前にレビューを回してください:
  1. ローカルの差分をレビューする。**PR がまだ無いのでプラグイン版（`/code-review <番号> --comment`）は使えません**。ユーザーに組み込みの `/code-review` の実行を依頼するか、レビュー用サブエージェント（範囲・観点の縛りは CLAUDE.md 参照）を使うか、自分で検証できる規模なら自分で回す
  2. 指摘があれば直してコミットし直す（記録は送るコミットの sha に紐づくので、直したら 1 からやり直し）
  3. `scripts/record-review.sh "<レビュー結果の要約>"` で記録を残す

レビューが不要だと判断した場合は、黙って飛ばさず理由を記録してください:
  scripts/record-review.sh --skip "<理由>"' "${sha:0:12}" "$log_dir" "${shas[$sha]}")"
  exit 0
done

exit 0
