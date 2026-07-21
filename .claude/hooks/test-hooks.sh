#!/bin/bash
# フックのテスト。ネットワークにも gh 認証にも依存しない。
# 実行: .claude/hooks/test-hooks.sh （CI の test ジョブからも実行される）
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"
source ./lib.sh

pass=0
fail=0

ok()   { pass=$((pass + 1)); }
ng()   { fail=$((fail + 1)); printf 'FAIL: %s\n' "$1"; }

# ---------------------------------------------------------------------------
# 1. コマンド判定（hook_is_pr_merge）
# ---------------------------------------------------------------------------

# $1 = 期待（match / nomatch）, $2 = 説明, $3 = コマンド文字列
check_match() {
  local want=$1 desc=$2 cmd=$3 got
  if hook_is_pr_merge "$cmd"; then got=match; else got=nomatch; fi
  [ "$got" = "$want" ] && ok || ng "$(printf '%s\n  期待=%s 実際=%s\n  入力: %s' "$desc" "$want" "$got" "$cmd")"
}

# 実際のマージ呼び出しは検出する
check_match match   '素のマージ'             'gh pr merge 1 --squash'
check_match match   '先行するcd'             'cd /repo && gh pr merge 1 --squash --delete-branch'
check_match match   'セミコロン区切り'       'echo start; gh pr merge 2 --squash'
check_match match   'パイプの後ろ'           'true | gh pr merge 3 --squash'
check_match match   'サブシェル'             '(gh pr merge 4 --squash)'
check_match match   '余分な空白'             'gh   pr   merge   5 --squash'
# 以下はアンカー導入時に一度取りこぼしていた形（回帰防止）
check_match match   '環境変数の前置'         'GH_TOKEN=xxx gh pr merge 6 --squash'
check_match match   '環境変数2つ'            'A=1 B=2 gh pr merge 7 --squash'
check_match match   'env 経由'               'env gh pr merge 8 --squash'
check_match match   'time 経由'              'time gh pr merge 9 --squash'
check_match match   'nohup 経由'             'nohup gh pr merge 10 --squash'
check_match match   'sudo 経由'              'sudo gh pr merge 11 --squash'

# 文章中の言及は検出しない（誤検知の回帰防止）
check_match nomatch 'バッククォート内の言及' 'gh api repos/o/r/pulls/1/comments -f body="`gh pr merge` が deny されます"'
check_match nomatch '文中の言及'             'echo "マージ手順は gh pr merge --squash です"'
check_match nomatch '別コマンドの引数'       'grep -rn "gh pr merge" docs/'
check_match nomatch '無関係なコマンド'       'git status'
check_match nomatch 'merge違い'              'git merge main'
check_match nomatch 'pr merge以外のgh'       'gh pr view 1 --json comments'

# ---------------------------------------------------------------------------
# 2. enforce-code-review.sh の fail-closed 挙動
#    「確認できないときは deny」が守られているかを見る。ここが無言で allow に
#    転ぶと、フックが効いていないことに誰も気づけない。
# ---------------------------------------------------------------------------

MERGE_CMD='gh pr merge 1 --squash'

# $1 = 差し替える PATH, $2 = 差し替える HOME（省略時は現在の HOME）
# bash は絶対パスで起動する。PATH を差し替えた状態で `bash` と書くと、
# シェルではなく bash 自体が見つからずに落ちて「出力なし」になり、
# フックが allow したのか起動できなかったのか区別がつかなくなる。
run_review_hook() {
  printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$MERGE_CMD" | jq -Rs .)" \
    | env PATH="$1" HOME="${2:-$HOME}" /bin/bash ./enforce-code-review.sh 2>/dev/null
}

# $1 = 説明, $2 = 出力, $3 = 理由に含まれるべき文字列
expect_deny() {
  local desc=$1 out=$2 want=$3 reason
  if [ -z "$out" ]; then
    ng "$desc: 出力なし（= 無言で allow に転んでいる）"
    return
  fi
  reason=$(printf '%s' "$out" | jq -r '.hookSpecificOutput.permissionDecisionReason' 2>/dev/null)
  if [ -z "$reason" ] || [ "$reason" = null ]; then
    ng "$desc: deny 応答が妥当な JSON でない: $out"
  elif ! printf '%s' "$reason" | grep -q "$want"; then
    ng "$desc: 理由に「$want」が含まれない: $reason"
  else
    ok
  fi
}

# jq / gh が見つからない → 「レビュー未投稿」ではなく、見つからない旨を理由にする
expect_deny 'jq が無い場合は deny' "$(run_review_hook /nonexistent)" 'jq'

# gh だけ無い状況を作る。PATH から実在ディレクトリを外すだけだと、CI ランナーのように
# gh が最初から入っている環境で再現できないため、jq だけを置いた一時ディレクトリを使う。
# HOME も存在しないパスにして hook_augment_path が gh を拾い直さないようにする。
tmpbin=$(mktemp -d)
ln -s "$(command -v jq)" "$tmpbin/jq"
if command -v gh >/dev/null 2>&1 && [ -x "/usr/local/bin/gh" ]; then
  : # /usr/local/bin は hook_augment_path が拾うため、この環境ではケースを飛ばす
else
  expect_deny 'gh が無い場合は deny' "$(run_review_hook "$tmpbin" /nonexistent-home)" 'gh'
fi
rm -rf "$tmpbin"

# lib.sh を読めない場所にコピーして実行 → 無言 allow ではなく deny
tmp=$(mktemp -d)
cp ./enforce-code-review.sh "$tmp/"
expect_deny 'lib.sh が無い場合は deny' \
  "$(printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$MERGE_CMD" | jq -Rs .)" | bash "$tmp/enforce-code-review.sh" 2>/dev/null)" \
  'lib.sh'
rm -rf "$tmp"

# マージ以外のコマンドは、ツールが揃っていれば素通りする
out=$(printf '{"tool_input":{"command":"git status"}}' | bash ./enforce-code-review.sh 2>/dev/null)
[ -z "$out" ] && ok || ng "マージ以外のコマンドは素通りすべき: $out"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
