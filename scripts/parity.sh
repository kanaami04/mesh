#!/usr/bin/env bash
# TS実装(オラクル)とRust移植版の `mesh check` 出力を突き合わせる。
#
# **この移植で最悪の不具合は「Rust側だけに出る診断」(誤検知)**なので、それを見つけたら
# 失敗させる。逆向きの差(TS版だけに出る=検出漏れ)は移植途中なので情報として出すだけ。
# milestone 46〜48は3回連続で誤検知を出荷しかけており、いずれも書き捨てのワンライナーで
# 確認していた——判定を固定するのがこのスクリプトの目的。
#
#   scripts/parity.sh            # 突き合わせ(誤検知があれば非ゼロ終了)
#   scripts/parity.sh --update   # Rust側の期待出力(expected.txt)を更新する
#
# 対象は examples/*.mesh と tests/parity/*/main.mesh。後者は1ケース1ディレクトリで、
# サブディレクトリを置けばパッケージ跨ぎも表現できる(エントリは常に main.mesh)。
set -uo pipefail
cd "$(dirname "$0")/.."

UPDATE=0
[ "${1:-}" = "--update" ] && UPDATE=1

# **必ずリポジトリルートから走らせる**(上の cd)。相対パスのまま別ディレクトリで実行すると
# 全件が「差あり」になる誤った計測になる——実際にセッション中2回踏んだ
RUST_BIN=rust/target/debug/mesh
if [ ! -f "src/cli.ts" ]; then
	echo "error: リポジトリルートが見つからない(このスクリプトは scripts/ に置いたまま実行すること)" >&2
	exit 2
fi
if [ ! -x "$RUST_BIN" ]; then
	echo "error: $RUST_BIN が無い。先に 'cargo build --manifest-path rust/Cargo.toml' を実行すること" >&2
	exit 2
fi

# 出力からファイルパスを除く(絶対パス・ディレクトリ構成に依存させないため)
normalize() { sed -e "s#^[^:]*/\([^/:]*\.mesh\)#\1#" -e "s#^\([^:]*\.mesh\)#\1#"; }
ts_check() { bun src/cli.ts check "$1" 2>&1 | grep -v '^\$ \|^error: script' | normalize; }
rs_check() { "$RUST_BIN" check "$1" 2>&1 | normalize; }

fp=0     # Rust側だけに出る診断(誤検知)——これがあれば失敗
miss=0   # TS版だけに出る診断(検出漏れ)——情報
n=0
updated=0

targets=()
for f in examples/*.mesh; do targets+=("$f"); done
for d in tests/parity/*/; do [ -f "$d/main.mesh" ] && targets+=("$d/main.mesh"); done

for f in "${targets[@]}"; do
	n=$((n + 1))
	ts=$(ts_check "$f")
	rs=$(rs_check "$f")

	# tests/parity/ のケースは Rust側の期待出力も保存する(CIはこれだけ見る)
	case "$f" in
	tests/parity/*)
		exp="$(dirname "$f")/expected.txt"
		if [ "$UPDATE" = "1" ]; then
			printf '%s\n' "$rs" >"$exp"
			updated=$((updated + 1))
		elif [ -f "$exp" ] && [ "$(cat "$exp")" != "$rs" ]; then
			echo "SNAPSHOT-DIFF $f — 期待と違う(意図した変更なら --update)"
			diff <(cat "$exp") <(printf '%s\n' "$rs") | sed 's/^/    /'
			fp=$((fp + 1))
		fi
		;;
	esac

	[ "$ts" = "$rs" ] && continue

	only_rs=$(diff <(printf '%s\n' "$ts") <(printf '%s\n' "$rs") | grep '^>' | grep -o 'error\[[a-z-]*\]' | sort -u | tr '\n' ' ')
	only_ts=$(diff <(printf '%s\n' "$ts") <(printf '%s\n' "$rs") | grep '^<' | grep -o 'error\[[a-z-]*\]' | sort -u | tr '\n' ' ')
	if [ -n "$only_rs" ]; then
		fp=$((fp + 1))
		echo "FALSE-POSITIVE $f — Rust側だけ: $only_rs"
		diff <(printf '%s\n' "$ts") <(printf '%s\n' "$rs") | sed 's/^/    /'
	else
		miss=$((miss + 1))
		echo "  miss $f — TS版だけ: $only_ts"
	fi
done

echo "---"
if [ "$UPDATE" = "1" ]; then
	echo "expected.txt を ${updated} 件更新した。**TS版との差が検出漏れだけであることを確認してからコミットすること**"
fi
echo "対象 ${n} ファイル / 検出漏れ ${miss} 件 / 誤検知・スナップショット差 ${fp} 件"
[ "$fp" -eq 0 ] || {
	echo "FAIL: Rust側だけに出る診断(またはスナップショット差)がある" >&2
	exit 1
}
echo "OK: Rust側だけに出る診断は無い"
