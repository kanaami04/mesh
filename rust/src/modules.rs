// 複数ファイル/複数パッケージの発見。TS版`cli.ts`の`loadModules`/`loadDependencies`の移植。
// ここはファイルI/O層の処理であり、checker.rs/codegen.rsの「診断を出さない」設計とは
// 無関係——存在しないディレクトリ・空パッケージ・ネストしたパスは単純な明確なErr
// (TS版のconsole.error+process.exitに相当)。stdlib(mesh/io等)はまだ実装が無いため対象外

use crate::parser::parse;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ModuleSource {
    pub pkg: String,
    pub file: PathBuf,
    pub source: String,
}

// importの発見だけのための軽量パース。構文エラーは無視する——本格的なエラー報告は
// 後続の本パース(呼び出し元がModuleSourceごとに行う)に委ねる(TS版importsOfと同じ)
fn imports_of(source: &str) -> Vec<String> {
    parse(source).map(|p| p.imports.iter().map(|i| i.path.clone()).collect()).unwrap_or_default()
}

// エントリファイルと、そこから(推移的に)importされたパッケージのソースを集める。
// プロジェクトルート = エントリファイルのディレクトリ。パッケージ = ルート直下の
// ディレクトリで、その中の全.meshファイルが1パッケージの名前空間を成す
// (エントリ自身は"main"の1ファイルのみ——同じディレクトリの他の.meshファイルは
// 含めない。TS版cli.ts:loadModulesのコメント参照)
pub fn load_modules(entry_file: &Path) -> Result<Vec<ModuleSource>, String> {
    let entry_source =
        fs::read_to_string(entry_file).map_err(|e| format!("failed to read {}: {e}", entry_file.display()))?;
    let root = entry_file.parent().unwrap_or_else(|| Path::new("."));
    let initial_queue = imports_of(&entry_source);
    let mut modules = vec![ModuleSource { pkg: "main".to_string(), file: entry_file.to_path_buf(), source: entry_source }];
    modules.extend(load_dependencies(root, initial_queue)?);
    Ok(modules)
}

// importグラフを再帰的に(BFSキューで)辿ってパッケージのソースを集める。`loaded`で
// 再訪問を防ぐので、パッケージ間の循環があってもここでは無限ループにならない
// (単に両方1回ずつ読むだけ——処理順を決める依存グラフの循環検出はcodegen側の仕事)
fn load_dependencies(root: &Path, initial_queue: Vec<String>) -> Result<Vec<ModuleSource>, String> {
    let mut modules = Vec::new();
    let mut loaded: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = initial_queue.into();
    while let Some(path) = queue.pop_front() {
        if !loaded.insert(path.clone()) {
            continue;
        }
        if path.contains('/') {
            return Err(format!(
                "error: nested package paths ('{path}') are not supported yet — packages are single directories under the project root"
            ));
        }
        let dir = root.join(&path);
        if !dir.is_dir() {
            return Err(format!("error: cannot find package '{path}' (expected directory '{}' with .mesh files)", dir.display()));
        }
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .map_err(|e| format!("failed to read directory {}: {e}", dir.display()))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("mesh"))
            .collect();
        files.sort();
        if files.is_empty() {
            return Err(format!("error: package '{path}' has no .mesh files (in '{}')", dir.display()));
        }
        for file in files {
            let source = fs::read_to_string(&file).map_err(|e| format!("failed to read {}: {e}", file.display()))?;
            queue.extend(imports_of(&source));
            modules.push(ModuleSource { pkg: path.clone(), file, source });
        }
    }
    Ok(modules)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 各テストが自分専用の一時ディレクトリを持てるよう、テスト名+プロセスIDで分ける
    // (cargo testはデフォルトで並行実行されるため、テスト間で衝突しないようにする)
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mesh_modules_test_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_modulesはエントリ1ファイルだけをmainパッケージにする() {
        let root = temp_dir("entry_only");
        fs::write(root.join("main.mesh"), "fn main() {}\n").unwrap();
        fs::write(root.join("other.mesh"), "fn unused() {}\n").unwrap(); // 同ディレクトリの無関係ファイル
        let modules = load_modules(&root.join("main.mesh")).unwrap();
        assert_eq!(modules.len(), 1, "mainパッケージはエントリファイルのみを含むべき");
        assert_eq!(modules[0].pkg, "main");
    }

    #[test]
    fn load_modulesは同一パッケージの複数ファイルを読み込む() {
        let root = temp_dir("multi_file_pkg");
        fs::write(root.join("main.mesh"), "import \"mathutil\"\nfn main() {}\n").unwrap();
        let pkg_dir = root.join("mathutil");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("a.mesh"), "export fn a() int { return 1 }\n").unwrap();
        fs::write(pkg_dir.join("b.mesh"), "export fn b() int { return 2 }\n").unwrap();
        let modules = load_modules(&root.join("main.mesh")).unwrap();
        assert_eq!(modules.len(), 3, "main1件+mathutilの2ファイル");
        let mathutil_files: Vec<_> = modules.iter().filter(|m| m.pkg == "mathutil").collect();
        assert_eq!(mathutil_files.len(), 2);
    }

    #[test]
    fn load_modulesは存在しないパッケージを明確なエラーにする() {
        let root = temp_dir("missing_pkg");
        fs::write(root.join("main.mesh"), "import \"doesnotexist\"\nfn main() {}\n").unwrap();
        let err = load_modules(&root.join("main.mesh")).unwrap_err();
        assert!(err.contains("cannot find package"), "got: {err}");
    }

    #[test]
    fn load_modulesは空パッケージを明確なエラーにする() {
        let root = temp_dir("empty_pkg");
        fs::write(root.join("main.mesh"), "import \"emptypkg\"\nfn main() {}\n").unwrap();
        fs::create_dir_all(root.join("emptypkg")).unwrap();
        let err = load_modules(&root.join("main.mesh")).unwrap_err();
        assert!(err.contains("has no .mesh files"), "got: {err}");
    }

    #[test]
    fn load_modulesはネストしたパッケージパスを明確なエラーにする() {
        let root = temp_dir("nested_path");
        fs::write(root.join("main.mesh"), "import \"a/b\"\nfn main() {}\n").unwrap();
        let err = load_modules(&root.join("main.mesh")).unwrap_err();
        assert!(err.contains("nested package paths"), "got: {err}");
    }
}
