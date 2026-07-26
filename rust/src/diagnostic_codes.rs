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
// `DIAGNOSTIC_EXPLANATIONS`(`mesh explain`用の説明文マップ、TS版後半)はmilestone 38で
// `explain.rs`が扱うようになった——本文はRust側へ複製せず、TS版の定義から取り出している
// (理由はexplain.rsの冒頭コメント)。説明を出す範囲は`ALL`(=このenum)に絞る。

use crate::token::Pos;

// enum・`ALL`・`as_str`は**1つの表から生成する**。手で3箇所に書き分けると、変体を足したときの
// `ALL`への追加漏れ(=その診断が`mesh explain`から静かに欠ける)が起きる——`as_str`の網羅性は
// コンパイラが検査してくれるが、定数配列の網羅性は誰も検査してくれないため。
// `macro_rules!`は「コンパイル前にコードを展開する型付きテンプレート」で、ここでは
// `変体名 => "文字列",`の並びを受け取ってenum定義・`ALL`・`as_str`の3つへ同時に展開している。
macro_rules! diagnostic_codes {
    ($($variant:ident => $text:literal,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum DiagnosticCode {
            $($variant,)*
        }

        impl DiagnosticCode {
            // 実装済みの診断コード全部(宣言順)。`mesh explain`(explain.rs)が
            // 「Rust版が出せる範囲」を知るために使う
            pub const ALL: &'static [DiagnosticCode] = &[$(DiagnosticCode::$variant,)*];

            // TS版DiagnosticCodeの文字列リテラルと同じ表記(`mesh check --json`の code フィールド用)
            pub fn as_str(self) -> &'static str {
                match self {
                    $(DiagnosticCode::$variant => $text,)*
                }
            }
        }
    };
}

diagnostic_codes! {
    ReservedWord => "reserved-word",
    BuiltinRedeclared => "builtin-redeclared",
    AlreadyDeclared => "already-declared",
    Shadowing => "shadowing",
    UndefinedName => "undefined-name",
    TypeMismatch => "type-mismatch",
    ImmutableAssignment => "immutable-assignment",
    MissingMain => "missing-main",
    InvalidMainSignature => "invalid-main-signature",
    InvalidOperation => "invalid-operation",
    IncomparableTypes => "incomparable-types",
    NotBool => "not-bool",
    UseIsNone => "use-is-none",
    DivisionByZero => "division-by-zero",
    ArgumentCount => "argument-count",
    BuiltinArgType => "builtin-arg-type",
    UnknownField => "unknown-field",
    MissingFields => "missing-fields",
    DuplicateField => "duplicate-field",
    MethodNotCalled => "method-not-called",
    InvalidReceiverType => "invalid-receiver-type",
    MethodFieldConflict => "method-field-conflict",
    DuplicateMethod => "duplicate-method",
    VoidUsedAsValue => "void-used-as-value",
    DiscriminatedUnionTagMissing => "discriminated-union-tag-missing",
    DiscriminatedUnionNoMatch => "discriminated-union-no-match",
    DiscriminatedUnionAmbiguous => "discriminated-union-ambiguous",
    InvalidIndexType => "invalid-index-type",
    NotIndexable => "not-indexable",
    NotRangeable => "not-rangeable",
    RangeArity => "range-arity",
    CallbackSignatureMismatch => "callback-signature-mismatch",
    CannotInferType => "cannot-infer-type",
    NotAChannel => "not-a-channel",
    CompoundAssignOnMap => "compound-assign-on-map",
    InvalidTestSignature => "invalid-test-signature",
    EmptyMatch => "empty-match",
    UnionRequired => "union-required",
    ImpossiblePattern => "impossible-pattern",
    UnreachablePattern => "unreachable-pattern",
    WildcardNotAlone => "wildcard-not-alone",
    MatchNotExhaustive => "match-not-exhaustive",
    MixedVoidArms => "mixed-void-arms",
    EmptySelect => "empty-select",
    PropContextNotString => "prop-context-not-string",
    PropRequiresFailureUnion => "prop-requires-failure-union",
    PropNothingToPropagate => "prop-nothing-to-propagate",
    PropContextStructuredError => "prop-context-structured-error",
    PropReturnTypeMismatch => "prop-return-type-mismatch",
    OrNeverFails => "or-never-fails",
    OrRequiresBinding => "or-requires-binding",
    OrNoSuccessValue => "or-no-success-value",
    OrFallbackTypeMismatch => "or-fallback-type-mismatch",
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
