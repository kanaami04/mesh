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
    2) echo '### Code review skipped: docsのみの変更のため' ;;
    3) echo '### Code review skipped' ;;
    4) echo '### Code review skipped:   ' ;;
    5) echo '### Code review skipped:' ;;
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

# 「レビュー不要」を明示したコメント（理由つき）は allow する。
# 黙って飛ばすのではなく、判断と理由をPRに残させるための形式。
hook_out() { printf '%s' "$1" | sed -n '/^---OUT---$/,/^---LOG---$/p' | sed '1d;$d'; }

skip_ok=$(hook_out "$(run_with_mock_gh 'gh pr merge 2 --squash')")
[ -z "$skip_ok" ] && ok || ng "$(printf '理由つきの「レビュー不要」コメントはallowされるべき\n  出力: %s' "$skip_ok")"

# 理由が無い（見出しだけ / 空白だけ）スキップは deny する。理由が無ければ
# 「不要だと判断した記録」として意味を成さないため。前方一致で通ってしまう
# 実装だとここが素通りする（回帰防止）
for pr in 3 4 5; do
  skip_ng=$(hook_out "$(run_with_mock_gh "gh pr merge $pr --squash")")
  if [ -n "$skip_ng" ] && printf '%s' "$skip_ng" | grep -q 'skipped'; then
    ok
  else
    ng "$(printf '理由の無いスキップ(PR #%s)はdenyされるべき\n  出力: %s' "$pr" "$skip_ng")"
  fi
done

# ---------------------------------------------------------------------------
# 3-b. コメント抽出フィルタ（hook_comment_first_lines_filter）
#      **各コメントの先頭行だけ**を出すこと。全文を見ると、マーカーを説明・引用した
#      だけのコメント（コードフェンスの中に書いた等）でゲートを通ってしまう
#      （code reviewで発覚・モックghで再現確認した実際のすり抜け）。
# ---------------------------------------------------------------------------

filter=$(hook_comment_first_lines_filter)

# コードフェンスの中にマーカーを書いた「説明コメント」は、先頭行だけ見れば素通りしない
fenced='{"comments":[{"body":"この形式はこう書きます:\n\n```\n### Code review skipped: 理由\n```\n\nまだスキップしていません。"}]}'
got=$(printf '%s' "$fenced" | jq -r "$filter")
if printf '%s' "$got" | grep -qE '^### Code review'; then
  ng "$(printf 'コードフェンス内のマーカーは抽出されるべきでない\n  抽出結果: %s' "$got")"
else
  ok
fi

# 正当なコメント（見出しが先頭行）は当然拾う。複数コメントなら1行ずつ出る
legit='{"comments":[{"body":"雑談\nです"},{"body":"### Code review\n\nNo issues found."}]}'
got2=$(printf '%s' "$legit" | jq -r "$filter")
[ "$(printf '%s\n' "$got2" | grep -c '')" -eq 2 ] && ok || ng "$(printf 'コメント数ぶんの行が出るべき\n  抽出結果: %s' "$got2")"
printf '%s' "$got2" | grep -qE '^### Code review[[:space:]]*$' && ok || ng "$(printf '先頭行の見出しは拾うべき\n  抽出結果: %s' "$got2")"

# CRLF（\r 終端）が混ざっても見出し判定を邪魔しない
crlf='{"comments":[{"body":"### Code review\r\n\r\nNo issues found."}]}'
got3=$(printf '%s' "$crlf" | jq -r "$filter")
printf '%s' "$got3" | grep -qE '^### Code review[[:space:]]*$' && ok || ng "$(printf 'CRLFでも見出しを拾うべき\n  抽出結果: %s' "$got3")"

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

# ---------------------------------------------------------------------------
# 5. git worktree の注意喚起フック（remind-worktree.sh）
# ---------------------------------------------------------------------------

# $1 = 期待（match / nomatch）, $2 = 説明, $3 = コマンド文字列
check_wt() {
  local want=$1 desc=$2 cmd=$3 got
  if hook_is_worktree_add "$cmd"; then got=match; else got=nomatch; fi
  [ "$got" = "$want" ] && ok || ng "$(printf '%s\n  期待=%s 実際=%s\n  入力: %s' "$desc" "$want" "$got" "$cmd")"
}

check_wt match   '素のworktree add'      'git worktree add /tmp/base HEAD~1'
check_wt match   '先行するcd'            'cd /repo && git worktree add /tmp/base main'
check_wt match   'セミコロン区切り'      'echo start; git worktree add /tmp/base main'
check_wt match   'サブシェル'            '(git worktree add /tmp/base main)'
check_wt match   '余分な空白'            'git   worktree   add   /tmp/base'
check_wt match   'gitのグローバルオプション' 'git -C /repo worktree add /tmp/base main'
check_wt match   '環境変数の前置'        'GIT_DIR=x git worktree add /tmp/base'
check_wt match   'env 経由'              'env git worktree add /tmp/base'
# 発火してはいけない形（後片付け・確認のコマンドを止めない）
check_wt nomatch 'worktree list'         'git worktree list'
check_wt nomatch 'worktree remove'       'git worktree remove /tmp/base'
check_wt nomatch 'worktree prune'        'git worktree prune'
check_wt nomatch '文中の言及'            'echo "git worktree add をこれから使う"'
check_wt nomatch '別コマンドのadd'       'git add -A'
check_wt nomatch 'オプション値の中の言及' 'git commit -m "worktree add をやめた"'
check_wt nomatch 'PRコメント本文'        'gh pr comment 1 --body "git worktree add /tmp/x を使う"'
check_wt match   '文字列の後に本物'      'echo "git worktree add" && git worktree add /tmp/x main'

# ---------------------------------------------------------------------------
# 6. git stash の拒否フック（block-git-stash.sh、issue #83）
# ---------------------------------------------------------------------------

# $1 = 期待（match / nomatch）, $2 = 説明, $3 = コマンド文字列
check_stash() {
  local want=$1 desc=$2 cmd=$3 got
  if hook_is_stash "$cmd"; then got=match; else got=nomatch; fi
  [ "$got" = "$want" ] && ok || ng "$(printf '%s\n  期待=%s 実際=%s\n  入力: %s' "$desc" "$want" "$got" "$cmd")"
}

check_stash match   '素のstash'            'git stash'
check_stash match   'stash push'           'git stash push -m wip'
check_stash match   'stash pop'            'git stash pop'
check_stash match   'stash apply'          'git stash apply'
check_stash match   'stash save'           'git stash save wip'
check_stash match   'stash drop'           'git stash drop'
check_stash match   '先行するcd'           'cd /repo && git stash'
check_stash match   'セミコロン区切り'     'git stash; git checkout main'
check_stash match   'サブシェル'           '(git stash && git checkout main)'
check_stash match   '余分な空白'           'git   stash   pop'
check_stash match   'gitのグローバルオプション' 'git -C /repo stash'
check_stash match   'env 経由'             'env git stash'
# 発火してはいけない形（状態確認は通す・別コマンドを止めない）
check_stash nomatch 'stash list'           'git stash list'
check_stash nomatch 'stash show'           'git stash show -p'
check_stash nomatch '文中の言及'           'echo "git stash は禁止"'
check_stash nomatch '別コマンド'           'git status'
check_stash nomatch 'コミットメッセージ'   'git commit -m "git stash をやめた"'
# 引数の文字列としてコマンド名が現れる形(2026-07-27に実際に誤発火した)
check_stash nomatch 'PRコメント本文'       'gh pr comment 87 --body "状態確認なら git stash list は通ります"'
check_stash nomatch 'シングルクォート'     "echo 'git stash pop は禁止'"
check_stash nomatch 'ダブルクォート内'     'gh pr comment 1 --body "git stash push -m x"'
# ただしクォートの外に本物の呼び出しがあれば拾う
check_stash match   '文字列の後に本物'     'echo "git stash は禁止" && git stash pop'
check_wt match   'gitの-cオプション'     'git -c core.pager=cat worktree add /tmp/base'
check_wt nomatch '無関係なコマンド'      'cargo build'

# 実際にフックを起動したときの応答を確認する（ask 応答で、理由に後片付けが含まれること）
out=$(printf '{"tool_input":{"command":"git worktree add /tmp/base main"}}' | /bin/bash ./remind-worktree.sh 2>/dev/null)
if printf '%s' "$out" | jq -e '.hookSpecificOutput.permissionDecision == "ask"' >/dev/null 2>&1; then ok; else ng "$(printf 'worktree addにはask応答を返すべき\n  出力: %s' "$out")"; fi
if printf '%s' "$out" | jq -re '.hookSpecificOutput.permissionDecisionReason' 2>/dev/null | grep -q 'worktree remove'; then ok; else ng "$(printf '理由に後片付けの手順(worktree remove)を含めるべき\n  出力: %s' "$out")"; fi
# 対象外のコマンドでは何も出さない（素通り）
out=$(printf '{"tool_input":{"command":"git worktree list"}}' | /bin/bash ./remind-worktree.sh 2>/dev/null)
[ -z "$out" ] && ok || ng "$(printf 'worktree list では素通りすべき\n  出力: %s' "$out")"
# **fail-open**: 依存ツールが無くても作成を止めない（enforce-code-review.sh とは逆の方針）
emptydir=$(mktemp -d)
out=$(printf '{"tool_input":{"command":"git worktree add /tmp/base main"}}' \
  | PATH="$emptydir" HOME=/nonexistent-home /bin/bash ./remind-worktree.sh 2>/dev/null)
[ -z "$out" ] && ok || ng "$(printf 'jq等が無い環境では素通り(fail-open)すべき\n  出力: %s' "$out")"
rm -rf "$emptydir"

# ---------------------------------------------------------------------------
# 7. push 前の code review 記録フック（enforce-review-before-push.sh）
#    レビューをマージ直前ではなく push 直前に置くためのゲート。マージ側と同じく
#    **確認できないときは deny** が守られているかを見る。
# ---------------------------------------------------------------------------

# $1 = 期待（match / nomatch）, $2 = 説明, $3 = コマンド文字列
check_push() {
  local want=$1 desc=$2 cmd=$3 got
  if hook_is_push "$cmd"; then got=match; else got=nomatch; fi
  [ "$got" = "$want" ] && ok || ng "$(printf '%s\n  期待=%s 実際=%s\n  入力: %s' "$desc" "$want" "$got" "$cmd")"
}

check_push match   '素のpush'              'git push'
check_push match   '上流設定つき'          'git push -u origin feature'
check_push match   '先行するcd'            'cd /repo && git push'
check_push match   'セミコロン区切り'      'echo start; git push origin main'
check_push match   'サブシェル'            '(git push --force-with-lease)'
check_push match   'gitのグローバルオプション' 'git -c core.pager=cat push'
check_push nomatch '別コマンド'            'git status'
check_push nomatch 'pushという語の別用法'  'git log --oneline'
# 引数の文字列としてコマンド名が現れる形（block-git-stash.sh が実際に踏んだ誤発火）
check_push nomatch 'PRコメント本文'        'gh pr comment 1 --body "手順は git push です"'
check_push nomatch 'シングルクォート'      "echo 'git push は最後'"

# --- 送られるローカル ref の取り出し（hook_push_srcs）---
# $1 = 期待（空白区切り）, $2 = 説明, $3 = 断片
check_srcs() {
  local want=$1 desc=$2 seg=$3 got
  got=$(hook_push_srcs "$seg" | tr '\n' ' ')
  got=${got% }
  [ "$got" = "$want" ] && ok || ng "$(printf '%s\n  期待=[%s] 実際=[%s]\n  入力: %s' "$desc" "$want" "$got" "$seg")"
}

check_srcs 'HEAD'          '引数なしなら現在のブランチ'   'git push'
check_srcs 'HEAD'          'リモートだけならHEAD'         'git push origin'
check_srcs 'feature'       'ブランチ名'                   'git push -u origin feature'
check_srcs 'HEAD'          'refspecのローカル側'          'git push origin HEAD:refs/heads/x'
check_srcs 'old'           '強制pushの+は落とす'          'git push origin +old:refs/heads/x'
check_srcs 'a b'           '複数refspec'                  'git push origin a b'
# 断片はクォートを保ったまま渡される（refspecを落とさないため）。ref名にクォートは
# 使えないので取り除いてよい——落とし忘れると正当なpushが「解決できない」で止まる
check_srcs 'HEAD'          'クォート付きrefspec'          'git push origin "HEAD:refs/heads/x"'
# 値を次の単語として取るオプションを refspec と取り違えない
check_srcs 'feature'       '-o の値は refspec ではない'   'git push -o ci.skip origin feature'
# `--force-with-lease` は値を `=` で付ける形しか無い。次の単語を食べるとリモート名が消える
check_srcs 'feature'       'force-with-leaseは値を取らない' 'git push --force-with-lease origin feature'
# リダイレクションを refspec と取り違えない。**このフック自身の初回 push で踏んだ**
# （`git push -u origin br 2>&1 | tail -5` が「ref『2>&1』を解決できない」で止まった）。
# 単独の `&` は区切り文字にしていないので `2>&1` は断片に残る——マージ側も同じ形で
# 一度壊れており、その回帰防止テストがすぐ上の check_pr_num にある
check_srcs 'feature'       'stderrリダイレクト'           'git push -u origin feature 2>&1'
check_srcs 'feature'       '出力先の指定(空白あり)'       'git push origin feature > /dev/null'
check_srcs 'feature'       '出力先の指定(空白なし)'       'git push origin feature >out.log'
check_srcs 'feature'       '追記リダイレクト'             'git push origin feature >> log.txt'
# 何も送らない呼び出しは記録を要求しない（出力なし）
check_srcs ''              'dry-run'                      'git push --dry-run origin feature'
check_srcs ''              'リモートブランチの削除'       'git push origin --delete feature'
check_srcs ''              ':branch 形式の削除'           'git push origin :feature'
# 対象を特定できない形は降参して呼び出し元に deny させる
check_srcs '!unresolvable' '--all'                        'git push --all origin'
check_srcs '!unresolvable' '--tags'                       'git push --tags origin'
check_srcs '!unresolvable' '別リポジトリを指す -C'        'git -C /other push'
# クォートの中を && で断片分割してしまった残骸（lib.sh の既知の近似）。
# 本物のpushに見えるので deny 側に倒れる = fail-closed であることを固定する
check_srcs '!unresolvable' 'クォート断片の残骸'           "git push'"

# --- 実際にフックを起動する（使い捨てのリポジトリを作って確認）---
hooks_dir=$PWD # このスクリプトは .claude/hooks へ cd 済み
pushrepo=$(mktemp -d)
git -C "$pushrepo" init -q .
git -C "$pushrepo" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
# 既定ブランチ名は git の版・設定で変わるので、テストが依存する名前を明示的に作る
git -C "$pushrepo" checkout -q -B main
pushsha=$(git -C "$pushrepo" rev-parse HEAD)

# $1 = コマンド文字列
run_push_hook() {
  printf '{"tool_input":{"command":%s},"cwd":%s}' \
    "$(printf '%s' "$1" | jq -Rs .)" "$(printf '%s' "$pushrepo" | jq -Rs .)" \
    | env CLAUDE_PROJECT_DIR="$pushrepo" /bin/bash ./enforce-review-before-push.sh 2>/dev/null
}

# $1 = 説明, $2 = コマンド, $3 = 理由に含まれるべき文字列
expect_push_deny() {
  local desc=$1 out reason
  out=$(run_push_hook "$2")
  if [ -z "$out" ]; then
    ng "$desc: 出力なし（= 無言で allow に転んでいる）"
    return
  fi
  reason=$(printf '%s' "$out" | jq -r '.hookSpecificOutput.permissionDecisionReason' 2>/dev/null)
  if [ -z "$reason" ] || [ "$reason" = null ]; then
    ng "$desc: deny 応答が妥当な JSON でない: $out"
  elif ! printf '%s' "$reason" | grep -q "$3"; then
    ng "$desc: 理由に「$3」が含まれない: $reason"
  else
    ok
  fi
}

# $1 = 説明, $2 = コマンド
expect_push_allow() {
  local out
  out=$(run_push_hook "$2")
  [ -z "$out" ] && ok || ng "$(printf '%s: 素通りすべき\n  出力: %s' "$1" "$out")"
}

expect_push_deny  '記録が無ければ deny'   'git push -u origin main' 'code review の記録がありません'
expect_push_deny  '対象不明なら deny'     'git push --all origin'   '特定できませんでした'
expect_push_deny  '解決できないref'       'git push origin nope'    '解決できませんでした'
expect_push_allow '何も送らない dry-run'  'git push --dry-run'
expect_push_allow 'リモートブランチ削除'  'git push origin --delete feature'
expect_push_allow 'push以外のコマンド'    'git status'

# 記録を置けば通る。**中身も見る**——空の記録で通る作りだと「レビューを回した記録」
# として意味を成さない（マージ側でスキップの理由を必須にしているのと同じ考え方）
pushlog=$(hook_review_log_dir "$pushrepo")
mkdir -p "$pushlog"
printf 'note:    \n' > "$pushlog/$pushsha"
expect_push_deny  '中身が空の記録は deny' 'git push' 'code review の記録がありません'
printf 'note: 指摘なし\n' > "$pushlog/$pushsha"
expect_push_allow '記録があれば素通り'    'git push'

# 記録は HEAD の sha に紐づく。コミットを積み直したら再レビューが要求される
git -C "$pushrepo" -c user.email=t@t -c user.name=t commit -q --allow-empty -m second
expect_push_deny  'コミットを積んだら再レビュー' 'git push' 'code review の記録がありません'

# scripts/record-review.sh が書いた記録をフックが読めること（両者の場所がずれていないか。
# 定義は lib.sh の hook_review_log_dir に一本化してあるが、ずれると黙って全 deny になる）
(cd "$pushrepo" && "$hooks_dir/../../scripts/record-review.sh" 'テスト用の記録' >/dev/null 2>&1)
expect_push_allow 'record-review.sh の記録で通る' 'git push'
# --skip も理由つきなら通る／理由が無ければ記録自体を作らない
git -C "$pushrepo" -c user.email=t@t -c user.name=t commit -q --allow-empty -m third
(cd "$pushrepo" && "$hooks_dir/../../scripts/record-review.sh" --skip '' >/dev/null 2>&1)
expect_push_deny  '理由の無いスキップは記録されない' 'git push' 'code review の記録がありません'
(cd "$pushrepo" && "$hooks_dir/../../scripts/record-review.sh" --skip 'docsのみの変更のため' >/dev/null 2>&1)
expect_push_allow '理由つきスキップは通る' 'git push'

# worktree の中から push する場合、CLAUDE_PROJECT_DIR（=メインの作業ツリー）ではなく
# **cwd** を基準にすること。メイン側がレビュー済みだからといって、worktree にある
# 別のコミットを通してはいけない（fail-open の穴）
wtbase=$(mktemp -d)
if git -C "$pushrepo" worktree add -q --detach "$wtbase/wt" HEAD >/dev/null 2>&1; then
  git -C "$wtbase/wt" -c user.email=t@t -c user.name=t commit -q --allow-empty -m 'worktreeだけのコミット'
  out=$(printf '{"tool_input":{"command":"git push"},"cwd":%s}' "$(printf '%s' "$wtbase/wt" | jq -Rs .)" \
    | env CLAUDE_PROJECT_DIR="$pushrepo" /bin/bash ./enforce-review-before-push.sh 2>/dev/null)
  if [ -n "$out" ] && printf '%s' "$out" | jq -re '.hookSpecificOutput.permissionDecisionReason' 2>/dev/null | grep -q '記録がありません'; then
    ok
  else
    ng "$(printf 'worktree の未レビューのコミットは deny すべき（メイン側の記録で通ってはいけない）\n  出力: %s' "$out")"
  fi
  git -C "$pushrepo" worktree remove --force "$wtbase/wt" >/dev/null 2>&1
else
  : # worktree を作れない環境ではスキップ
fi
rm -rf "$wtbase"

rm -rf "$pushrepo"

# --- fail-closed（確認できないときは deny）---
PUSH_CMD='git push -u origin main'

# jq が無い → 無言 allow ではなく、見つからない旨を理由に deny
expect_deny 'push: jq が無い場合は deny' \
  "$(printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$PUSH_CMD" | jq -Rs .)" \
    | env PATH=/nonexistent HOME=/nonexistent-home /bin/bash ./enforce-review-before-push.sh 2>/dev/null)" 'jq'

# git だけ無い状況（gh のときと同じく、hook_augment_path で解決できてしまう環境では諦める）
tmpbin3=$(mktemp -d)
ln -s "$(command -v jq)" "$tmpbin3/jq"
ln -s "$(command -v grep)" "$tmpbin3/grep"
fakehome3=$(mktemp -d)
git_would_resolve() (
  PATH="$tmpbin3"
  HOME="$fakehome3"
  hook_augment_path
  command -v git >/dev/null 2>&1
)
if git_would_resolve; then
  : # このマシンでは git が hook_augment_path 経由で見つかるためスキップ
else
  expect_deny 'push: git が無い場合は deny' \
    "$(printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$PUSH_CMD" | jq -Rs .)" \
      | env PATH="$tmpbin3" HOME="$fakehome3" /bin/bash ./enforce-review-before-push.sh 2>/dev/null)" 'git'
fi
rm -rf "$tmpbin3" "$fakehome3"

# 不正なJSON入力 → 空コマンド扱いで無言 allow するのではなく deny
expect_deny 'push: jq解析失敗なら deny' \
  "$(printf 'これはJSONではない' | bash ./enforce-review-before-push.sh 2>/dev/null)" 'jq'

# lib.sh を読めない場所にコピーして実行 → 無言 allow ではなく deny
tmp3=$(mktemp -d)
cp ./enforce-review-before-push.sh "$tmp3/"
expect_deny 'push: lib.sh が無い場合は deny' \
  "$(printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$PUSH_CMD" | jq -Rs .)" | bash "$tmp3/enforce-review-before-push.sh" 2>/dev/null)" \
  'lib.sh'
rm -rf "$tmp3"

# git リポジトリの外で実行 → 記録の場所が分からないので deny
outside=$(mktemp -d)
expect_deny 'push: リポジトリ外なら deny' \
  "$(printf '{"tool_input":{"command":%s},"cwd":%s}' "$(printf '%s' "$PUSH_CMD" | jq -Rs .)" "$(printf '%s' "$outside" | jq -Rs .)" \
    | env -u CLAUDE_PROJECT_DIR /bin/bash ./enforce-review-before-push.sh 2>/dev/null)" \
  'リポジトリのルート'
rm -rf "$outside"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
