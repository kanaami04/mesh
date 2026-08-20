#!/bin/bash
# deferred-check.sh(PreToolUseフック)のテスト(ADR-0049決定1)。
#
# 各ケースは合成JSONを標準入力に流し、終了コード・標準出力・状態ファイルを検証する。
# このフックの故障モードはすべて「無音」(何も出さずexit 0)のため、黙るべき場面と
# 出すべき場面の両方を並べてピンする。台帳は一時ディレクトリに合成したものを使い、
# 実リポジトリの docs/adr/DEFERRED.md と .claude/state/ は書き換えない
# (実台帳を対象にするケース9だけは、実台帳を**コピーして**一時ディレクトリで検証する)。
#
# 実行: bash .claude/hooks/tests/deferred-check.test.sh(依存: bash・jq)
set -u

here=$(cd "$(dirname "$0")" && pwd -P)
hook="$here/../deferred-check.sh"
repo=$(cd "$here/../../.." && pwd -P)
real_ledger="$repo/docs/adr/DEFERRED.md"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq が見つかりません。deferred-check.sh は jq に依存するためテストできません。" >&2
  exit 1
fi
[ -f "$hook" ] || { echo "フック本体が見つかりません: $hook" >&2; exit 1; }
[ -f "$real_ledger" ] || { echo "実台帳が見つかりません: $real_ledger" >&2; exit 1; }

tmp=$(mktemp -d "${TMPDIR:-/tmp}/deferred-check-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

total=0
failed=0

report() { # report <ok|fail> <description>
  total=$((total + 1))
  if [ "$1" = ok ]; then
    printf 'ok %d - %s\n' "$total" "$2"
  else
    printf 'not ok %d - %s\n' "$total" "$2"
    failed=$((failed + 1))
  fi
}

assert_contains() { # <description> <haystack> <needle>
  case "$2" in
    *"$3"*) report ok "$1" ;;
    *) report fail "$1" ;;
  esac
}

assert_not_contains() { # <description> <haystack> <needle>
  case "$2" in
    *"$3"*) report fail "$1" ;;
    *) report ok "$1" ;;
  esac
}

assert_empty() { # <description> <actual>
  if [ -z "$2" ]; then
    report ok "$1"
  else
    report fail "$1(実際の出力: $2)"
  fi
}

assert_exit_zero() { # <description> <exit-code>
  if [ "$2" -eq 0 ]; then
    report ok "$1"
  else
    report fail "$1(実際の終了コード: $2)"
  fi
}

assert_file_has_line() { # <description> <file> <line>
  if grep -qxF "$3" "$2" 2>/dev/null; then
    report ok "$1"
  else
    report fail "$1($2 に $3 が無い)"
  fi
}

# --- テスト用プロジェクト1: 合成台帳 ------------------------------------------
# 行の割り当て: D-1=単独ヒット / D-2・D-3=同一ファイル2行 / D-4=解決済み /
# D-5=ID欄の後ろパッド / D-6=ID欄の前パッド(欄の空白ゆらぎの両形)。
proj="$tmp/proj"
mkdir -p "$proj/docs/adr" "$proj/src"
cat > "$proj/docs/adr/DEFERRED.md" <<'EOF'
# 申し送り台帳(テスト用)

| ID | 先送りした事柄 | トリガー | 根拠 | 放置した場合の影響 | 状態 |
|---|---|---|---|---|---|
| D-1 | 合成申し送り1(parser) | `file:src/parser.rs` | テスト | 影響1 | 未対応 |
| D-2 | 合成申し送り2(main) | `file:src/main.rs` | テスト | 影響2 | 未対応 |
| D-3 | 合成申し送り3(mainの2行目) | `file:src/main.rs` | テスト | 影響3 | 未対応 |
| D-4 | 解決済みの申し送り | `file:src/legacy.rs` | テスト | 影響4 | 解決(2026-08-20 対応済み) |
| D-5  | ID欄が後ろパッドされた行 | `file:src/padded.rs` | テスト | 影響5 | 未対応 |
|   D-6 | ID欄が前パッドされた行 | `file:src/padded.rs` | テスト | 影響6 | 未対応 |
EOF
touch "$proj/src/parser.rs" "$proj/src/main.rs" "$proj/src/legacy.rs" "$proj/src/padded.rs"
# 無関係ファイルも実在させる(パス正規化の cd が失敗しないように)
touch "$proj/src/unrelated.rs"

stdout=""
hook_rc=0

run_hook() { # <project-dir> <session-id> <file-path> — EditツールのJSONを組み立てて流す
  local dir="$1" sid="$2" path="$3"
  local json
  json=$(printf '{"session_id":"%s","tool_name":"Edit","tool_input":{"file_path":"%s"}}' "$sid" "$path")
  stdout=$(printf '%s' "$json" | CLAUDE_PROJECT_DIR="$dir" bash "$hook" 2>/dev/null)
  hook_rc=$?
}

run_hook_raw() { # <project-dir> <raw-stdin> — 壊れた入力のFail-open検証用
  local dir="$1" input="$2"
  stdout=$(printf '%s' "$input" | CLAUDE_PROJECT_DIR="$dir" bash "$hook" 2>/dev/null)
  hook_rc=$?
}

ctx() { # <hook-stdout> — additionalContextを取り出す(無音なら空文字列)
  printf '%s' "$1" | jq -r '.hookSpecificOutput.additionalContext // empty' 2>/dev/null
}

# --- ケース1: file: トリガーに該当するファイル → 該当行が出る -------------------
# 絶対パスで渡す形(Editツールが実際に流すのと同じ)で検証する。
proj_abs=$(cd "$proj" && pwd -P)
run_hook "$proj" "case1-session" "$proj_abs/src/parser.rs"
assert_exit_zero "ケース1: 終了コード0(ブロックしない)" "$hook_rc"
assert_contains "ケース1: 本文に該当行のID(D-1)が出る" "$(ctx "$stdout")" "D-1"
assert_contains "ケース1: 本文に該当行の事柄が出る" "$(ctx "$stdout")" "合成申し送り1"
assert_file_has_line "ケース1: 状態ファイルにD-1を記録" "$proj/.claude/state/deferred-shown-case1-session" "D-1"

# --- ケース2: 1ファイルに2行該当 → 2行とも出る ---------------------------------
run_hook "$proj" "case2-session" "src/main.rs"
assert_exit_zero "ケース2: 終了コード0" "$hook_rc"
assert_contains "ケース2: 1行目(D-2)が出る" "$(ctx "$stdout")" "D-2"
assert_contains "ケース2: 2行目(D-3)も出る" "$(ctx "$stdout")" "D-3"

# --- ケース3: 同一セッションの2回目 → 無音 -------------------------------------
run_hook "$proj" "case3-session" "src/main.rs"
run_hook "$proj" "case3-session" "src/main.rs"
assert_exit_zero "ケース3: 終了コード0" "$hook_rc"
assert_empty "ケース3: 同一セッション2回目は無音" "$stdout"

# --- ケース4: 別セッション → 再び出る ------------------------------------------
run_hook "$proj" "case4-session-a" "src/main.rs"
run_hook "$proj" "case4-session-b" "src/main.rs"
assert_contains "ケース4: 別セッションでは再びD-2が出る" "$(ctx "$stdout")" "D-2"
assert_contains "ケース4: 別セッションでは再びD-3も出る" "$(ctx "$stdout")" "D-3"

# --- ケース5: 無関係なファイル → 無音 ------------------------------------------
run_hook "$proj" "case5-session" "src/unrelated.rs"
assert_exit_zero "ケース5: 終了コード0" "$hook_rc"
assert_empty "ケース5: トリガーに無関係なファイルは無音" "$stdout"

# --- ケース6: 状態欄が「解決」の行 → 出ない -------------------------------------
run_hook "$proj" "case6-session" "src/legacy.rs"
assert_exit_zero "ケース6: 終了コード0" "$hook_rc"
assert_empty "ケース6: 解決済みの行だけが該当するときは無音" "$stdout"

# --- ケース7: ID欄の空白が揃えられた台帳 → 本文が空にならず出る ------------------
# ID欄の空白のゆらぎ(前パッド・後ろパッド)で行を取りこぼすと、
# 「中身が空の通知を1回出して以後永久に沈黙する」事故になる(impl-review 2026-08-20)。
run_hook "$proj" "case7-session" "src/padded.rs"
assert_exit_zero "ケース7: 終了コード0" "$hook_rc"
assert_contains "ケース7: 後ろパッド行(D-5)も本文に出る" "$(ctx "$stdout")" "D-5"
assert_contains "ケース7: 前パッド行(D-6)も本文に出る" "$(ctx "$stdout")" "D-6"

# --- ケース8: 壊れたJSON・file_path欠落 → exit 0で無音(Fail-open) ---------------
run_hook_raw "$proj" '{not json'
assert_exit_zero "ケース8a: 壊れたJSONでも終了コード0" "$hook_rc"
assert_empty "ケース8a: 壊れたJSONは無音" "$stdout"
run_hook_raw "$proj" '{"session_id":"case8b","tool_name":"Edit","tool_input":{}}'
assert_exit_zero "ケース8b: file_path欠落でも終了コード0" "$hook_rc"
assert_empty "ケース8b: file_path欠落は無音" "$stdout"

# --- ケース9: 実台帳の file: トリガー全行 → 本文が空でなくD-番号を含む ----------
# 実台帳を一時ディレクトリにコピーして検証する(実リポジトリに状態ファイルを作らない)。
proj_real="$tmp/proj-real"
mkdir -p "$proj_real/docs/adr" "$proj_real/src"
cp "$real_ledger" "$proj_real/docs/adr/DEFERRED.md"
real_rows=0
while IFS= read -r row; do
  [ -z "$row" ] && continue
  real_rows=$((real_rows + 1))
  id=$(printf '%s' "$row" | awk -F'|' '{print $2}' | tr -d '[:space:]*')
  rel=$(printf '%s' "$row" | sed -n 's/.*`file:\([^`]*\)`.*/\1/p')
  if [ -z "$id" ] || [ -z "$rel" ]; then
    report fail "ケース9: 実台帳の行からIDとfile:パスを抽出できる(ID=${id:-抽出不能} rel=${rel:-抽出不能})"
    continue
  fi
  report ok "ケース9: 実台帳の行からIDとfile:パスを抽出できる($id → $rel)"
  mkdir -p "$proj_real/$(dirname "$rel")"
  run_hook "$proj_real" "case9-$id" "$rel"
  assert_exit_zero "ケース9: $id($rel)は終了コード0" "$hook_rc"
  assert_contains "ケース9: 実台帳 $id($rel)の本文が空でなくD-番号を含む" "$(ctx "$stdout")" "$id"
done <<ROWS
$(grep '^|[[:space:]]*D-' "$real_ledger" | grep -F 'file:')
ROWS

if [ "$real_rows" -eq 0 ]; then
  report fail "ケース9: 実台帳にfile:トリガーの行が1行も見つからない(台帳かこの抽出の壊れ)"
else
  report ok "ケース9: 実台帳のfile:トリガー行を${real_rows}行検証した"
fi

# --- 結果サマリ ----------------------------------------------------------------
printf '1..%d\n' "$total"
# --- ケース10: permissionDecision を返さない(R1の回帰防止) --------------------
# "allow" は許可プロンプトを飛ばすため、「着手前に人が判断すべき」と印を付けた
# ファイルに限って許可判断を奪う。コメントで意図を書いても機械は撃たないので刺す。
run_hook "$proj" "case10-session" "src/parser.rs"
assert_empty "ケース10: permissionDecision を返さない(許可判断を奪わない)" \
  "$(printf '%s' "$stdout" | jq -r '.hookSpecificOutput.permissionDecision // empty')"

# --- ケース11: worktree配下のパスでも撃ち、その worktree の台帳を読む -----------
# パスだけ読み替えて台帳の根を本体に残すと、worktreeで台帳を更新しても
# 本体の古い台帳から撃たれる(impl-review 2026-08-20の指摘)。
wt="$proj/.claude/worktrees/feat-x"
mkdir -p "$wt/docs/adr" "$wt/src"
cat > "$wt/docs/adr/DEFERRED.md" <<'LEDGER'
| ID | 先送りした事柄 | トリガー | 根拠 | 影響 | 状態 |
|---|---|---|---|---|---|
| D-77 | worktree専用の申し送り | `file:src/parser.rs` | r | i | 未対応 |
LEDGER
run_hook "$proj" "case11-session" "$wt/src/parser.rs"
assert_contains "ケース11: worktree側の台帳の行(D-77)が出る" "$(ctx "$stdout")" "D-77"
assert_not_contains "ケース11: 本体側の台帳の行(D-1)は出ない" "$(ctx "$stdout")" "D-1"

# --- ケース12: 未作成ディレクトリへの新規Writeでも撃つ --------------------------
# `src/parser/mod.rs` を作る瞬間はディレクトリがまだ無い。祖先を遡って解決できないと
# 無音になり、まさにD-1・D-2のトリガー領域を取りこぼす。
cat >> "$proj/docs/adr/DEFERRED.md" <<'LEDGER'
| D-78 | 未作成ディレクトリの申し送り | `file:src/newdir/mod.rs` | r | i | 未対応 |
LEDGER
run_hook "$proj" "case12-session" "$proj_abs/src/newdir/mod.rs"
assert_contains "ケース12: 未作成ディレクトリでもD-78が出る" "$(ctx "$stdout")" "D-78"

# --- ケース13: 書式不正の行は知らせるが、2回目は黙る ---------------------------
# R2(永久沈黙)を直した代わりに永久連呼にしないこと。
cat >> "$proj/docs/adr/DEFERRED.md" <<'LEDGER'
| D-79x | ID欄が想定形でない行 | `file:src/broken.rs` | r | i | 未対応 |
LEDGER
run_hook "$proj" "case13-session" "src/broken.rs"
assert_contains "ケース13: 書式不正を知らせる" "$(ctx "$stdout")" "書式が不正"
run_hook "$proj" "case13-session" "src/broken.rs"
assert_empty "ケース13: 同一セッション2回目は黙る(永久連呼しない)" "$stdout"

# --- ケース14: 末尾パイプの無い行でも状態欄を正しく読む -------------------------
# GFMは末尾の `|` を省略できる。$(NF-1) 固定だと1列ずれて「解決」を読み落とす。
cat >> "$proj/docs/adr/DEFERRED.md" <<'LEDGER'
| D-80 | 末尾パイプ無しで解決済み | `file:src/nopipe.rs` | r | i | 解決(やらないと決めた)
LEDGER
run_hook "$proj" "case14-session" "src/nopipe.rs"
assert_empty "ケース14: 末尾パイプ無しでも解決行は提示しない" "$stdout"

if [ "$failed" -gt 0 ]; then
  printf 'FAILED: %d/%d assertions failed\n' "$failed" "$total"
  exit 1
fi
printf 'All %d assertions passed\n' "$total"
exit 0
