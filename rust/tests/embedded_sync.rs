// `rust/embedded/` に複製した3ファイルが、TS版の原本とズレていないか見張る(TS撤去 段階2)。
//
// Rust版は `mesh card` の本文・`mesh explain` の説明文・生成JSへ埋め込むランタイムを
// **TS版のソースから`include_str!`で取り出していた**。つまり`src/`を消すとRust版が
// **ビルドできなくなる**——単なるオラクル依存ではなく、撤去の実質の最初のブロッカーだった。
//
// 段階2では「Rust側へ複製し、Rustを自己完結させる」方針を採った(TS版を完全に廃止したいため、
// 共有ディレクトリのような中間形態は置かない)。**複製は放っておけば必ず腐る**ので、
// 原本がまだ在る間はこのテストがバイト一致を強制する。
//
// `src/`を削除した後(撤去 段階5)は原本が無くなるので、このテストは自動的に
// 「複製が正典」へ切り替わる(原本が無ければskip)。**そのときこのファイルごと消してよい**。

use std::fs;
use std::path::{Path, PathBuf};

// (rust/embedded/ 内の名前, src/ 内の名前)
const PAIRS: &[(&str, &str)] = &[
    ("runtime.ts", "runtime.ts"),
    ("card.ts", "card.ts"),
    ("diagnostic-codes.ts", "diagnostic-codes.ts"),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("rust/ の親").to_path_buf()
}

#[test]
fn 複製した埋め込みソースが原本と一致している() {
    let root = repo_root();
    let mut checked = 0;
    for (embedded, original) in PAIRS {
        let emb_path = root.join("rust/embedded").join(embedded);
        let src_path = root.join("src").join(original);
        let emb = fs::read_to_string(&emb_path).unwrap_or_else(|e| panic!("{} を読めない: {e}", emb_path.display()));
        // 原本が無い = TS実装を撤去済み。複製が正典になったので比較しない
        let Ok(src) = fs::read_to_string(&src_path) else { continue };
        assert_eq!(
            emb,
            src,
            "{} が原本 {} とズレている。\n\
             TS版がまだ在る間は原本が正典——`cp {} {}` で揃えること\n\
             (複製は放っておけば必ず腐るので、このテストが番人になっている)",
            emb_path.display(),
            src_path.display(),
            src_path.display(),
            emb_path.display()
        );
        checked += 1;
    }
    // **全部skipされたら、このテストはもう何も守っていない**——撤去 段階5が済んだ合図なので、
    // このファイルごと消す(残しておくと「検証している」という誤解だけが残る)
    if checked == 0 {
        eprintln!("note: src/ の原本が無い(TS実装は撤去済み)。rust/tests/embedded_sync.rs は削除してよい");
    }
}
