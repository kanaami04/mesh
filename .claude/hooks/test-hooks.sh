#!/bin/bash
# フックの判定ロジックのテスト。ネットワークや gh 認証には依存しない。
# 実行: .claude/hooks/test-hooks.sh
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"
source ./lib.sh

pass=0
fail=0

# $1 = 期待（match / nomatch）, $2 = 説明, $3 = コマンド文字列
check_match() {
  local want=$1 desc=$2 cmd=$3 got
  if hook_is_pr_merge "$cmd"; then got=match; else got=nomatch; fi
  if [ "$got" = "$want" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAIL: %s\n  期待=%s 実際=%s\n  入力: %s\n' "$desc" "$want" "$got" "$cmd"
  fi
}

# --- 実際のマージ呼び出しは検出する ---
check_match match   '素のマージ'           'gh pr merge 1 --squash'
check_match match   '先行するcd'           'cd /repo && gh pr merge 1 --squash --delete-branch'
check_match match   'セミコロン区切り'     'echo start; gh pr merge 2 --squash'
check_match match   'パイプの後ろ'         'true | gh pr merge 3 --squash'
check_match match   'サブシェル'           '(gh pr merge 4 --squash)'
check_match match   '余分な空白'           'gh   pr   merge   5 --squash'

# --- 文章中の言及は検出しない（今回直した誤検知） ---
check_match nomatch 'バッククォート内の言及' 'gh api repos/o/r/pulls/1/comments -f body="`gh pr merge` が deny されます"'
check_match nomatch '文中の言及'             'echo "マージ手順は gh pr merge --squash です"'
check_match nomatch '別コマンドの引数'       'grep -rn "gh pr merge" docs/'
check_match nomatch '無関係なコマンド'       'git status'
check_match nomatch 'merge違い'              'git merge main'
check_match nomatch 'pr merge以外のgh'       'gh pr view 1 --json comments'

# --- squashフックの応答 ---
run_squash_hook() {
  printf '{"tool_input":{"command":%s}}' "$(printf '%s' "$1" | jq -Rs .)" | ./enforce-squash-merge.sh
}

out=$(run_squash_hook 'gh pr merge 1 --squash')
if [ -z "$out" ]; then pass=$((pass + 1)); else
  fail=$((fail + 1)); printf 'FAIL: --squash 付きは素通りすべき\n  実際: %s\n' "$out"
fi

out=$(run_squash_hook 'gh pr merge 1')
if printf '%s' "$out" | grep -q '"permissionDecision":"deny"'; then pass=$((pass + 1)); else
  fail=$((fail + 1)); printf 'FAIL: --squash 無しは deny すべき\n  実際: %s\n' "$out"
fi

# deny の理由文には `gh pr merge` という語句が含まれる。これ自体が誤検知の再発を
# 招かないこと、および JSON として妥当であることを確認する。
out=$(run_squash_hook 'gh pr merge 1')
if printf '%s' "$out" | jq -e '.hookSpecificOutput.permissionDecisionReason' >/dev/null 2>&1; then
  pass=$((pass + 1))
else
  fail=$((fail + 1)); printf 'FAIL: deny 応答が妥当な JSON でない\n  実際: %s\n' "$out"
fi

out=$(run_squash_hook 'echo "説明: マージは gh pr merge --squash で行います"')
if [ -z "$out" ]; then pass=$((pass + 1)); else
  fail=$((fail + 1)); printf 'FAIL: 文章中の言及を deny してはいけない\n  実際: %s\n' "$out"
fi

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
