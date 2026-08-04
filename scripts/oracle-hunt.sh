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
#
# **終了コード**(呼び出し側=自走ループはこれで振る舞いを分ける):
#   0 … オラクルと一致(探索したが差は無かった)
#   1 … `DIFF`(オラクルとの差。**診断のズレと生成JSのズレの両方**を含む)または
#         `UNPARSED`(生成物がパースできない=検証になっていない)
#         ——「診断ゼロなのに生成JSが取れない」もDIFF。系統的な壊れは下の
#         codegenスモークテストが事前に落とすので、ここまで来たら**その形固有の食い違い**
#   2 … 環境の問題(bunが無い / オラクルが壊れている〈check・buildの両方をスモークテストする〉/
#         引数が不正 / gitが使えない)
#   3 … `SKIPPED`(コンパイラ編集中・バイナリが古い。**測っていない**)
#
# **3を「差0件」と読んではいけない**——測っていないので何も言えていない。
# `mise run oracle-hunt`経由だとmiseが3も`ERROR task failed`と表示するが、
# **見送りであって失敗ではない**(出力の`SKIPPED:`で判別する)。
set -uo pipefail
cd "$(dirname "$0")/.."

COUNT="${1:-200}"
SEED="${2:-$(date +%s)}"

# **引数を検証する**。`set -e`を使っていないので、生成器が落ちても素通りして
# 「対象1件・差0件・OK」という**偽の成功**になる(code reviewで発覚。
# `scripts/oracle-hunt.sh abc 123`で再現していた)
case "$COUNT" in '' | *[!0-9]*) echo "error: 件数は正の整数で指定すること(渡された値: '$COUNT')" >&2 && exit 2 ;; esac
case "$SEED" in '' | *[!0-9]*) echo "error: シードは正の整数で指定すること(渡された値: '$SEED')" >&2 && exit 2 ;; esac
[ "$COUNT" -gt 0 ] || {
	echo "error: 件数は1以上で指定すること" >&2
	exit 2
}

# **リポジトリルートから走っていることを確かめる**(`sweep.sh`/`parity.sh`と同じ門番。
# あちらは「実際にセッション中2回踏んだ」と記録している)
if [ ! -d "rust/src" ]; then
	echo "error: リポジトリルートが見つからない(このスクリプトは scripts/ に置いたまま実行すること)" >&2
	exit 2
fi

RUST_BIN=rust/target/debug/mesh
if [ ! -x "$RUST_BIN" ]; then
	echo "error: $RUST_BIN が無い。先に 'cargo build --manifest-path rust/Cargo.toml' を実行すること" >&2
	exit 2
fi

# **コンパイラを編集中なら見送る**(終了コード3=スキップ)。
#
# このスクリプトは**バイナリを読むだけでリビルドしない**ので、編集途中の作業ツリーで走ると
# 「そのとき たまたま `target/debug` にあったバイナリ」を正解として扱ってしまう。
# 自走ループ(`/loop`)から無人で回すと、**編集途中の状態を「オラクルとの差分」として
# 報告する**ことになる——差分の意味が失われる。
#
# **失敗(exit 1)ではなくスキップ(exit 3)にするのが要点**。編集中に「差が出た」と
# 騒がれても意味がないし、「差0件」と報告されるのも嘘になる。呼び出し側が
# 「見送った」と「差が無かった」を区別できるようにする。
#
# 意図的に汚れたツリーで測りたいときは `ORACLE_HUNT_FORCE=1` を付ける。
if [ "${ORACLE_HUNT_FORCE:-}" != "1" ]; then
	# **gitが使えなければ「クリーン」ではなく「確認できない」として止める**(exit 2)。
	# `2>/dev/null`で握りつぶして空文字列を「クリーン」と読むと、gitリポジトリでない場所
	# (tarball展開・`git archive`の出力)やgitが無い環境で**ガードが無音で無効化される**。
	# `.claude/hooks/enforce-code-review.sh`が採っている「確認できないときは常にdenyする」と
	# 同じ原則に揃えた(code reviewで発覚)
	if ! dirty=$(git status --porcelain -- rust/ scripts/ 2>&1); then
		echo "error: git status が失敗した(編集中かどうか確認できないので止める)" >&2
		echo "  $dirty" >&2
		echo "  確認を飛ばして測りたいなら ORACLE_HUNT_FORCE=1 を付ける" >&2
		exit 2
	fi
	if [ -n "$dirty" ]; then
		echo "SKIPPED: rust/ か scripts/ に未コミットの変更がある(編集中とみなして見送る)"
		echo "$dirty" | head -5 | sed 's/^/    /'
		echo "    測りたいなら ORACLE_HUNT_FORCE=1 を付ける"
		exit 3
	fi
	# バイナリがビルド入力より古ければ、直したはずの変更が反映されていない
	# (`cargo`は`eval "$(mise env -s bash)"`が要るので、忘れてビルドせず測る事故が
	# docs/handoff.md「検証の進め方」3.に記録されている)。
	#
	# **`rust/src/*.rs`だけでは足りない**——`rust/embedded/{runtime,card,diagnostic-codes}.ts`は
	# `include_str!`でバイナリへ埋め込まれ、とくに`runtime.ts`は**生成JSに直結する**。
	# `Cargo.toml`/`Cargo.lock`も同様。**コミット済みだがリビルド前**という一瞬が死角だった
	# (未コミットなら上の`git status`が拾うので、危ないのはその一瞬だけ。code reviewで発覚)
	stale=$(find rust/src rust/embedded rust/Cargo.toml rust/Cargo.lock -type f -newer "$RUST_BIN" 2>/dev/null | head -1)
	if [ -n "$stale" ]; then
		echo "SKIPPED: $RUST_BIN がビルド入力より古い($stale の方が新しい)"
		echo "    先に 'cargo build --manifest-path rust/Cargo.toml' を実行すること"
		# **内容を変えずに`touch`だけした場合は、cargoがリビルドしないのでmtimeが進まず
		# 見送りが続く**(同一秒でもナノ秒差で`-newer`は真になる)。無人ループだと
		# 「見送り」を延々と報告し続けることになるので、脱出手段を明示しておく
		echo "    ビルドしても解けないなら、内容が変わっていない可能性がある"
		echo "    (cargoがリビルドせずmtimeが進まない)。その場合は: touch $RUST_BIN"
		exit 3
	fi
fi

# **bunはキャッシュの有無に関わらず要る**。キャッシュがある場合に確認を飛ばすと、
# `bun: command not found`が全件で空出力になり**本物の差分と見分けがつかない**
# (code reviewで発覚: 差1件・FAILという、コンパイラの退行そっくりの出方をしていた)
command -v bun >/dev/null 2>&1 || {
	echo "error: bun が要る(TS版オラクルの実行に使う)。docs/setup.md 参照" >&2
	exit 2
}

# オラクルは固定コミット(`cd7273a^`)に紐づくので**キャッシュして使い回してよい**
# (worktreeと違って古くなることがない)。無ければ復元する。
#
# **完了印(.ready)で判定する**。`src/cli.ts`の存在だけを見ると、`git archive`は成功したが
# `bun install`が失敗した**中途半端なキャッシュ**を再利用してしまう——その状態では
# オラクル側が全件空を返し、**13件の偽DIFF**(実際に再現)や、標本が小さいと**偽のOK**になる。
# どちらも「オラクルが壊れている」ではなく「コンパイラが壊れている」ように見えるのが厄介
ORACLE=/tmp/mesh-ts-oracle
if [ ! -f "$ORACLE/.ready" ]; then
	echo "TS版オラクルを復元中(cd7273a^)..."
	rm -rf "$ORACLE"
	mkdir -p "$ORACLE"
	git archive cd7273a^ src package.json tsconfig.json bun.lock | tar -x -C "$ORACLE" || {
		echo "error: オラクルのソースを取り出せない" >&2
		exit 2
	}
	(cd "$ORACLE" && bun install --frozen-lockfile >/dev/null 2>&1) || {
		echo "error: オラクルの依存を入れられない" >&2
		exit 2
	}
	touch "$ORACLE/.ready"
fi

# **生成JSの「本体」だけを取り出す**(2026-07-31)。ランタイムのprelude部分は比べない
# ——Rust版の`rust/embedded/runtime.ts`は撤去後も変更が入る(2026-07-29に`__panic`の
# 表示を分けた)のに対し、TS版オラクルは`cd7273a^`で凍結されているため、preludeは
# **必ず食い違う**。比べたいのは「同じMeshソースから同じJSプログラムが出るか」。
#
# **なぜ診断だけでは足りないか**: milestone 66で「json structのエンコーダ合成が
# 丸ごと未移植」が見つかったのは生成JSを比べたからで、**診断は一致していた**。
# 同じ類の移植漏れは診断の突き合わせでは構造的に見えない。
# `mise run parity`はコーパスに対してこれをやっているが、**ランダムな入力に対しては
# 誰も比べていなかった**——そこがこの追加で埋まる。
body() { sed -n '/^\/\/ ===== end runtime =====$/,$p'; }
# **ファイルの有無を確かめてから読む**。`body <"$missing" 2>/dev/null`はリダイレクトの
# 準備段階で失敗するので**`2>/dev/null`が効かず**、`No such file or directory`が
# 端末へ漏れる(code reviewで指摘され、buildスモークの実測ログにも実際に出ていた)。
# ビルドが落ちてファイルが出来ない形は「取れない」として扱いたいだけなので、空を返す
body_of() { [ -f "$1" ] && body <"$1" || true; }

# **オラクルが本当に動くかを毎回確かめる**(スモークテスト)。
# 壊れたオラクルは「差分」に化けるので、**比較を始める前に**弾く。
# 既知の入力に対して既知の診断が返ることまで見る——起動しただけでは足りない
SMOKE=$(mktemp -d)
printf 'fn main() {\n\ty: Nope = 1\n\tprint(1)\n}\n' >"$SMOKE/smoke.mesh"
smoke_out=$(cd "$ORACLE" && bun src/cli.ts check "$SMOKE/smoke.mesh" 2>&1)
case "$smoke_out" in *unknown-type*) ;; *)
	rm -rf "$SMOKE"
	echo "error: TS版オラクルが期待どおり動かない(スモークテスト失敗)。キャッシュを消して再実行すること: rm -rf $ORACLE" >&2
	echo "  オラクルの出力: $smoke_out" >&2
	exit 2
	;;
esac

# **codegen側もスモークテストする**(2026-07-31、生成JSの突き合わせを足したときに追加)。
# 上の検査は**診断が出る**プログラムしか使っていないので、オラクルの`build`(コード生成)経路を
# 一度も通らない。生成JSの比較はそこに依存しているので、**壊れていると全件が偽のDIFF**になる
# ——「壊れた環境が、コンパイラの退行そっくりに見える」形で、PR #114がこの類の穴を4つ潰した
# あとの5つ目にあたる(code reviewで指摘された)。
#
# **ここで弾いておくことに意味がある**: 系統的な壊れを事前に落とせるので、ループ中に
# 「診断ゼロなのに生成JSが取れない」が出たら、それは**そのプログラム固有の食い違い**
# (片方だけビルドできない=本物の発見)だと言い切れる。だからあちらはDIFFのままでよい。
printf 'fn main() {\n\tprint(1)\n}\n' >"$SMOKE/ok.mesh"
smoke_rs=$("$RUST_BIN" "$SMOKE/ok.mesh" --emit-js 2>&1 >"$SMOKE/rs.js")
smoke_ts=$(cd "$ORACLE" && bun src/cli.ts build "$SMOKE/ok.mesh" -o "$SMOKE/ts.js" 2>&1 >/dev/null)
smoke_rs_body=$(body_of "$SMOKE/rs.js")
smoke_ts_body=$(body_of "$SMOKE/ts.js")
rm -rf "$SMOKE"
if [ -z "$smoke_rs_body" ] || [ -z "$smoke_ts_body" ]; then
	echo "error: 生成JSのスモークテストに失敗した(診断ゼロのプログラムから本体を取り出せない)" >&2
	echo "  Rust側 ${#smoke_rs_body} 文字 / TS側 ${#smoke_ts_body} 文字" >&2
	[ -n "$smoke_rs" ] && echo "  Rust stderr: $smoke_rs" >&2
	[ -n "$smoke_ts" ] && echo "  TS   stderr: $smoke_ts" >&2
	echo "  目印(// ===== end runtime =====)が消えた可能性もある。キャッシュを消して再実行: rm -rf $ORACLE" >&2
	exit 2
fi

# 差が出た形は**捨てずに残す**。調査もコーパスへの昇格もここから始める
FINDS=$(mktemp -d /tmp/mesh-oracle-finds.XXXXXX)
OUT=$(mktemp -d)
python3 scripts/oracle_hunt_gen.py "$OUT" "$SEED" "$COUNT" || {
	echo "error: 生成器が失敗した" >&2
	rm -rf "$OUT" "$FINDS"
	exit 2
}
# **生成物が本当にあるかを確かめる**。空だとglobが展開されず、ループが
# 「リテラルのglob文字列」1件を検査して両方とも空 → **偽の一致**になる
generated=$(find "$OUT" -name '*.mesh' | wc -l)
if [ "$generated" -ne "$COUNT" ]; then
	echo "error: $COUNT 件を頼んだのに $generated 件しか生成されていない" >&2
	rm -rf "$OUT" "$FINDS"
	exit 2
fi

# **位置(行:桁)まで含めて比べる**。診断コードの集合だけだと比較が弱く、
# 「同じコードが違う場所に出る」ズレを見逃す(TS版には同じ注釈を2回報告する場所がある)。
# 250件で位置込みと集合のみを両方試して差が出ないことを確認したうえで、厳しい側を採った
diags() { grep -oE '[0-9]+:[0-9]+: error\[[a-z-]+\]' | tr '\n' '|'; }

# **生成JSの「本体」だけを取り出す**(2026-07-31)。ランタイムのprelude部分は比べない
# ——Rust版の`rust/embedded/runtime.ts`は撤去後も変更が入る(2026-07-29に`__panic`の
# 表示を分けた)のに対し、TS版オラクルは`cd7273a^`で凍結されているため、preludeは
# **必ず食い違う**。比べたいのは「同じMeshソースから同じJSプログラムが出るか」。
#
# **なぜ診断だけでは足りないか**: milestone 66で「json structのエンコーダ合成が
# 丸ごと未移植」が見つかったのは生成JSを比べたからで、**診断は一致していた**。
# 同じ類の移植漏れは診断の突き合わせでは構造的に見えない。
# `mise run parity`はコーパスに対してこれをやっているが、**ランダムな入力に対しては
# 誰も比べていなかった**——そこがこの追加で埋まる。
diffs=0
skips=0
n=0
# **生成JSを実際に何件比べたか**。診断ゼロのケースだけが対象なので、これを出さないと
# 「差0件」が「比べた結果0件」なのか「1件も比べていない」のか読み手に区別できない
# (docs/handoff.md「0件でしたという報告ほど検証が要る」)
js_compared=0
for f in "$OUT"/*.mesh; do
	n=$((n + 1))
	rs=$("$RUST_BIN" check "$f" 2>&1 | diags)
	# **パースできない生成物は検証になっていない**(milestone 62で19件中3件が空振りしていた)。
	# **字句/構文段階の診断コードは`syntax-error`だけではない**——`diagnostic_codes.rs`には
	# `unexpected-character`/`unknown-escape`/`unterminated-interpolation`/
	# `unterminated-string`もある。`syntax-error`だけ見ていると、字句レベルで壊れた生成物が
	# UNPARSEDに数えられずオラクル比較へ回り、**両方が同じエラーを出して「一致」に見える**
	# (code reviewで発覚。当時の生成器は字句エラーを作らなかったので実害は無かったが、
	# 語彙を広げると踏む)
	case "$rs" in *syntax-error* | *unexpected-character* | *unknown-escape* | *unterminated-interpolation* | *unterminated-string*)
		skips=$((skips + 1))
		echo "UNPARSED $(basename "$f") — パースできない(生成テンプレートの誤り)"
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
		continue
	fi
	# **診断が一致し、かつ両方とも診断ゼロのときだけ生成JSを比べる**。
	# 診断が出るプログラムはそもそもビルドされないので、比較対象が存在しない
	[ -z "$rs" ] || continue
	# **標準エラーは捨てずに拾う**。捨てると「環境が壊れている」と「コンパイラが退行した」を
	# 区別できなくなる——PR #114がこの形の穴を4つ潰し、PR #120でも同じ指摘を受けている
	# (診断の突き合わせ側は元から`2>&1`で本文へ混ぜている)。`2>&1 >file`の順序が要点で、
	# 先にstderrを「今の標準出力=コマンド置換」へ向けてから、stdoutをファイルへ落とす
	rs_js_file="$OUT/.rs_emit.js"
	rs_err=$("$RUST_BIN" "$f" --emit-js 2>&1 >"$rs_js_file")
	js_rs=$(body_of "$rs_js_file")
	rm -f "$rs_js_file"
	# **TS版は`-o`で書き出す形しか無い**ので一時ファイルへ受ける。`-o /dev/stdout`にすると
	# 「wrote /dev/stdout」というステータス行が本文に混ざり、**全件が偽のDIFF**になる
	# (この追加を入れた直後に30件中18件で踏んだ)
	ts_js_file="$OUT/.ts_emit.js"
	ts_err=$(cd "$ORACLE" && bun src/cli.ts build "$f" -o "$ts_js_file" 2>&1 >/dev/null)
	js_ts=$(body_of "$ts_js_file")
	rm -f "$ts_js_file"
	# **どちらかが空なら比較になっていない**(ビルドが落ちた/出力形式が変わった/
	# `// ===== end runtime =====`の目印が消えた)。「両方空だから一致」という**偽の一致**を
	# 作らないよう止める。**このときこそstderrが要る**——原因が環境なのかコンパイラなのかは
	# エラー本文を見ないと決められない
	if [ -z "$js_rs" ] || [ -z "$js_ts" ]; then
		diffs=$((diffs + 1))
		cp "$f" "$FINDS/"
		echo "DIFF $(basename "$f" .mesh) — 診断ゼロなのに生成JSが取れない(Rust側 ${#js_rs} 文字 / TS側 ${#js_ts} 文字)"
		[ -n "$rs_err" ] && printf '    Rust stderr: %s\n' "$(printf '%s' "$rs_err" | head -3 | tr '\n' ' ')"
		[ -n "$ts_err" ] && printf '    TS   stderr: %s\n' "$(printf '%s' "$ts_err" | head -3 | tr '\n' ' ')"
		continue
	fi
	js_compared=$((js_compared + 1))
	if [ "$js_rs" != "$js_ts" ]; then
		diffs=$((diffs + 1))
		cp "$f" "$FINDS/"
		printf '%s\n' "$js_rs" >"$FINDS/$(basename "$f" .mesh).rust.js"
		printf '%s\n' "$js_ts" >"$FINDS/$(basename "$f" .mesh).ts.js"
		echo "DIFF $(basename "$f" .mesh) — 診断は一致したが**生成JSが違う**"
		diff <(printf '%s\n' "$js_ts") <(printf '%s\n' "$js_rs") | head -12 | sed 's/^/    /'
	fi
done

echo "---"
echo "対象 $n 件(うち生成JSも比べた ${js_compared} 件)/ オラクルとの差 ${diffs} 件 / パース失敗 ${skips} 件 / seed=$SEED"
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
