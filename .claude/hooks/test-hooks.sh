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
# 1. コマンド判定（hook_is_pr_merge = 断片分割 + hook_segment_is_merge）
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
# 以下は断片分割方式にする前は取りこぼしていた形（回帰防止。code reviewで発覚）
check_match match   'if/thenの内側'          'if true; then gh pr merge 12 --squash; fi'
check_match match   'for/doの内側'           'for i in 1; do gh pr merge 13 --squash; done'
check_match match   '波括弧の内側'           '{ gh pr merge 14 --squash; }'
# stderrリダイレクト(`2>&1`)を含む実コマンド。単独&を区切り文字にしていた版では
# 断片が `2>` で断ち切られ、PR番号抽出が壊れていた(このPR自身のマージ実行で発覚)
check_match match   'stderrリダイレクト付き'  'gh pr merge 15 --squash 2>&1 | tail -5'

# 文章中の言及は検出しない（誤検知の回帰防止）
check_match nomatch 'バッククォート内の言及' 'gh api repos/o/r/pulls/1/comments -f body="`gh pr merge` が deny されます"'
check_match nomatch '文中の言及'             'echo "マージ手順は gh pr merge --squash です"'
check_match nomatch '別コマンドの引数'       'grep -rn "gh pr merge" docs/'
check_match nomatch '無関係なコマンド'       'git status'
check_match nomatch 'merge違い'              'git merge main'
check_match nomatch 'pr merge以外のgh'       'gh pr view 1 --json comments'

# ---------------------------------------------------------------------------
# 2. PR番号の抽出（hook_segment_pr_num）
#    文字列全体を1回grepするのではなく断片ごとに見ることで、無関係な言及や
#    2件目以降のマージを取りこぼさないことを確認する（code reviewで発覚した不具合）。
# ---------------------------------------------------------------------------

# $1 = 期待するPR番号, $2 = 説明, $3 = 断片文字列
check_pr_num() {
  local want=$1 desc=$2 seg=$3 got
  got=$(hook_segment_pr_num "$seg")
  [ "$got" = "$want" ] && ok || ng "$(printf '%s\n  期待=%s 実際=%s\n  断片: %s' "$desc" "$want" "$got" "$seg")"
}

check_pr_num 1 '素の断片'                 'gh pr merge 1 --squash'
check_pr_num 1 'ラッパー付きの断片'       'sudo gh pr merge 1 --squash'
check_pr_num '' '番号無しの断片'          'gh pr merge --squash'
# 以下2件は実際に自分自身のマージ実行(`gh pr merge 3 ... 2>&1 | tail -5`)で踏んだ
# 回帰。単独&での分割と、grep -m1(先頭1行)をhead -1(先頭1件)の代用にしていたのが
# 原因だった(-oは1行内の複数マッチを別々の行に出すため、2>&1がリダイレクトの
# 一部として残った断片では「2」も一緒に拾ってしまっていた)
check_pr_num 3 'stderrリダイレクトを含む断片' 'gh pr merge 3 --squash --delete-branch 2>&1 '
check_pr_num 5 '同一行に複数の数字がある断片' 'gh pr merge 5 --repo owner/repo2'

# ---------------------------------------------------------------------------
# 3. 複数マージ・すり抜けの回帰防止（実際に gh を叩かず、モックで挙動を確認する）
# ---------------------------------------------------------------------------

# $1 = 期待するPR番号のリスト（スペース区切り、gh pr view に渡された順不同で比較）
# $2 = コマンド文字列
run_with_mock_gh() {
  local cmd=$1 mockdir log out
  mockdir=$(mktemp -d)
  log="$mockdir/calls.log"
  cat > "$mockdir/gh" <<EOF
#!/bin/bash
echo "\$@" >> "$log"
if [ "\$1" = "pr" ] && [ "\$2" = "view" ]; then
  case "\$3" in
    1) echo '### Code review'; echo '(no issues)' ;;
    999999) echo 'これはレビューコメントではない' ;;
    *) exit 1 ;;
  esac
  exit 0
fi
exit 1
EOF
  chmod +x "$mockdir/gh"
  ln -s "$(command -v jq)" "$mockdir/jq"
  ln -s "$(command -v grep)" "$mockdir/grep"
  out=$(printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$cmd" | jq -Rs .)" \
    | PATH="$mockdir" HOME=/nonexistent-home /bin/bash ./enforce-code-review.sh 2>/dev/null)
  echo "---OUT---"
  echo "$out"
  echo "---LOG---"
  cat "$log" 2>/dev/null
  rm -rf "$mockdir"
}

# PR #1 はレビュー済み、PR #999999 は未レビュー。両方チェックされるなら、
# 未レビューの #999999 を理由に deny されるはず（#1 だけ見て allow してはいけない）。
result=$(run_with_mock_gh 'gh pr merge 1 --squash && gh pr merge 999999 --squash')
out=$(printf '%s' "$result" | sed -n '/^---OUT---$/,/^---LOG---$/p' | sed '1d;$d')
log=$(printf '%s' "$result" | sed -n '/^---LOG---$/,$p' | sed '1d')

# deny理由に $n (PR番号) が載るのは、実際に `gh pr view "$n"` を呼んでレビュー無しと
# 判定した場合だけ(hook_deny呼び出し元を参照)。よってこの一致は「2件目のマージが
# 黙って読み飛ばされていない」ことの直接的な証拠になる。
# 連想配列のキー列挙順は不定なので、PR #1 が先にチェックされ得ること自体は問わない
# (どちらが先でも、レビュー無しのPRがあれば最終的にdenyになるのが正しい)。
if printf '%s' "$out" | grep -q '999999'; then
  ok
else
  ng "$(printf '複数マージ: 未レビューの2件目(#999999)が読み飛ばされずdenyの理由に含まれるべき\n  出力: %s' "$out")"
fi

if printf '%s' "$log" | grep -q '^pr view 999999 '; then
  ok
else
  ng "$(printf '複数マージ: PR #999999 が実際に gh pr view でチェックされるべき\n  ログ: %s' "$log")"
fi

# 逆にレビュー済みの単独PRなら allow（出力なし）になることも確認する
result2=$(run_with_mock_gh 'gh pr merge 1 --squash')
out2=$(printf '%s' "$result2" | sed -n '/^---OUT---$/,/^---LOG---$/p' | sed '1d;$d')
[ -z "$out2" ] && ok || ng "$(printf 'レビュー済み単独PRはallowされるべき\n  出力: %s' "$out2")"

# ---------------------------------------------------------------------------
# 4. enforce-code-review.sh の fail-closed 挙動
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

# jq が見つからない → 「レビュー未投稿」ではなく、見つからない旨を理由にする
expect_deny 'jq が無い場合は deny' "$(run_review_hook /nonexistent)" 'jq'

# gh だけ無い状況を作る。ハードコードした場所を1つ覗くだけでは hook_augment_path の
# 一覧とずれうるので、実際に hook_augment_path を通した上で gh が解決できるかを
# その場で判定する（この判定自体が hook_augment_path の実装とずれることはない）。
tmpbin=$(mktemp -d)
ln -s "$(command -v jq)" "$tmpbin/jq"
fakehome=$(mktemp -d)
gh_would_resolve() (
  PATH="$tmpbin"
  HOME="$fakehome"
  hook_augment_path
  command -v gh >/dev/null 2>&1
)
if gh_would_resolve; then
  : # このマシンでは gh が hook_augment_path 経由で見つかってしまい、
    # 「gh が無い」状況を作れないためスキップする
else
  expect_deny 'gh が無い場合は deny' "$(run_review_hook "$tmpbin" "$fakehome")" 'gh'
fi
rm -rf "$tmpbin" "$fakehome"

# grep が無い状況（jq/gh/bash だけを置く）→ 無言 allow ではなく deny すべき
tmpbin2=$(mktemp -d)
ln -s "$(command -v jq)" "$tmpbin2/jq"
ln -s "$(command -v gh)" "$tmpbin2/gh"
out=$(printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$MERGE_CMD" | jq -Rs .)" \
  | PATH="$tmpbin2" HOME=/nonexistent-home /bin/bash ./enforce-code-review.sh 2>/dev/null)
if [ -z "$out" ]; then
  ng "grep が無い場合は deny すべきだが出力なし（= 無言で allow）"
else
  reason=$(printf '%s' "$out" | jq -r '.hookSpecificOutput.permissionDecisionReason' 2>/dev/null)
  if printf '%s' "$reason" | grep -q 'grep'; then ok; else ng "grep が無い場合の理由に「grep」が含まれない: $reason"; fi
fi
rm -rf "$tmpbin2"

# jq の解析失敗（不正なJSON入力）→ 空コマンドとして無言 allow するのではなく deny すべき
out=$(printf 'これはJSONではない' | bash ./enforce-code-review.sh 2>/dev/null)
if [ -z "$out" ]; then
  ng "jqの解析に失敗した場合は deny すべきだが出力なし（= 無言で allow）"
else
  reason=$(printf '%s' "$out" | jq -r '.hookSpecificOutput.permissionDecisionReason' 2>/dev/null)
  if printf '%s' "$reason" | grep -q 'jq'; then ok; else ng "jq解析失敗時の理由に「jq」が含まれない: $reason"; fi
fi

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

# ---------------------------------------------------------------------------
# 5. JSON エスケープ（bail / hook_deny）
#    gh のエラーメッセージに引用符が含まれても、応答が壊れたJSONにならないこと。
# ---------------------------------------------------------------------------

quote_json_escape_test() {
  local reason
  reason=$(hook_json_escape 'error: dial tcp: lookup "api.github.com": no such host')
  printf '{"x":"%s"}' "$reason" | jq -e . >/dev/null 2>&1
}
if quote_json_escape_test; then ok; else ng 'hook_json_escape は引用符を含む文字列を妥当なJSONにエスケープすべき'; fi

# bail() 経由でも同様に確認する（gh がクォート付きエラーを返すケースの再現）
mockdir=$(mktemp -d)
cat > "$mockdir/gh" <<'EOF'
#!/bin/bash
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
  echo 'error: dial tcp: lookup "api.github.com": no such host' >&2
  exit 1
fi
exit 1
EOF
chmod +x "$mockdir/gh"
ln -s "$(command -v jq)" "$mockdir/jq"
ln -s "$(command -v grep)" "$mockdir/grep"
out=$(printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$MERGE_CMD" | jq -Rs .)" \
  | PATH="$mockdir" HOME=/nonexistent-home /bin/bash ./enforce-code-review.sh 2>/dev/null)
if [ -n "$out" ] && printf '%s' "$out" | jq -e . >/dev/null 2>&1; then
  ok
else
  ng "$(printf 'ghのエラーに引用符が含まれる場合もdeny応答は妥当なJSONであるべき\n  出力: %s' "$out")"
fi
rm -rf "$mockdir"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
