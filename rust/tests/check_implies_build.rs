// **性質: `mesh check` が診断ゼロなら、コード生成も成功する。**
//
// 「性質検査(オラクルの代わり)」の第2弾(第1弾は `fmt_corpus.rs` の「整形は診断を変えない」)。
// 考え方は `docs/handoff.md`「性質検査(オラクルの代わり)」節が一次情報源:
// TS版を撤去して失ったのは*新しい入力にも答えを出せる相手*なので、
// **答えを知らなくても正誤を判定できる**検査を足していく。この性質は期待値を持たない
// ——「checkが黙ったのにbuildが失敗した」という**自己矛盾**だけを見る。
//
// **なぜ要るか**: Meshは「AIが書き、人間が読む」前提の言語で、`mesh check --json` を
// エージェント向けの構造化診断として出している。**checkが「問題なし」と言ったのにビルドできない**
// のは、その約束そのものが破れている状態になる。
//
// **構造的な理由**: `mesh check` は `full_checker.rs`、コード生成は `checker.rs`
// (codegenが必要とする型情報だけを解く最小リゾルバ)という**別々の実装**を通る。
// 片方だけが知っている機能があると、この性質が破れる。
//
// 判定に `--emit-js`(生成JSを標準出力へ)を使うのは、**JSランタイムも出力ファイルも要らない**ため。
// 実行結果まで見る検査は `fmt_corpus.rs` 側が examples に対して行っている。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

fn silent(args: &[&str]) -> bool {
    Command::new(mesh_bin())
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("mesh を起動できること")
        .success()
}

fn checks_clean(path: &Path) -> bool {
    silent(&["check", path.to_str().expect("パスがUTF-8であること")])
}

fn builds(path: &Path) -> bool {
    silent(&[path.to_str().expect("パスがUTF-8であること"), "--emit-js"])
}

// **いま性質を破っているケース**(リポジトリ相対パス → 理由)。
//
// 増やすときは理由を必ず書くこと。**このリストは両方向に効く**——載せたケースが
// 通るようになったらテストが落ちるので、直したのに消し忘れることはできない。
// さらに整合性テストが「登録ケースは`check`が黙っていること」も要求する
// ——診断が出るようになると性質の前提から外れて黙って対象外へ落ちるため。
//
// 2件はcodegen側の明示的な「未対応」(`codegen.rs` が名指しでErrを返す既知の穴)。
// 1件は`check_assign_target`がフィールド代入の値の型を検査しない検出漏れ
// (TS版は検査していた。`mesh build`だけが落ちる)。
//
// **導入時は4件で、一度8件まで増えてから3件になった**——この増減自体が記録の価値がある。
// 増えたのは`full_checker`側の誤検知を直した回で、**checkが黙るようになった結果
// codegen側の遅れが露出した**から。減ったのは次の回でcodegen側へ`collect_facts`を
// 移植したから。**性質検査は「片方を直すともう片方の遅れが見える」働きをする**。
// 経緯は `docs/handoff.md`「性質検査(オラクルの代わり)」節。
const KNOWN_VIOLATIONS: &[(&str, &str)] = &[
    ("tests/parity/49-pkg-receiver-exported-ok/main.mesh", "codegen: パッケージ修飾レシーバが未対応"),
    ("tests/parity/50-pkg-usage-ok/main.mesh", "codegen: パッケージ修飾の値参照(呼び出しを伴わない)が未対応"),
    ("tests/parity/member-assign-unchecked/main.mesh", "check_assign_targetがフィールド代入の値の型を検査しない(TS版は検査していた)"),
];

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
fn checkが黙ったならコード生成も成功する() {
    let root = repo_root();
    let files = corpus();
    assert!(files.len() > 100, "コーパスの収集が壊れている(集まったのは{}件)", files.len());

    let known: Vec<PathBuf> = KNOWN_VIOLATIONS.iter().map(|(p, _)| root.join(p)).collect();
    let mut covered = 0;
    let mut skipped = 0;
    let mut broken = Vec::new();
    let mut fixed = Vec::new();

    for f in &files {
        // 性質の前提は「診断ゼロ」。診断が出るケースは**この性質の対象外**
        // (出るべき診断が出ているだけなので、buildが通らなくても矛盾ではない)
        if !checks_clean(f) {
            skipped += 1;
            continue;
        }
        covered += 1;
        let ok = builds(f);
        let is_known = known.contains(f);
        if !ok && !is_known {
            broken.push(f.strip_prefix(&root).unwrap_or(f).display().to_string());
        }
        if ok && is_known {
            fixed.push(f.strip_prefix(&root).unwrap_or(f).display().to_string());
        }
    }

    // **黙って絞らない**: 何件を実際に検査し、何件が前提を満たさなかったかを必ず出す
    eprintln!("check通過 {covered} 件を検査 / 診断ありで対象外 {skipped} 件 / 既知の破れ {}", KNOWN_VIOLATIONS.len());

    assert!(broken.is_empty(), "checkは黙ったのにコード生成が失敗した(既知リストに無い):\n  {}", broken.join("\n  "));
    assert!(fixed.is_empty(), "KNOWN_VIOLATIONS に載っているのに通るようになった。直したなら消すこと:\n  {}", fixed.join("\n  "));
}

#[test]
fn 既知の破れリストが腐っていない() {
    // パスのtypoや、ケースの改名でリストが空振りするのを防ぐ。
    //
    // **`checks_clean`も要求するのが要点**——上のテストは「診断が出るケース」を性質の前提外
    // として飛ばすので、登録済みのケースが**診断を出すようになると黙って対象外へ落ち**、
    // リストに残ったまま誰も気づけない。実際に踏んだ: `narrow-then-assign-field` は
    // `full_checker`へ`invalidatePath`を移植した回に診断を出すようになり、リストから
    // 外す必要があった(外し忘れてもどのテストも落ちない状態だった)
    let root = repo_root();
    for (p, reason) in KNOWN_VIOLATIONS {
        let path = root.join(p);
        assert!(path.is_file(), "KNOWN_VIOLATIONS のパスが存在しない: {p}");
        assert!(!reason.is_empty(), "{p} に理由が書かれていない");
        assert!(checks_clean(&path), "{p} は診断を出すので性質の対象外。KNOWN_VIOLATIONS から外すこと(理由: {reason})");
    }
}
