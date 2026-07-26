#!/bin/bash
# PreToolUse(Bash) hook: `git worktree add` を見つけたら、後片付けまで含む手順
# (.claude/skills/git-worktree/SKILL.md)を示して確認を求める。
#
# **enforce-code-review.sh とは設計方針が逆**: あちらは「確認できないときは deny」だが、
# こちらは**確認できないときは素通り(allow委譲)**にする。目的が「消し忘れの防止」という
# 注意喚起であり、フックが壊れたときに worktree の作成そのものを止めると、
# 失敗のコストの方が守りたいものより大きくなるため。
#
# 判定は lib.sh の hook_is_worktree_add(断片分割 + 行頭アンカー)に委ねる。
# `git worktree list` / `remove` / `prune` や、文字列中の言及では発火しない。

lib="${BASH_SOURCE[0]%/*}/lib.sh"
# shellcheck source=./lib.sh
if [ ! -r "$lib" ] || ! source "$lib"; then
  exit 0 # 素通り(上記の方針どおり fail-open)
fi

hook_augment_path
command -v jq >/dev/null 2>&1 || exit 0
command -v grep >/dev/null 2>&1 || exit 0

cmd=$(jq -r '.tool_input.command // ""') || exit 0
hook_is_worktree_add "$cmd" || exit 0

hook_ask "git worktree を作ろうとしています。**作ったら必ず消す**ところまでが手順です(.claude/skills/git-worktree/SKILL.md)。
- リポジトリの外(例: /tmp/...)に作る
- 比較用途なら detached HEAD で作る(ブランチを増やさない)
- 用が済んだら 'git worktree remove <path>'、最後に 'git worktree list' で残っていないことを確認する
- rm -rf で消さない(管理情報が残る。消してしまったら 'git worktree prune')
なお、変更前後の比較に 'git stash' を使うのは禁止です(空振りして無関係な stash を pop する事故が実際に起きました)。"
exit 0
