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
// **milestone 58で、parser.rs/lexer.rsが`CompileError.code: &'static str`として直接持って
// いた構文・字句カテゴリのコードもこの列挙型へ載せた**(milestone 61で2種足して23種)。それらは以前からRust版が
// 出していたのに、enumに載っていないせいで`mesh explain`から静かに欠け、「未移植の診断」と
// 誤って数えられていた——`CompileError.code`自体は`&'static str`のままで、こちらは
// 「Rust版が出せるコードの一覧」を正しく表すことだけを担う。
//
// **この列挙型はTS版の診断コードの部分集合**という不変条件がある(`explain.rs`が説明文を
// TS版の定義から引くため)。TS版に無いRust固有のコード——`interpolation-too-deep`
// (parser.rsの`MAX_INTERP_DEPTH`、Rustだけの安全弁)——は載せない。
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
    IntLiteralOverflow => "int-literal-overflow",
    ArgumentCount => "argument-count",
    BuiltinArgType => "builtin-arg-type",
    BuiltinAsValue => "builtin-as-value",
    PackageNameReserved => "package-name-reserved",
    InvalidPackageName => "invalid-package-name",
    SelfImport => "self-import",
    ImportCycle => "import-cycle",
    UnknownPackage => "unknown-package",
    UnknownPackageType => "unknown-package-type",
    UnknownPackageFunction => "unknown-package-function",
    PackageSymbolIsAType => "package-symbol-is-a-type",
    PackageAsValue => "package-as-value",
    NotExported => "not-exported",
    BuiltinTypeRedeclared => "builtin-type-redeclared",
    NameConflictsWithPackage => "name-conflicts-with-package",
    UnknownField => "unknown-field",
    NotAStruct => "not-a-struct",
    NarrowRequired => "narrow-required",
    MissingFields => "missing-fields",
    DuplicateField => "duplicate-field",
    MethodNotCalled => "method-not-called",
    InvalidReceiverType => "invalid-receiver-type",
    MethodFieldConflict => "method-field-conflict",
    DuplicateMethod => "duplicate-method",
    VoidUsedAsValue => "void-used-as-value",
    MissingReturnValue => "missing-return-value",
    DiscriminatedUnionTagRequired => "discriminated-union-tag-required",
    ReservedFieldName => "reserved-field-name",
    DiscriminatedUnionTagMissing => "discriminated-union-tag-missing",
    DiscriminatedUnionNoMatch => "discriminated-union-no-match",
    DiscriminatedUnionAmbiguous => "discriminated-union-ambiguous",
    InvalidIndexType => "invalid-index-type",
    NotIndexable => "not-indexable",
    NotRangeable => "not-rangeable",
    RangeArity => "range-arity",
    CallbackSignatureMismatch => "callback-signature-mismatch",
    CannotInferType => "cannot-infer-type",
    GenericTypeParamConflict => "generic-type-param-conflict",
    GenericTypeParamNotInferable => "generic-type-param-not-inferable",
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
    UnknownType => "unknown-type",
    AnyTypeRemoved => "any-type-removed",
    TypeAliasCycle => "type-alias-cycle",
    ErrorTypeMustBeStruct => "error-type-must-be-struct",
    ErrorTypeAliasesExisting => "error-type-aliases-existing",
    DeferRequiresCall => "defer-requires-call",
    NotCallable => "not-callable",
    // milestone 59: json structのデコーダ合成が出す2診断。合成は`json_decode.rs`が
    // parse直後に行うので`CompileError`経由で報告されるが、**TS版と同じ診断コード**なので
    // ここにも載せる(`mesh explain`で引けるようにするため)
    JsonStructMissingImport => "json-struct-missing-import",
    JsonStructUnsupportedField => "json-struct-unsupported-field",
    // ---- 構文・字句カテゴリ(milestone 58で統合)----
    // parser.rs/lexer.rsが`CompileError.code`として`&'static str`で直接持っていた分。
    // **これらは以前からRust版が出していた**——enumに載っていなかったので`mesh explain`から
    // 静かに欠けており、「未移植の診断」と誤って数えられていた(milestone 58の調査で発覚)。
    //
    // **`interpolation-too-deep`だけは載せない**。TS版に無いRust固有の安全弁
    // (文字列補間のネスト上限。parser.rsの`MAX_INTERP_DEPTH`参照)で、`explain`の説明文は
    // TS版の定義から引く仕組みなので載せると`すべての診断コードに説明文がある`テストが落ちる
    // ——「このenumはTS版の診断コードの部分集合」という不変条件を保つ
    BareStructShape => "bare-struct-shape",
    ChanCapacityRequired => "chan-capacity-required",
    EmptyInterpolation => "empty-interpolation",
    EmptyTypedArrayLiteralRemoved => "empty-typed-array-literal-removed",
    FnTypeWithParamNames => "fn-type-with-param-names",
    ImportOrder => "import-order",
    InterpolationInType => "interpolation-in-type",
    InvalidAssignmentTarget => "invalid-assignment-target",
    InvalidImportPath => "invalid-import-path",
    InvalidSpawnTarget => "invalid-spawn-target",
    InvalidTopLevelDeclaration => "invalid-top-level-declaration",
    JsonTypeNotSupported => "json-type-not-supported",
    MethodExportRedundant => "method-export-redundant",
    MisplacedMut => "misplaced-mut",
    MultipleReturnValuesRemoved => "multiple-return-values-removed",
    MultipleSelectDefaults => "multiple-select-defaults",
    PostfixBangRenamed => "postfix-bang-renamed",
    SyntaxError => "syntax-error",
    TopLevelMutNotAllowed => "top-level-mut-not-allowed",
    UnexpectedCharacter => "unexpected-character",
    UnknownEscape => "unknown-escape",
    UnterminatedInterpolation => "unterminated-interpolation",
    UnterminatedString => "unterminated-string",
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
