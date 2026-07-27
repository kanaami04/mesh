#!/usr/bin/env bash
# **組み合わせを機械的に作ってTS版と突き合わせる**(milestone 67)。
#
# `scripts/parity.sh` は「コーパスに載っている形」しか測れない。milestone 65で見つけた
# 誤検知5種は、`!`/`&&`/`||` で包んだ条件式という**単体では全部コーパスにある要素の
# 組み合わせ**だった——だから parity は0件のまま通り続けていた。
#
# このスクリプトは軸(値の作り方 × 条件の書き方 × 使い方 など)の直積を一時ファイルへ
# 展開し、TS版とRust版の診断コード集合を突き合わせる。**コーパスへ入れるのは差が出た形と
# 代表例だけ**——直積そのものを常設すると数千件になり回らなくなるため、
# 「新しい検査を足したら手で一度回す」使い方を想定している。
#
#   scripts/sweep.sh            # 既定の3軸を回す
#   scripts/sweep.sh --keep     # 生成した .mesh を消さずに残す(差が出たとき調べる用)
#
# 出力の見方: FP=Rust側だけに出る診断(最悪) / MISS=TS版だけに出る(検出漏れ) /
# SKIP=そもそもパースできない(生成テンプレートの誤り。**空振りなので必ず直すこと**——
# milestone 62では19件中3件が空振りしていたのに一致として数えていた)
set -uo pipefail
cd "$(dirname "$0")/.."

KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1

RUST_BIN=rust/target/debug/mesh
if [ ! -f "src/cli.ts" ]; then
	echo "error: リポジトリルートが見つからない(このスクリプトは scripts/ に置いたまま実行すること)" >&2
	exit 2
fi
if [ ! -x "$RUST_BIN" ]; then
	echo "error: $RUST_BIN が無い。先に 'cargo build --manifest-path rust/Cargo.toml' を実行すること" >&2
	exit 2
fi

OUT=$(mktemp -d)
python3 scripts/sweep_gen.py "$OUT"

fp=0
miss=0
skip=0
ok=0
for f in "$OUT"/*.mesh; do
	b=$(basename "$f" .mesh)
	ts=$(bun src/cli.ts check "$f" 2>&1 | grep -oE 'error\[[a-z-]+\]' | sort | tr '\n' ' ')
	rs=$("$RUST_BIN" check "$f" 2>&1 | grep -oE 'error\[[a-z-]+\]' | sort | tr '\n' ' ')
	case "$ts" in
	*syntax-error*)
		skip=$((skip + 1))
		echo "SKIP $b — パースできていない(テンプレートの誤り。この件は検証になっていない)"
		continue
		;;
	esac
	if [ "$ts" = "$rs" ]; then
		ok=$((ok + 1))
		continue
	fi
	extra=$(comm -13 <(echo "$ts" | tr ' ' '\n' | sort -u) <(echo "$rs" | tr ' ' '\n' | sort -u) | tr -d ' \n')
	if [ -n "$extra" ]; then
		fp=$((fp + 1))
		echo "FP   $b — TS:[$ts] RS:[$rs]  ($f)"
	else
		miss=$((miss + 1))
		echo "MISS $b — TS:[$ts] RS:[$rs]  ($f)"
	fi
done

echo "---"
echo "一致 ${ok} / 誤検知 ${fp} / 検出漏れ ${miss} / パース失敗 ${skip}"
if [ "$KEEP" = "1" ]; then
	echo "生成物: $OUT"
else
	rm -rf "$OUT"
fi
[ "$fp" -eq 0 ] && [ "$skip" -eq 0 ] || {
	echo "FAIL: 誤検知またはパース失敗がある" >&2
	exit 1
}
echo "OK"
