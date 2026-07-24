// 診断コード(milestone 22・フルchecker移植の第一歩): TS版`src/diagnostic-codes.ts`
// (107種)からの部分移植。この一歩では新設のfull_checker.rsが実際に出す診断だけを
// 移植する——残り100種は対応する検査を後続milestoneで足すたびに追加していく
// (docs/handoff.md「次のフェーズ: フルchecker移植」節で「全107件を先に埋めるか」を
// 未決としていた点への回答)。**注**: 未使用のpub変体自体はこのenumがpub modの
// 公開APIとして扱われるため`cargo clippy -- -D warnings`のdead-code警告にはならない
// (実測済み——先に全107件定義しても即座にビルドが壊れるわけではない)。それでも
// 検査が存在しないコードを先回りして定義しないのは、実装のないenum変体が
// 「対応する検査があるはず」という誤解を招く(実際に検査を書くまで存在しない
// ことにしておいた方が正確)ため、という設計判断として選んでいる。
//
// 同じ理由で、既存のparser.rs/lexer.rsが`CompileError.code: &'static str`として
// 直接持っている診断コード群(構文・字句カテゴリ、token.rsのコメント参照)も、
// この列挙型へはまだ統合していない——統合は、そちら側のコードを実際にこの型へ
// 移行するタイミングで行う。
//
// `DIAGNOSTIC_EXPLANATIONS`(`mesh explain`用の説明文マップ、TS版後半)はまだ
// `mesh explain`自体が無いため今回は移植しない(そのCLIサブコマンドを生やす
// milestoneで一緒に持ってくる)。

use crate::token::Pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    ReservedWord,
    BuiltinRedeclared,
    AlreadyDeclared,
    Shadowing,
    UndefinedName,
    TypeMismatch,
    ImmutableAssignment,
    MissingMain,
    InvalidMainSignature,
    InvalidOperation,
    IncomparableTypes,
    NotBool,
    UseIsNone,
    DivisionByZero,
    ArgumentCount,
    BuiltinArgType,
    UnknownField,
    MissingFields,
    DuplicateField,
    MethodNotCalled,
}

impl DiagnosticCode {
    // TS版DiagnosticCodeの文字列リテラルと同じ表記(`mesh check --json`の code フィールド用)
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::ReservedWord => "reserved-word",
            DiagnosticCode::BuiltinRedeclared => "builtin-redeclared",
            DiagnosticCode::AlreadyDeclared => "already-declared",
            DiagnosticCode::Shadowing => "shadowing",
            DiagnosticCode::UndefinedName => "undefined-name",
            DiagnosticCode::TypeMismatch => "type-mismatch",
            DiagnosticCode::ImmutableAssignment => "immutable-assignment",
            DiagnosticCode::MissingMain => "missing-main",
            DiagnosticCode::InvalidMainSignature => "invalid-main-signature",
            DiagnosticCode::InvalidOperation => "invalid-operation",
            DiagnosticCode::IncomparableTypes => "incomparable-types",
            DiagnosticCode::NotBool => "not-bool",
            DiagnosticCode::UseIsNone => "use-is-none",
            DiagnosticCode::DivisionByZero => "division-by-zero",
            DiagnosticCode::ArgumentCount => "argument-count",
            DiagnosticCode::BuiltinArgType => "builtin-arg-type",
            DiagnosticCode::UnknownField => "unknown-field",
            DiagnosticCode::MissingFields => "missing-fields",
            DiagnosticCode::DuplicateField => "duplicate-field",
            DiagnosticCode::MethodNotCalled => "method-not-called",
        }
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// TS版`Diagnostic`インターフェースのfull-checker移植分。`file`(複数ファイル区別用)と
// `fix`(機械適用可能な自動修正)はmilestone 22のスコープ外(単一ファイル・パッケージ無し・
// fix無し診断のみ)なので、対応する機能を足すタイミングでフィールドごと追加する。
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub pos: Pos,
    pub code: DiagnosticCode,
    pub message: String,
}
