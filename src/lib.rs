//! Meshコンパイラ本体。仕様は docs/spec/ が正。
//!
//! パイプライン(予定): 字句解析 → 構文解析 → 型検査 → JS生成。
//! 実装は docs/spec/ の章順に、tddスキルのRed→Green→Refactorで進める。

pub mod lexer;

/// Meshソースを受け取り、JavaScriptソースを返す。
///
/// まだ何も実装されていない(Phase 2の骨組み)。最初の実装対象は字句解析器。
pub fn compile(source: &str) -> Result<String, String> {
    let _ = source;
    Err("Meshコンパイラはまだ実装されていません(Phase 2 進行中)".to_string())
}
