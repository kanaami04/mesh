// examples の生成JSをスナップショットとして凍結する(TS撤去 段階5)。
//
// **撤去前は `scripts/parity.sh` がTS版の出力と1バイト単位で突き合わせていた**が、
// TS版を消すとその比較相手が居なくなる。代わりに「撤去時点のRust版の出力」を正解として
// 固定し、以降の変更が生成コードを変えたら気づけるようにする。
//
// **ランタイムのprelude(`// ===== Mesh runtime =====` 〜 `// ===== end runtime =====`)は
// 保存しない**——`rust/embedded/runtime.ts` そのものであり、内容は
// `rust/tests/embedded_sync.rs` が別途守っている。24本ぶん重複して抱えると
// スナップショットが読みにくくなるだけなので、**プログラム固有の部分だけ**を凍結する。
//
//   UPDATE_SNAPSHOTS=1 cargo test --test codegen_snapshot   # 意図した変更のとき更新する
//
// 更新したら**差分を必ず読むこと**。生成コードが変わったのに意図が説明できないなら、
// それは退行の可能性が高い。

use std::path::{Path, PathBuf};
use std::process::Command;

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

// preludeを落として、そのプログラム固有の部分だけを取り出す
fn strip_runtime(js: &str) -> String {
    const END: &str = "// ===== end runtime =====";
    match js.find(END) {
        Some(i) => js[i + END.len()..].trim_start_matches('\n').to_string(),
        // 目印が無い = preludeの出し方が変わった。**黙って全文を保存すると気づけない**ので落とす
        None => panic!("生成JSに `{END}` が無い(preludeの出力形式が変わった?)"),
    }
}

#[test]
fn examplesの生成jsが記録どおり() {
    let root = repo_root();
    let snap_dir = root.join("tests/codegen-snapshots");
    let update = std::env::var("UPDATE_SNAPSHOTS").is_ok();
    if update {
        std::fs::create_dir_all(&snap_dir).expect("スナップショット置き場を作れること");
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("examples"))
        .expect("examples/ を読めること")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mesh"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "examples/ が空(パスの解決が壊れている)");

    let out_dir = std::env::temp_dir().join("mesh-codegen-snapshot");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("一時ディレクトリを作れること");

    let mut mismatches = Vec::new();
    for f in &files {
        let stem = f.file_stem().expect("拡張子なしの名前").to_string_lossy().into_owned();
        let js_path = out_dir.join(format!("{stem}.js"));
        // **リポジトリ相対のパスで、リポジトリルートから起動する**。
        // 生成JSは`__iarith`等のエラー位置に**コマンドラインで渡したパスをそのまま埋め込む**ため、
        // 絶対パスを渡すとスナップショットがマシン固有になる(実際にCIで17件落ちて発覚した——
        // 手元では自分の絶対パスと一致していたので気づけなかった)
        let rel = format!("examples/{}", f.file_name().expect("ファイル名").to_string_lossy());
        let out = Command::new(mesh_bin())
            .current_dir(&root)
            .args(["build", &rel, "-o", js_path.to_str().unwrap()])
            .output()
            .expect("mesh build を起動できること");
        assert!(out.status.success(), "build失敗 {}: {}", f.display(), String::from_utf8_lossy(&out.stderr));
        let body = strip_runtime(&std::fs::read_to_string(&js_path).expect("生成JSを読めること"));

        let snap = snap_dir.join(format!("{stem}.js"));
        if update {
            std::fs::write(&snap, &body).expect("スナップショットを書けること");
            continue;
        }
        let expected = std::fs::read_to_string(&snap)
            .unwrap_or_else(|_| panic!("スナップショットが無い: {} (UPDATE_SNAPSHOTS=1 で作る)", snap.display()));
        if expected != body {
            // **名前だけ出しても原因に辿り着けない**ので、最初の差分行を添える
            let first = expected
                .lines()
                .zip(body.lines())
                .find(|(a, b)| a != b)
                .map(|(a, b)| format!("\n    記録: {a}\n    実際: {b}"))
                .unwrap_or_else(|| "\n    (行数が違う)".to_string());
            mismatches.push(format!("{stem}{first}"));
        }
    }
    let _ = std::fs::remove_dir_all(&out_dir);

    assert!(
        mismatches.is_empty(),
        "生成JSが記録と違う: {mismatches:?}\n\
         意図した変更なら `UPDATE_SNAPSHOTS=1 cargo test --test codegen_snapshot` で更新し、\
         **差分を必ず読むこと**(説明できない変化は退行の可能性が高い)"
    );
}
