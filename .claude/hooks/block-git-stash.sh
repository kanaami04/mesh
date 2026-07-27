#!/bin/bash
# PreToolUse(Bash) hook: `git stash` を拒否する（issue #83）。
#
# **enforce-code-review.sh と同じく「確認できないときは deny」**にする。あちらは
# 「レビュー無しのマージを通さない」ため、こちらは「事故が確実に起きる操作を通さない」ため。
# remind-worktree.sh の fail-open とは方針が逆なのは、止めるコストが小さく（別の手段が
# あるうえ、それが正しい手段でもある）、通してしまったときのコストが大きいから。
#
# なぜ止めるか（.claude/skills/git-worktree/SKILL.md に経緯）:
#   作業ツリーが既にクリーンだと `git stash` は**何も保存せず**、続く `git stash pop` が
#   **スタックに元からあった無関係な stash** を展開して競合を起こす。実際に
#   `rust/src/full_checker.rs` が `UU` になる事故が2回起きている（2026-07-26 と 07-27）。
#   2回目は「スキルに禁止と事故内容まで書いてある」状態で踏んだので、文書ではなく
#   フックで機械的に止めることにした。
#
# 読み取り専用の `git stash list` / `git stash show` は通す（状態確認は必要なため）。

lib="${BASH_SOURCE[0]%/*}/lib.sh"
# shellcheck source=./lib.sh
if [ ! -r "$lib" ] || ! source "$lib"; then
  # lib が読めない = 判定できない。deny 方針なので止める（メッセージは自前で組み立てる）
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"git stash を拒否するフックの lib.sh が読めませんでした。git stash は使わず、git worktree で別の状態を用意してください。"}}\n'
  exit 0
fi

hook_augment_path
if ! command -v jq >/dev/null 2>&1 || ! command -v grep >/dev/null 2>&1; then
  hook_deny "git stash を拒否するフックが jq/grep を見つけられませんでした。git stash は使わず、git worktree で別の状態を用意してください。"
  exit 0
fi

cmd=$(jq -r '.tool_input.command // ""') || {
  hook_deny "git stash を拒否するフックがコマンドを読み取れませんでした。git stash は使わず、git worktree で別の状態を用意してください。"
  exit 0
}
hook_is_stash "$cmd" || exit 0

hook_deny "**git stash は使わないでください**（.claude/skills/git-worktree/SKILL.md）。

作業ツリーが既にクリーンだと git stash は**何も保存せず**、続く git stash pop が
**スタックに元からあった無関係な stash** を展開して競合します（実際に2回起きています）。

やりたいことが「別のコミット/ブランチの状態を見る」なら、作業ツリーを動かさずに済む
git worktree を使ってください:

  git worktree add /tmp/mesh-base <commit-or-branch>   # リポジトリの外に作る
  ...（比較する）...
  git worktree remove /tmp/mesh-base                    # **必ず消す**
  git worktree list                                     # 残っていないか確認

やりたいことが「未コミットの変更を一旦どけたい」なら、まず変更をコミットしてください
（feature ブランチ上なので、後から amend/rebase で整理できます）。

状態確認だけなら git stash list / git stash show は通ります。"
exit 0
