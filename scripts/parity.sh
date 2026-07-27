#!/usr/bin/env bash
# Rust版 `mesh check` の出力が**記録どおり**かを確かめる。
#
# **2026-07-27のTS実装撤去まで、これはTS版(オラクル)との突き合わせだった。**
# 「Rust側だけに出る診断」(この移植で最悪の不具合)を見つけるのが目的で、
# milestone 46〜48の3連続の誤検知や、milestone 65でmainに出荷されていた誤検知5種を
# 実際に捕まえてきた。TS版が消えた今は**撤去時点の出力を正解として凍結**したスナップショットと
# 突き合わせる形に縮退している——比較相手は変わったが、役割(生成物の退行検出)は同じ。
#
#   scripts/parity.sh            # 突き合わせ(差があれば非ゼロ終了)
#   scripts/parity.sh --update   # 期待出力(expected.txt)を更新する
#
# 対象は examples/*.mesh と tests/parity/*/main.mesh。後者は1ケース1ディレクトリで、
# サブディレクトリを置けばパッケージ跨ぎも表現できる(エントリは常に main.mesh)。
#
# **生成JSの突き合わせは `rust/tests/codegen_snapshot.rs` が担う**(撤去前は
# このスクリプトがTS版と1バイト単位で比べていた分)。
set -uo pipefail
cd "$(dirname "$0")/.."

UPDATE=0
[ "${1:-}" = "--update" ] && UPDATE=1

# **必ずリポジトリルートから走らせる**(上の cd)。相対パスのまま別ディレクトリで実行すると
# 全件が「差あり」になる誤った計測になる——実際にセッション中2回踏んだ
RUST_BIN=rust/target/debug/mesh
if [ ! -d "rust/src" ]; then
	echo "error: リポジトリルートが見つからない(このスクリプトは scripts/ に置いたまま実行すること)" >&2
	exit 2
fi
if [ ! -x "$RUST_BIN" ]; then
	echo "error: $RUST_BIN が無い。先に 'cargo build --manifest-path rust/Cargo.toml' を実行すること" >&2
	exit 2
fi

# 出力からファイルパスを除く(絶対パス・ディレクトリ構成に依存させないため)
normalize() { sed -e "s#^[^:]*/\([^/:]*\.mesh\)#\1#" -e "s#^\([^:]*\.mesh\)#\1#"; }
rs_check() { "$RUST_BIN" check "$1" 2>&1 | normalize; }

diffs=0
n=0
updated=0

targets=()
for f in examples/*.mesh; do targets+=("$f"); done
for d in tests/parity/*/; do [ -f "$d/main.mesh" ] && targets+=("$d/main.mesh"); done

for f in "${targets[@]}"; do
	n=$((n + 1))
	rs=$(rs_check "$f")
	# **examples も記録する**(撤去前はTS版と比べていた分をスナップショットで置き換える)
	case "$f" in
	examples/*) exp="tests/parity-examples/$(basename "$f" .mesh).txt" ;;
	*) exp="$(dirname "$f")/expected.txt" ;;
	esac
	mkdir -p "$(dirname "$exp")"
	if [ "$UPDATE" = "1" ]; then
		printf '%s\n' "$rs" >"$exp"
		updated=$((updated + 1))
		continue
	fi
	if [ ! -f "$exp" ]; then
		echo "MISSING $f — 記録が無い($exp)。--update で作ること"
		diffs=$((diffs + 1))
		continue
	fi
	if [ "$(cat "$exp")" != "$rs" ]; then
		echo "DIFF $f — 記録と違う(意図した変更なら --update)"
		diff <(cat "$exp") <(printf '%s\n' "$rs") | sed 's/^/    /'
		diffs=$((diffs + 1))
	fi
done

echo "---"
if [ "$UPDATE" = "1" ]; then
	echo "期待出力を ${updated} 件更新した。**差分を必ず読むこと**——説明できない変化は退行の可能性が高い"
fi
echo "対象 ${n} ファイル / 記録との差 ${diffs} 件"
[ "$diffs" -eq 0 ] || {
	echo "FAIL: 診断の出力が記録と違う" >&2
	exit 1
}
echo "OK: 診断の出力は記録どおり"
