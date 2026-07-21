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

# コマンド文字列を、実際のマージ呼び出しが区切られうる位置（; | ( ) { } および
# && / ||）で断片に分割する。1本の文字列全体を一括で grep するのではなく、断片ごとに
# 「これは merge 呼び出しか」を判定することで、次の2つの不具合を同時に解消している:
#
# 1. `if ...; then gh pr merge 1; fi` のような制御構文の内側にある呼び出しも、
#    断片単位で見れば `then gh pr merge 1` として素直に判定できる
# 2. `gh pr merge 1 && gh pr merge 2` のように複数のマージが連結されていても、
#    断片ごとにPR番号を抽出するので、後続のマージを見落とさない
#    （文字列全体を1回grepして「最初に現れた数値」を拾う方式だと、無関係な言及や
#    2件目以降のマージ呼び出しを取りこぼす — 実際に発生した不具合）
#
# 単独の `&`（バックグラウンド実行）は区切り文字に含めない。`2>&1` や `&>out` の
# ようなリダイレクト演算子の一部として現れる頻度の方が圧倒的に高く、区切ってしまうと
# 直後のPR番号を誤って途中で断ち切ってしまう（実際に `gh pr merge N ... 2>&1 | tail`
# を自分自身のマージで実行して踏んだ）。バックグラウンド実行で繋いだマージを
# 見落とす可能性は残るが、実害の大きさが非対称なのでこちらを優先する。
#
# 外部コマンドを使わず bash の文字列置換だけで行う（sed/awk 等への依存を増やさない）。
# 既知の限界: クォート・ヒアドキュメント・`${...}` のようなパラメータ展開の中身も
# 区切り文字として解釈してしまう（シェルの構文解析はしていないため）。
hook_split_segments() {
  local s=$1
  s=${s//&&/$'\n'}
  s=${s//'||'/$'\n'}
  s=${s//'|'/$'\n'}
  s=${s//;/$'\n'}
  s=${s//(/$'\n'}
  s=${s//)/$'\n'}
  s=${s//\{/$'\n'}
  s=${s//\}/$'\n'}
  printf '%s' "$s"
}

# 断片(hook_split_segments の1行)が実際のマージ呼び出しかを判定する。
# 先頭（行頭の空白の後）から、環境変数代入・ラッパーコマンド（sudo/env/time/nohup/
# command/xargs）・`then`/`else`/`elif`/`do` のようなシェルキーワードを任意個許した上で
# `gh pr merge` に一致するかを見る。断片単位なので `^` アンカーで十分（誤マッチ防止に
# 「文中の言及」を拾わない効果もそのまま持つ）。
hook_segment_is_merge() {
  printf '%s' "$1" | grep -Eq \
    '^[[:space:]]*((then|else|elif|do)[[:space:]]+|[A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+|(sudo|env|time|nohup|command|xargs)[[:space:]]+)*gh[[:space:]]+pr[[:space:]]+merge\b'
}

# hook_segment_is_merge で真と判定された断片から、その呼び出しのPR番号を取り出す。
# 断片単位で行うので「文字列全体で最初に現れた数値」ではなく「その呼び出し自身の
# 数値」を正しく拾える。番号が書かれていない場合は空文字列を返す（呼び出し元が
# 現在のブランチのPRとして解決する）。
hook_segment_pr_num() {
  # head ではなく bash 組み込みの read で先頭1件だけ取り出す。head は fail-closed の
  # 対象ツールに含めていないので、無いと「PR番号を特定できない」という誤った理由で
  # denyされてしまう（実際にテストで発覚した）。
  # `grep -m1` は「先頭1行だけ出力」であって「先頭1件だけ出力」ではない —
  # `-o` は1行内の複数マッチを全て別行に出すので、同じ行に2つ数字があると
  # （例: リダイレクトの `2>` が断片の末尾に残ったケース）両方拾ってしまう。
  # 実際に自分自身のマージ実行時に踏んだ不具合。
  local n
  while IFS= read -r n; do break; done < <(printf '%s' "$1" | grep -oE 'pr[[:space:]]+merge\b.*' | grep -oE '[0-9]+')
  printf '%s' "$n"
}

# コマンド文字列に `gh pr merge` の実際の呼び出しが（どこかの断片に）含まれるかを
# 判定する。yes/no の判定だけで済む呼び出し元向け（PR番号の抽出が要る場合は
# hook_split_segments + hook_segment_is_merge/hook_segment_pr_num を直接使うこと）。
hook_is_pr_merge() {
  local seg
  while IFS= read -r seg; do
    hook_segment_is_merge "$seg" && return 0
  done <<< "$(hook_split_segments "$1")"
  return 1
}

# 文字列を JSON の値として安全な形にエスケープする（\ " タブ 改行 復帰）。
# jq に依存せず bash 組み込みのパラメータ展開だけで行う — enforce-code-review.sh の
# bail() は lib.sh を読み込む前に使われることがあるため、jq はもちろん sed/awk のような
# 外部コマンドにも頼れない。同じロジックを bail() 側にも複製している（コメント参照）。
hook_json_escape() {
  local s=$1
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\t'/\\t}
  s=${s//$'\r'/\\r}
  s=${s//$'\n'/\\n}
  printf '%s' "$s"
}

# PreToolUse フックの deny 応答を出力する。$1 = 理由（ユーザーに表示される）。
hook_deny() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$(hook_json_escape "$1")"
}
