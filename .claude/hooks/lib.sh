#!/bin/bash
# フック共通のヘルパー（enforce-code-review.sh / enforce-review-before-push.sh /
# remind-worktree.sh / block-git-stash.sh が読む）。
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
# クォートで囲まれた中身を落とす。`gh pr comment --body "... git stash list ..."` のように
# **引数の文字列として**コマンド名が現れる形で誤発火するのを防ぐ(2026-07-27に
# block-git-stash.sh が実際にこれで誤発火し、レビューコメントの投稿が止まった)。
# シェルの正確なパースはしない——「クォートの中はコマンドではない」という近似で十分で、
# 近似を外す方向(=拾いすぎ)より落とす方向に倒す方が、フックとしては安全側になる。
# エスケープされたクォートは扱わない(その形でコマンドを隠すのは現実的でない)。
# **ヒアドキュメント(`<<'EOF' ... EOF`)の中身も落とせない**——フックはコマンド文字列しか
# 見えず、範囲を正しく解釈するにはシェルのパーサが要る。実害は「長文をヒアドキュメントで
# 渡すと止まる」ことで、`--body-file` にすれば回避できるため近似のまま残す(2026-07-27に
# このPR自身のレビューコメント投稿で踏んだ)。
# **`hook_is_pr_merge` には適用しない**——あちらは断片からPR番号を取り出すのに引数
# (`gh pr view -q "..."` のクエリ等)を保ったまま扱う必要があり、落とすとテストが壊れた
# (実際に踏んだ)。マージ判定は文字列引数の中にコマンド名が現れる形の被害が無いので
# 適用しなくてよい
hook_strip_quoted() {
  printf '%s' "$1" | sed -e "s/'[^']*'/ /g" -e 's/"[^"]*"/ /g'
}

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

# 断片が `git worktree add` の呼び出しかを判定する(hook_segment_is_merge と同じ考え方——
# 行頭から環境変数代入・ラッパーコマンド・シェルキーワードを任意個許して一致を見る)。
# `git worktree list` / `remove` / `prune` は対象外。文中の言及(echo等の引数)も拾わない。
# gitのグローバルオプションは値を取るものがある(`git -C <path> worktree add`)ので、
# 「`-`で始まる語 + 任意でその値」の繰り返しを許す(テストで発覚)。
hook_segment_is_worktree_add() {
  printf '%s' "$1" | grep -Eq \
    '^[[:space:]]*((then|else|elif|do)[[:space:]]+|[A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+|(sudo|env|time|nohup|command|xargs)[[:space:]]+)*git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-][^[:space:]]*)?)*[[:space:]]+worktree[[:space:]]+add\b'
}

# コマンド文字列に `git worktree add` の実際の呼び出しが含まれるかを判定する
hook_is_worktree_add() {
  local seg
  while IFS= read -r seg; do
    hook_segment_is_worktree_add "$seg" && return 0
  done <<< "$(hook_split_segments "$(hook_strip_quoted "$1")")"
  return 1
}

# 断片が `git push` の呼び出しかを判定する(hook_segment_is_worktree_add と同じ考え方)。
# `git -C <path> push` のようにグローバルオプションを挟む形も拾う。
hook_segment_is_push() {
  printf '%s' "$1" | grep -Eq \
    '^[[:space:]]*((then|else|elif|do)[[:space:]]+|[A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+|(sudo|env|time|nohup|command|xargs)[[:space:]]+)*git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-][^[:space:]]*)?)*[[:space:]]+push\b'
}

# コマンド文字列に `git push` の実際の呼び出しが含まれるか
hook_is_push() {
  local seg
  while IFS= read -r seg; do
    hook_segment_is_push "$(hook_strip_quoted "$seg")" && return 0
  done <<< "$(hook_split_segments "$1")"
  return 1
}

# push 断片から「送られるローカル側の ref」を1行ずつ取り出す。
#
# **クォートを落とさない断片を渡すこと**。hook_strip_quoted は判定(is_push)にだけ使う
# ——引数まで落とすと `git push origin "HEAD:refs/heads/x"` の refspec が消えて
# 「引数なし = HEAD」に化け、別のコミットを無記録で通してしまう(fail-open 方向の事故)。
#
# 出力の約束:
#   - 通常の push          → ローカル側の ref 名（解決前の文字列。`HEAD` を含む）
#   - 何も送らない呼び出し → 出力なし（`--dry-run` / `--delete` / `:branch` 形式の削除）
#   - 対象を特定できない形 → `!unresolvable` の1行（`--all` / `--mirror` / `--tags` /
#                            別リポジトリを指す `-C` など）。呼び出し元は deny する
hook_push_srcs() {
  local -a words=()
  read -r -a words <<< "$1"
  local n=${#words[@]} i=0 w

  # `git` そのものに到達するまで読み飛ばす（環境変数代入・ラッパー・シェルキーワード）
  while [ "$i" -lt "$n" ] && [ "${words[$i]}" != git ]; do i=$((i + 1)); done
  if [ "$i" -ge "$n" ]; then printf '!unresolvable\n'; return; fi
  i=$((i + 1))

  # `push` までのグローバルオプション。作業対象のリポジトリを差し替えるものが
  # 混じっていたら、このリポジトリの HEAD と突き合わせる前提が崩れるので降参する
  while [ "$i" -lt "$n" ] && [ "${words[$i]}" != push ]; do
    case "${words[$i]}" in
      -C|--git-dir|--git-dir=*|--work-tree|--work-tree=*) printf '!unresolvable\n'; return ;;
    esac
    i=$((i + 1))
  done
  if [ "$i" -ge "$n" ]; then printf '!unresolvable\n'; return; fi
  i=$((i + 1))

  local -a positional=()
  local skip_next=0 dry=0 delete=0 broad=0
  while [ "$i" -lt "$n" ]; do
    w=${words[$i]}
    i=$((i + 1))
    if [ "$skip_next" -eq 1 ]; then skip_next=0; continue; fi
    case "$w" in
      --dry-run|-n) dry=1 ;;
      --delete|-d) delete=1 ;;
      # 送る先が広すぎて「このコミット」に還元できない形
      --all|--mirror|--tags|--follow-tags) broad=1 ;;
      # 値を**次の単語**として取るオプション（refspec と取り違えないよう1語読み飛ばす）。
      # `--force-with-lease` はここに入れない——値は `=` で付ける形しか無く、
      # 素の `git push --force-with-lease origin main` で origin を食べてしまう
      -o|--push-option|--repo|--exec|--receive-pack) skip_next=1 ;;
      -*) ;;
      *)
        # リダイレクションは引数ではない。`git push -u origin br 2>&1 | tail` の
        # `2>&1` を refspec と取り違えて「解決できない」で止まる
        # （**このフック自身の初回 push で踏んだ**。単独の `&` は区切り文字に
        # していないので `2>&1` は断片に残る——hook_split_segments のコメント参照。
        # マージ側も同じ形で一度壊れている）
        if printf '%s' "$w" | grep -Eq '^([0-9]*|&)(>>?|<)'; then
          # 演算子だけの語（`>` `2>` `>>` `<`）なら、次の語はファイル名なので一緒に飛ばす
          printf '%s' "$w" | grep -Eq '^([0-9]*|&)(>>?|<)$' && skip_next=1
        else
          positional+=("$w")
        fi
        ;;
    esac
  done

  # 何も送らない呼び出しは記録を要求しない
  if [ "$dry" -eq 1 ] || [ "$delete" -eq 1 ]; then return; fi
  if [ "$broad" -eq 1 ]; then printf '!unresolvable\n'; return; fi

  # 最初の位置引数はリモート（`git push main` の `main` もリモート名として解釈される）。
  # refspec が無ければ送られるのは現在のブランチ = HEAD
  if [ "${#positional[@]}" -le 1 ]; then printf 'HEAD\n'; return; fi

  local p src
  for p in "${positional[@]:1}"; do
    # 引数まわりのクォートを落とす。断片は**クォートを保ったまま**渡される約束なので、
    # `git push origin "HEAD:refs/heads/x"` の refspec は `"HEAD:...` の形で届く
    # （ref 名にクォートは使えないので、単純に取り除いてよい）
    p=${p//\"/}
    p=${p//\'/}
    src=${p%%:*}
    src=${src#+}
    [ -z "$src" ] && continue # `:branch` 形式はリモートブランチの削除
    printf '%s\n' "$src"
  done
}

# code review の記録（enforce-review-before-push.sh が読み、scripts/record-review.sh が書く）を
# 置くディレクトリ。$1 = リポジトリのルート。**両者がずれると片方だけ直る事故になる**ので
# ここに一本化する
hook_review_log_dir() {
  printf '%s' "${1%/}/.claude/review-log"
}

hook_segment_is_stash() {
  # `git stash` 全般（save/push/pop/apply、引数なしの `git stash` も）。
  # 比較目的の退避が事故を起こすので、読み取り専用の `git stash list` / `show` だけは通す。
  printf '%s' "$1" | grep -Eq \
    '^[[:space:]]*((then|else|elif|do)[[:space:]]+|[A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+|(sudo|env|time|nohup|command|xargs)[[:space:]]+)*git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-][^[:space:]]*)?)*[[:space:]]+stash\b' \
    && ! printf '%s' "$1" | grep -Eq '[[:space:]]+stash[[:space:]]+(list|show)\b'
}

# コマンド文字列に `git stash`（読み取り専用の list/show を除く）の呼び出しが含まれるか
hook_is_stash() {
  local seg
  while IFS= read -r seg; do
    hook_segment_is_stash "$seg" && return 0
  done <<< "$(hook_split_segments "$(hook_strip_quoted "$1")")"
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
# レビューコメント判定に使う jq フィルタ。**各コメントの先頭行だけ**を1行ずつ出す。
# 全文を見ると、マーカーを**説明・引用**しただけのコメント(コードフェンスの中に
# `### Code review skipped: ...` と書いた等)でもゲートを通ってしまう——「確認できない
# ときは deny」という原則に反する(code reviewで発覚・モックghで再現確認)。
# 見出しはコメントの先頭行に書く運用なので、先頭行に限定しても正当なコメントは落ちない。
# フックとテストで同じ文字列を使うためにここへ切り出している(片方だけ直る事故を防ぐ)。
hook_comment_first_lines_filter() {
  printf '%s' '.comments[].body | split("\n")[0] | sub("\r$"; "")'
}

# 確認(ask)応答。deny と違い「止める」ではなく「理由を見せて判断を仰ぐ」用途
hook_ask() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"%s"}}\n' "$(hook_json_escape "$1")"
}

hook_deny() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$(hook_json_escape "$1")"
}
