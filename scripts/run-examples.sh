#!/usr/bin/env bash
# examples/*.mesh を全部実行して動作確認する。既定はTS版(オラクル)、
# `--rust` を渡すとRust版CLIで走らせる。
#
# **1本落ちても止めない**——`set -e`で打ち切られていたため、意図的にpanicする
# defer_panic.mesh(7本目)以降の17本が長らく一度も実行されていなかった(milestone 64で発覚)。
# 期待どおり落ちる例はEXPECT_FAILへ列挙し、それ以外の失敗があるときだけ非ゼロで終わる。
#
# **「どの例が意図的に落ちるか」の知識をここ1箇所に置く**のが目的でもある——
# 以前はmise.tomlとCIワークフローに同じ判定が二重に書かれていた(撤去計画 段階1で統合)。
set -uo pipefail
cd "$(dirname "$0")/.."

# defer の動作確認のため意図的に panic する(tests/e2e.test.ts に専用テストがある)
EXPECT_FAIL="examples/defer_panic.mesh"

if [ "${1:-}" = "--rust" ]; then
	RUN() { rust/target/debug/mesh run "$1"; }
	WHICH="Rust版"
else
	RUN() { bun src/cli.ts run "$1"; }
	WHICH="TS版"
fi

failed=""
for f in examples/*.mesh; do
	echo "=== $f ==="
	if RUN "$f"; then
		case " $EXPECT_FAIL " in *" $f "*) failed="$failed $f(落ちるはずが成功)" ;; esac
	else
		case " $EXPECT_FAIL " in *" $f "*) ;; *) failed="$failed $f" ;; esac
	fi
done

if [ -n "$failed" ]; then
	echo "[run-examples/$WHICH] 失敗:$failed" >&2
	exit 1
fi
echo "[run-examples/$WHICH] $(ls examples/*.mesh | wc -l) 本すべて期待どおり"
