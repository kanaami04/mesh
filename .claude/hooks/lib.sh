#!/bin/bash
# enforce-code-review.sh / enforce-squash-merge.sh が共有するヘルパー。
# 両者が同じ判定・同じ環境問題を抱えていたため、1箇所にまとめている。

# フックは非対話シェルで起動されるため ~/.bashrc が読まれず、mise や
# ユーザーローカルに入れたツール（gh など）が PATH に無いことがある。
# 見つからないまま実行すると「コマンドが無い」→「該当なし」と誤判定するので、
# 代表的なインストール先を PATH に補っておく。
hook_augment_path() {
  local dir
  for dir in \
    "$HOME/.local/share/mise/shims" \
    "$HOME/.local/bin" \
    "$HOME/.bun/bin" \
    "$HOME/.cargo/bin" \
    /opt/homebrew/bin \
    /usr/local/bin
  do
    [ -d "$dir" ] || continue
    case ":$PATH:" in
      *":$dir:"*) ;;
      *) PATH="$PATH:$dir" ;;
    esac
  done
  export PATH
}

# コマンド文字列に `gh pr merge` の *実際の呼び出し* が含まれるかを判定する。
#
# 単純に全体を grep すると、マージ手順を説明する文章（PRコメント・ドキュメント・
# echo する文字列など）に含まれる `gh pr merge` にも誤マッチし、マージと無関係な
# コマンドまで deny してしまう（実際に踏んだ）。
# そこでコマンドの先頭 — 行頭、または `;` `&&` `||` `|` `(` の直後 — にアンカーする。
#
# 既知の限界: ヒアドキュメントや複数行文字列の *行頭* に `gh pr merge` が現れる
# ケースはまだ誤マッチする。シェルの構文解析なしに完全な判別はできないため、
# 「本文をファイル経由で渡す」等で回避する。
hook_is_pr_merge() {
  printf '%s' "$1" | grep -Eq '(^|[;&|(]|&&|\|\|)[[:space:]]*(sudo[[:space:]]+)?gh[[:space:]]+pr[[:space:]]+merge\b'
}

# PreToolUse フックの deny 応答を出力する。$1 = 理由（ユーザーに表示される）。
hook_deny() {
  local reason
  # JSON 文字列として安全になるようエスケープする（" と \ とタブ・改行）
  reason=$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g' | awk 'BEGIN{ORS=""} {print sep $0; sep="\\n"}')
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$reason"
}
