#!/bin/bash
# enforce-code-review.sh が使うヘルパー。
#
# 設計方針: フックが担うのは「GitHub 側で表現できないローカルな約束」だけにする。
# 例えば「マージは squash に統一」はリポジトリ設定（merge commit と rebase を無効化）で
# サーバ側が強制するので、フックでは扱わない — Web UI からのマージにも効き、
# 環境にも依存せず、すり抜けようがないため。

# フックは非対話シェルで起動されるため ~/.bashrc が読まれず、mise などユーザーごとに
# 入れたツール（gh / jq）が PATH に無いことがある。代表的なインストール先を補う。
# 「補う」目的なので先頭に足す — 末尾だと古い gh が先に見つかって勝ってしまう。
hook_augment_path() {
  local dir
  for dir in \
    "$HOME/.local/share/mise/shims" \
    "$HOME/.local/bin" \
    "$HOME/.asdf/shims" \
    "$HOME/.nix-profile/bin" \
    "$HOME/.bun/bin" \
    "$HOME/.cargo/bin" \
    /home/linuxbrew/.linuxbrew/bin \
    /opt/homebrew/bin \
    /usr/local/bin
  do
    [ -d "$dir" ] || continue
    case ":$PATH:" in
      *":$dir:"*) ;;
      *) PATH="$dir:$PATH" ;;
    esac
  done
  export PATH
}

# コマンド文字列に `gh pr merge` の *実際の呼び出し* が含まれるかを判定する。
#
# 単純に全体を grep すると、マージ手順を説明する文章（PRコメント・ドキュメント・
# echo する文字列など）に誤マッチして無関係なコマンドまで deny してしまう。
# 一方で先頭に厳しくアンカーすると `FOO=1 gh pr merge` のような実際の呼び出しを
# 取りこぼす。そこで「コマンドの先頭（行頭 / ; && || | ( の直後）＋ 環境変数代入や
# env・sudo などのラッパーを任意個」まで許す形にしている。
#
# 既知の限界: ヒアドキュメントや複数行文字列の *行頭* に `gh pr merge` が現れる
# ケースはまだ誤マッチする。シェルの構文解析なしに完全な判別はできないため、
# 誤マッチ側（＝安全側に倒れて deny する）に寄せた上で限界を明示しておく。
hook_is_pr_merge() {
  printf '%s' "$1" | grep -Eq \
    '(^|[;&|(]|&&|\|\|)[[:space:]]*([A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+|(sudo|env|time|nohup|command|xargs)[[:space:]]+)*gh[[:space:]]+pr[[:space:]]+merge\b'
}

# PreToolUse フックの deny 応答を出力する。$1 = 理由（ユーザーに表示される）。
hook_deny() {
  local reason
  # JSON 文字列として安全になるようエスケープする（\ と " と改行）
  reason=$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g' | awk 'BEGIN{ORS=""} {print sep $0; sep="\\n"}')
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$reason"
}
