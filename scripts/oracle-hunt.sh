#!/usr/bin/env bash
# **TS版オラクルとの差分ハンター**。ランダムに生成した .mesh を Rust版と TS版の両方へ通し、
# 診断が食い違う形を探す。
#
#   scripts/oracle-hunt.sh              # 200件、シードは時刻から
#   scripts/oracle-hunt.sh 500          # 件数を指定
#   scripts/oracle-hunt.sh 200 1234     # シードも指定(差が出た形の再現用)
#
# **なぜオラクルに聞けるのか**: TS実装は 2026-07-27 の `cd7273a` で削除されただけで、
# git 履歴からいつでも復元できる。「実行できるオラクルを失った」と長く思い込んでいたが、
# 失っていたのは**常時稼働の並走**であって、問い合わせる手段ではなかった
# (詳細は docs/handoff.md「TS版のソースは git 履歴に残っている」節)。
#
# `mise run sweep` との違いは**探索の仕方**:
#   sweep       … 軸の直積を決め打ちで作り、**記録**と突き合わせる(毎回同じ126件・CI向き)
#   oracle-hunt … 軸をランダムに組み合わせ、**オラクル**と突き合わせる(毎回違う形・探索向き)
#
# 記録を持たないので `--update` に相当する抜け道が無い——**判定はオラクルがする**。
set -uo pipefail
cd "$(dirname "$0")/.."

COUNT="${1:-200}"
SEED="${2:-$(date +%s)}"

RUST_BIN=rust/target/debug/mesh
if [ ! -x "$RUST_BIN" ]; then
	echo "error: $RUST_BIN が無い。先に 'cargo build --manifest-path rust/Cargo.toml' を実行すること" >&2
	exit 2
fi

# オラクルは固定コミット(`cd7273a^`)に紐づくので**キャッシュして使い回してよい**
# (worktreeと違って古くなることがない)。無ければ復元する
ORACLE=/tmp/mesh-ts-oracle
if [ ! -f "$ORACLE/src/cli.ts" ]; then
	echo "TS版オラクルを復元中(cd7273a^)..."
	rm -rf "$ORACLE"
	mkdir -p "$ORACLE"
	git archive cd7273a^ src package.json tsconfig.json bun.lock | tar -x -C "$ORACLE" || {
		echo "error: オラクルのソースを取り出せない" >&2
		exit 2
	}
	(cd "$ORACLE" && bun install --frozen-lockfile >/dev/null 2>&1) || {
		echo "error: オラクルの依存を入れられない(bunが要る)" >&2
		exit 2
	}
fi

# 差が出た形は**捨てずに残す**。調査もコーパスへの昇格もここから始める
FINDS=$(mktemp -d /tmp/mesh-oracle-finds.XXXXXX)
OUT=$(mktemp -d)
python3 scripts/oracle_hunt_gen.py "$OUT" "$SEED" "$COUNT"

# **位置(行:桁)まで含めて比べる**。診断コードの集合だけだと比較が弱く、
# 「同じコードが違う場所に出る」ズレを見逃す(TS版には同じ注釈を2回報告する場所がある)。
# 250件で位置込みと集合のみを両方試して差が出ないことを確認したうえで、厳しい側を採った
diags() { grep -oE '[0-9]+:[0-9]+: error\[[a-z-]+\]' | tr '\n' '|'; }

diffs=0
skips=0
n=0
for f in "$OUT"/*.mesh; do
	n=$((n + 1))
	rs=$("$RUST_BIN" check "$f" 2>&1 | diags)
	# **パースできない生成物は検証になっていない**(milestone 62で19件中3件が空振りしていた)
	case "$rs" in *syntax-error*)
		skips=$((skips + 1))
		echo "SKIP $(basename "$f") — パースできない(生成テンプレートの誤り)"
		continue
		;;
	esac
	ts=$(cd "$ORACLE" && bun src/cli.ts check "$f" 2>&1 | diags)
	if [ "$rs" != "$ts" ]; then
		diffs=$((diffs + 1))
		cp "$f" "$FINDS/"
		echo "DIFF $(basename "$f" .mesh)"
		echo "    Rust: [$rs]"
		echo "    TS  : [$ts]"
	fi
done

echo "---"
echo "対象 $n 件 / オラクルとの差 ${diffs} 件 / パース失敗 ${skips} 件 / seed=$SEED"
if [ "$diffs" -gt 0 ]; then
	echo "差が出た形: $FINDS"
	echo "**同じ集合を再生成するには: scripts/oracle-hunt.sh $COUNT $SEED**"
else
	rmdir "$FINDS" 2>/dev/null
fi
rm -rf "$OUT"

[ "$diffs" -eq 0 ] && [ "$skips" -eq 0 ] || {
	echo "FAIL: オラクルとの差またはパース失敗がある" >&2
	exit 1
}
echo "OK"
