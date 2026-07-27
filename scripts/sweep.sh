#!/usr/bin/env bash
# **組み合わせを機械的に作って診断の出力を記録と突き合わせる**(milestone 67)。
#
# `scripts/parity.sh` は「コーパスに載っている形」しか測れない。milestone 65で見つけた
# 誤検知5種は、`!`/`&&`/`||` で包んだ条件式という**単体では全部コーパスにある要素の
# 組み合わせ**だった——だから parity は0件のまま通り続けていた。
#
# このスクリプトは軸(値の作り方 × 条件の書き方 × 使い方 など)の直積を一時ファイルへ
# 展開する。**コーパスへ入れるのは差が出た形と代表例だけ**——直積そのものを常設すると
# 数千件になり回らなくなるため、「新しい検査を足したら手で一度回す」使い方を想定している。
#
# **2026-07-27のTS実装撤去まで、これはTS版(オラクル)との突き合わせだった。**
# 今は撤去時点の出力を凍結した記録(`tests/sweep-expected.txt`)と突き合わせる。
#
#   scripts/sweep.sh            # 既定の直積を回す
#   scripts/sweep.sh --update   # 記録を更新する(**差分を必ず読むこと**)
#   scripts/sweep.sh --keep     # 生成した .mesh を消さずに残す(差が出たとき調べる用)
#
# 出力の見方: DIFF=記録と違う / SKIP=そもそもパースできない(生成テンプレートの誤り。
# **空振りなので必ず直すこと**——milestone 62では19件中3件が空振りしていたのに
# 一致として数えていた)
set -uo pipefail
cd "$(dirname "$0")/.."

KEEP=0
UPDATE=0
for a in "$@"; do
	[ "$a" = "--keep" ] && KEEP=1
	[ "$a" = "--update" ] && UPDATE=1
done

RUST_BIN=rust/target/debug/mesh
if [ ! -d "rust/src" ]; then
	echo "error: リポジトリルートが見つからない(このスクリプトは scripts/ に置いたまま実行すること)" >&2
	exit 2
fi
if [ ! -x "$RUST_BIN" ]; then
	echo "error: $RUST_BIN が無い。先に 'cargo build --manifest-path rust/Cargo.toml' を実行すること" >&2
	exit 2
fi

OUT=$(mktemp -d)
python3 scripts/sweep_gen.py "$OUT"

EXPECTED=tests/sweep-expected.txt
actual=$(mktemp)
skip=0
for f in "$OUT"/*.mesh; do
	b=$(basename "$f" .mesh)
	rs=$("$RUST_BIN" check "$f" 2>&1 | grep -oE 'error\[[a-z-]+\]' | sort | tr '\n' ' ')
	case "$rs" in
	*syntax-error*)
		skip=$((skip + 1))
		echo "SKIP $b — パースできていない(テンプレートの誤り。この件は検証になっていない)"
		;;
	esac
	printf '%s\t%s\n' "$b" "$rs" >>"$actual"
done
# **並び順をバイト順に固定する**——globの順序も`sort`の照合順序もロケール/環境依存で、
# そのままだとCIでだけ差分になる(`stmt_for_ok`と`stmt_forever_ok`の前後が入れ替わって実測で発覚)
LC_ALL=C sort -o "$actual" "$actual"

if [ "$UPDATE" = "1" ]; then
	mkdir -p "$(dirname "$EXPECTED")"
	cp "$actual" "$EXPECTED"
	echo "記録を更新した($(wc -l <"$EXPECTED") 件)。**差分を必ず読むこと**"
	rm -f "$actual"
	[ "$KEEP" = "1" ] && echo "生成物: $OUT" || rm -rf "$OUT"
	exit 0
fi

diffs=0
if [ ! -f "$EXPECTED" ]; then
	echo "error: 記録が無い($EXPECTED)。--update で作ること" >&2
	diffs=1
elif ! diff -q "$EXPECTED" "$actual" >/dev/null; then
	echo "DIFF: 記録と違う(意図した変更なら --update)"
	diff "$EXPECTED" "$actual" | head -40 | sed 's/^/    /'
	diffs=$(diff "$EXPECTED" "$actual" | grep -c '^[<>]')
fi

echo "---"
echo "対象 $(wc -l <"$actual") 件 / 記録との差 ${diffs} 件 / パース失敗 ${skip}"
rm -f "$actual"
if [ "$KEEP" = "1" ]; then
	echo "生成物: $OUT"
else
	rm -rf "$OUT"
fi
[ "$diffs" -eq 0 ] && [ "$skip" -eq 0 ] || {
	echo "FAIL: 記録との差またはパース失敗がある" >&2
	exit 1
}
echo "OK"
