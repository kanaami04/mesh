#!/usr/bin/env bash
# クラウド常駐のオラクル差分ハンター用ラッパー(1周回ぶん)。
#
# `scripts/oracle-hunt.sh` を1回実行し、結果を**永続ログ**へ追記したうえで、
# 呼び出し元(スケジュール経由で毎回新しく起動されるエージェント)が
# 機械的に振る舞いを分けられるよう末尾に `RESULT: ...` 行を出す。
#
# ログを `/tmp` のセッション固有スクラッチに置かないのが要点——クラウドスケジュールは
# 毎回別セッションとして起動されるため、セッションIDに紐づく場所へ書くと
# 前回までの累計が読めなくなる。`$HOME` 配下の固定パスに置き、再起動やセッション交代を
# またいで読み書きできるようにする。
set -uo pipefail
cd "$(dirname "$0")/.."

LOG_DIR="${MESH_ORACLE_HUNT_LOG_DIR:-$HOME/.mesh-oracle-hunt}"
LOG_FILE="$LOG_DIR/hunt.log"
mkdir -p "$LOG_DIR"

COUNT="${1:-300}"
SEED="${2:-$(date +%s%N | tail -c 9)}"
TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

eval "$(mise env -s bash)" 2>/dev/null || true

OUTPUT="$(scripts/oracle-hunt.sh "$COUNT" "$SEED" 2>&1)"
STATUS=$?

# oracle-hunt.sh の終了コードは DIFF と UNPARSED をどちらも exit 1 にまとめているので、
# ログ・報告の粒度を保つためにここで出力から見分ける
case $STATUS in
0) RESULT=OK ;;
3) RESULT=SKIPPED ;;
2) RESULT=ENV_ERROR ;;
1)
	if printf '%s\n' "$OUTPUT" | grep -q '^DIFF '; then
		RESULT=DIFF
	else
		RESULT=UNPARSED
	fi
	;;
*) RESULT="UNKNOWN_EXIT_$STATUS" ;;
esac

printf '%s\t%s\t%s\t%s\n' "$TS" "$SEED" "$COUNT" "$RESULT" >>"$LOG_FILE"

printf '%s\n' "$OUTPUT"
echo "---"
echo "LOG: $LOG_FILE"
echo "SEED: $SEED"
echo "RESULT: $RESULT"

exit "$STATUS"
