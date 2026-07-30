// **性質: 診断ゼロでビルドできたプログラムを実行しても、コンパイラのバグでは落ちない。**
//
// 性質検査(オラクルの代わり)の第3弾。第1弾は `fmt_corpus.rs`(整形は診断を変えない)、
// 第2弾は `check_implies_build.rs`(checkが黙ったならコード生成も成功する)。
// 考え方は `docs/handoff.md`「性質検査(オラクルの代わり)」節が一次情報源。
//
// ## なぜ「その先」が要るのか
//
// `check_implies_build` は**コード生成が成功すること**までしか見ない。生成されたJSが
// 実行できるかは別の話で、実際に**そこに穴があった**: 非mainパッケージのトップレベル
// constを参照するコード生成のバグ(milestone 68)は、`check`も`build`も通り抜けて
// 実行時に `x is not defined` で落ちていた。**うるさく失敗しているのに、誰も実行して
// いなかったから気づけなかった**という形。
//
// ## 期待値を持たない判定の置き方
//
// 「実行して期待どおりの結果を出す」をそのまま検査にすると、「期待どおり」の答えが必要に
// なってオラクル非依存でなくなる(記録にすると`--update`の抜け道も戻ってくる)。そこで
// **答えを知らなくても判定できる形**へ言い換える:
//
//   Meshのpanic(範囲外アクセス・整数オーバーフロー等)は**仕様内の停止**。
//   一方、生成JSが投げた素のエラー(`ReferenceError`等)は**コンパイラのバグ**しかありえない
//   ——標準ライブラリの失敗経路はすべて`error`値へ変換しているため(runtime.ts参照)。
//
// ランタイム側(`rust/embedded/runtime.ts`の`__panic`)が前者を `panic:`、後者を
// `internal error:` と**表示し分ける**ようにしたので、この検査は「`internal error:` が
// 出ないこと」だけを見ればよい。プログラムが何を出力するかは一切知らなくてよい。
//
// ## 前提を満たさないものは黙って飛ばす(件数は必ず出す)
//
// - `check`が診断を出すケース(`tests/parity/`の大半)は**この性質の対象外**。
//   ビルドできないのは正しい振る舞いなので矛盾ではない。
// - JSランタイム(bun/node)が無い環境ではスキップする。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn mesh_bin() -> PathBuf {
    let mut p = std::env::current_exe().expect("テストバイナリのパス");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("mesh")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("rust/ の親").to_path_buf()
}

fn js_runtime_available() -> bool {
    ["bun", "node"].iter().any(|r| Command::new(r).arg("--version").output().is_ok())
}

fn checks_clean(path: &Path) -> bool {
    Command::new(mesh_bin())
        .args(["check", path.to_str().expect("パスがUTF-8であること")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("mesh を起動できること")
        .success()
}

// 実行結果のうち、この性質が見るものだけ。**標準出力は見ない**(期待値を持たないため)
struct RunOutcome {
    stderr: String,
    timed_out: bool,
}

// **`mesh run`を時間制限つきで走らせる**。制限が無いと、チャネル待ちやサーバ待機で
// 止まるプログラムを1本混ぜただけでCIが無限に待つ(コーパスは今後も増える)。
// 30秒はコーパス最長(examplesの並行処理もの)の100倍以上の余裕がある。
//
// **標準エラーはパイプではなく一時ファイルへ受ける**。パイプだと2つ困る(code reviewの指摘):
// (1) ポーリング中は誰も読まないので、OSのパイプバッファ(Linuxで64KB程度)が埋まると
//     子プロセスが`write`でブロックし、**出力が多いだけのプログラムがタイムアウトに化ける**。
// (2) タイムアウトで打ち切ったとき、パイプの中身を捨てることになる——本物の
//     `internal error:`が「30秒で終わらなかった」という別の話に見えてしまう。
// ファイルなら上限が無く、kill後でも読める。
fn run_with_timeout(path: &Path) -> RunOutcome {
    let err_path = std::env::temp_dir().join(format!(
        "mesh-build-implies-run-{}.err",
        path.to_string_lossy().replace(['/', '\\', ':'], "_")
    ));
    let err_file = std::fs::File::create(&err_path).expect("一時ファイルを作れること");
    let mut child = Command::new(mesh_bin())
        .args(["run", path.to_str().expect("パスがUTF-8であること")])
        .current_dir(repo_root()) // 相対パスを解くのはリポジトリルート基準(examplesがio.readFileを使う)
        .stdout(std::process::Stdio::null())
        .stderr(err_file)
        .spawn()
        .expect("mesh を起動できること");

    // `wait_timeout`クレートを足さずに済ませる素朴なポーリング(20ms刻み・最大30秒)。
    // 検査対象は数十件なので、精度より依存を増やさないことを優先する
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut timed_out = false;
    loop {
        match child.try_wait().expect("子プロセスの状態を取れること") {
            Some(_) => break,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    // **タイムアウトでも読む**(上記(2))。読めなければ空として扱う
    let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&err_path);
    RunOutcome { stderr, timed_out }
}

fn corpus() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("examples"))
        .expect("examples/ を読めること")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mesh"))
        .collect();
    files.extend(
        std::fs::read_dir(root.join("tests/parity"))
            .expect("tests/parity/ を読めること")
            .flatten()
            .map(|e| e.path().join("main.mesh"))
            .filter(|p| p.is_file()),
    );
    // `fmt_corpus.rs` が検査対象の隣に置く一時ファイルを拾わない(並行実行するため)
    files.retain(|p| !p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("__")));
    files.sort();
    files
}

#[test]
fn 診断ゼロで実行できたプログラムはコンパイラのバグで落ちない() {
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため build⇒run の性質検査をスキップ");
        return;
    }
    let root = repo_root();
    let files = corpus();
    assert!(files.len() > 100, "コーパスの収集が壊れている(集まったのは{}件)", files.len());

    let mut covered = 0;
    let mut skipped = 0;
    let mut broken = Vec::new();

    for f in &files {
        if !checks_clean(f) {
            skipped += 1;
            continue;
        }
        covered += 1;
        let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
        let outcome = run_with_timeout(f);
        if outcome.timed_out {
            // 打ち切ったときも標準エラーは読めているので、原因の手がかりを一緒に出す
            let tail = outcome.stderr.lines().last().unwrap_or("(標準エラーは空)");
            broken.push(format!("{rel} — 30秒で終わらなかった(停止しないプログラムはコーパスに置かない)。最後の行: {tail}"));
            continue;
        }
        // **これが判定のすべて**。Meshのpanic(`panic:`)は仕様内なので通す——
        // `examples/defer_panic.mesh` は範囲外アクセスでpanicすることが目的のexample
        if let Some(line) = outcome.stderr.lines().find(|l| l.contains("internal error:")) {
            broken.push(format!("{rel} — {}", line.trim()));
        }
    }

    // **黙って絞らない**: 何件を実際に実行し、何件が前提を満たさなかったかを必ず出す
    // (「0件でした」が空振りでないことを読み手が確かめられるように)
    eprintln!("実行して検査 {covered} 件 / 診断ありで対象外 {skipped} 件");
    assert!(covered > 20, "実行できたケースが少なすぎる({covered}件)——前提の判定が壊れている可能性");

    assert!(
        broken.is_empty(),
        "診断ゼロでビルドできたのに、生成JSがコンパイラのバグで落ちた:\n  {}",
        broken.join("\n  ")
    );
}

// 出荷するランタイムをそのまま取り出す(`--emit-js`の出力の先頭にPRELUDEが入っている)。
// **本物を測るのが要点**——PRELUDEを別に書き写すと、写しの側だけ直して安心する形になる
fn shipped_runtime() -> String {
    let probe = std::env::temp_dir().join("mesh-runtime-probe.mesh");
    std::fs::write(&probe, "fn main() {\n\tprint(1)\n}\n").expect("プローブを書けること");
    let out = Command::new(mesh_bin())
        .args([probe.to_str().expect("パスがUTF-8であること"), "--emit-js"])
        .output()
        .expect("mesh を起動できること");
    assert!(out.status.success(), "プローブのビルドが失敗した: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("生成JSがUTF-8であること")
}

// ランタイムの`__panic`へ直接エラーを渡し、標準エラーへ何が出るかを見る。
// `__panic`は`process.exit(1)`を呼ぶので1件ずつ別プロセスで走らせる
fn panic_label_for(throw_expr: &str) -> String {
    let js = format!("{}\n__panic({throw_expr});\n", shipped_runtime());
    let path = std::env::temp_dir().join(format!("mesh-panic-label-{}.mjs", throw_expr.len()));
    std::fs::write(&path, js).expect("一時JSを書けること");
    let runtime = if Command::new("bun").arg("--version").output().is_ok() { "bun" } else { "node" };
    let out = Command::new(runtime).arg(&path).output().expect("JSランタイムを起動できること");
    let _ = std::fs::remove_file(&path);
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn 仕様内のpanicとコンパイラのバグを表示で見分けられる() {
    // **判定の土台そのものの検査**。上のテストは`internal error:`という印だけを見るので、
    // ランタイムがこの印を出さなくなると**静かに何も検査しなくなる**(「0件でした」が
    // 見る目の無さから来る、という handoff が繰り返し警告している形)。
    //
    // **最初はこれを「仕様内のpanicに`internal error:`が付かないこと」だけで書いて失敗した**
    // ——`internal = false`と壊しても仕様内のpanicは正しいままなので、両方のテストが
    // 素通りした。**印が出る方向を測らないと守りにならない**、というのが実地の教訓。
    // そこでランタイムの`__panic`へ直接エラーを渡し、両方向を固定する。
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため表示の検査をスキップ");
        return;
    }

    // (1) 素のJSエラー(= 生成JSが壊れている)は internal error として出る
    let foreign = panic_label_for("new Error(\"boom\")");
    assert!(foreign.contains("internal error:"), "素のJSエラーがコンパイラのバグとして報告されていない: {foreign}");
    assert!(foreign.contains("boom"), "元のメッセージが失われている: {foreign}");
    assert!(!foreign.contains("panic: boom"), "素のJSエラーがMeshのpanicとして報告されている: {foreign}");

    // (2) Meshのpanic(仕様内)は panic: のまま
    let spec = panic_label_for("new __Panic(\"oops\")");
    assert!(spec.contains("panic: oops"), "仕様内のpanicが`panic:`で出ていない: {spec}");
    assert!(!spec.contains("internal error:"), "仕様内のpanicがコンパイラのバグとして報告されている: {spec}");

    // (2') **ホストの限界(スタック溢れ)も仕様内**。ユーザー自身の深い再帰なので
    // コンパイラのバグにしてはいけない——code reviewで実測して見つかった誤ラベル。
    // 「`__Panic`以外はコンパイラのバグ」という素朴な判定だとここを踏む
    let limit = panic_label_for("new RangeError(\"Maximum call stack size exceeded\")");
    assert!(limit.contains("panic:"), "スタック溢れが`panic:`で出ていない: {limit}");
    assert!(!limit.contains("internal error:"), "ユーザーの深い再帰がコンパイラのバグとして報告されている: {limit}");
    assert!(limit.contains("tail-call"), "対処のヒントが出ていない: {limit}");

    // (3) 実際のプログラムでも仕様内のpanicが誤ラベルされない(範囲外アクセスするexample)
    let outcome = run_with_timeout(&repo_root().join("examples/defer_panic.mesh"));
    assert!(!outcome.timed_out, "defer_panic.mesh が終わらない");
    assert!(outcome.stderr.contains("panic:"), "仕様内のpanicが`panic:`で出ていない: {}", outcome.stderr);
    assert!(!outcome.stderr.contains("internal error:"), "仕様内のpanicが誤ラベルされている: {}", outcome.stderr);
}

#[test]
fn 誰のせいかの判定は3つの経路すべてに効く() {
    // **同じ判定を必要とする経路が3つある**——`__panic`(トップレベル・spawn/detach)、
    // `__httpDispatch`(リクエストごとの障害分離)、`__runTests`(テスト1件ごと)。
    // **最初は`__panic`だけ分けてcode reviewに指摘された**(PR #113の「同じ構文を扱う
    // 別経路を片方だけ直す」の再現)。判定は`__isCompilerBug`/`__blame`へ閉じてあるので、
    // ここではその2つを**3経路が実際に使っていること**を出荷物の上で確かめる。
    if !js_runtime_available() {
        eprintln!("skip: bun/node が見つからないため経路の検査をスキップ");
        return;
    }
    let js = shipped_runtime();
    for (site, needle) in [
        ("__panic", "__panicSink.push(__blame(e))"),
        ("__httpDispatch", "(isolated to this request)"),
        ("__runTests", "message: __blame(e)"),
    ] {
        assert!(js.contains(needle), "{site} が共通の判定を通っていない(出荷したランタイムに `{needle}` が無い)");
    }
    // `__httpDispatch`だけは文字列を組み立てるので、判定関数を使っていることも見る
    assert!(
        js.contains("__isCompilerBug(e) ? \"internal error\" : \"panic\""),
        "__httpDispatch が独自の文言を組み立てている(判定を共有していない)"
    );
}
