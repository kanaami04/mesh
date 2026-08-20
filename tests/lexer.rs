//! 字句解析器(lexer)の統合テスト。
//! docs/spec/01-lexical.md を正とし、TDDサイクルごとに1振る舞いずつ追加する。
//! 書き方はAAAパターン+1テスト1assert(規約: .claude/skills/test-writing/SKILL.md)。

use mesh::lexer::{ErrorCode, LexError, Span, StrSegment, Token, TokenKind, lex};

/// 空のソース文字列を字句解析すると、空のトークン列が返ること。
#[test]
fn empty_source_produces_no_tokens() {
    // Arrange
    let source = "";

    // Act
    let tokens = lex(source).expect("空入力の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(tokens, Vec::<Token>::new());
}

/// 10進整数リテラルが1個のIntトークンとして切り出されること(仕様1章1.6)。
#[test]
fn decimal_integer_literal_produces_single_int_token() {
    // Arrange
    let source = "42";

    // Act
    let tokens = lex(source).expect("整数リテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "42".to_string(),
            span: Span { start: 0, end: 2 },
        }]
    );
}

/// 空白類で区切られた整数リテラルが独立したIntトークンになり、
/// 空白類自体はトークンにならないこと(仕様1章1.2: スペース・タブ)。
/// 入力はスペースとタブの両方を含む代表例。
#[test]
fn whitespace_separates_integer_literals() {
    // Arrange
    let source = "1 \t22";

    // Act
    let tokens = lex(source).expect("空白類区切りの整数リテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Int,
                text: "22".to_string(),
                span: Span { start: 3, end: 5 },
            },
        ]
    );
}

/// 予約語 `let` が識別子と区別されてKwLetトークンになること(仕様1章1.4・1.5)。
/// 検証項目はトークン**種類**の区別のみ(text・spanの固定は
/// tokens_carry_byte_offset_spans が同型入力で担うため、重複を避けて種類列に絞る)。
#[test]
fn keyword_let_is_distinguished_from_identifiers() {
    // Arrange
    let source = "let x = 1";

    // Act
    let tokens = lex(source).expect("`let x = 1` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
        vec![
            TokenKind::KwLet,
            TokenKind::Ident,
            TokenKind::Eq,
            TokenKind::Int,
        ]
    );
}

/// 予約語は最長一致で判定し、`lettuce` のような「letで始まる識別子」を
/// 予約語に誤認しないこと(仕様1章L-2・1.5)。
#[test]
fn identifier_with_keyword_prefix_is_not_reserved() {
    // Arrange
    let source = "lettuce";

    // Act
    let tokens = lex(source).expect("`lettuce` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "lettuce".to_string(),
            span: Span { start: 0, end: 7 },
        }]
    );
}

/// 完全予約語22語(仕様1章1.5)がそれぞれ専用のキーワードトークンになること。
/// text/spanは対象外とし種類列に絞る: 22語ぶんのtext/spanを丸ごと明示すると冗長で
/// 可読性を損なうため、既存の keyword_let_is_distinguished_from_identifiers と同じ方式を採る。
#[test]
fn all_full_reserved_words_produce_keyword_tokens() {
    // Arrange
    let source = "let mut fn struct type if else match for in return \
                   import export or is none error extern true false break continue";

    // Act
    let tokens = lex(source).expect("完全予約語22語の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
        vec![
            TokenKind::KwLet,
            TokenKind::KwMut,
            TokenKind::KwFn,
            TokenKind::KwStruct,
            TokenKind::KwType,
            TokenKind::KwIf,
            TokenKind::KwElse,
            TokenKind::KwMatch,
            TokenKind::KwFor,
            TokenKind::KwIn,
            TokenKind::KwReturn,
            TokenKind::KwImport,
            TokenKind::KwExport,
            TokenKind::KwOr,
            TokenKind::KwIs,
            TokenKind::KwNone,
            TokenKind::KwError,
            TokenKind::KwExtern,
            TokenKind::KwTrue,
            TokenKind::KwFalse,
            TokenKind::KwBreak,
            TokenKind::KwContinue,
        ]
    );
}

/// 誘導用予約語(Meshに無い機能を指す予約語)`while` が出現するとE0104が
/// 位置つきで報告されること(仕様1章1.5・L-7〔負例: reserved-while〕)。
/// 誘導用は文法上の正当な出現位置が存在しないため、字句段階で即エラーにできる。
/// reserved-null / reserved-new は網で追加する。
#[test]
fn guidance_reserved_word_reports_e0104_with_span() {
    // Arrange
    let source = "while x";

    // Act
    let err = lex(source).expect_err("誘導用予約語 `while` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0104,
            span: Span { start: 0, end: 5 },
        }
    );
}

/// 誘導用予約語 `null` もE0104になること(仕様1章L-7〔負例: reserved-null〕。
/// 案内「不在は `T | none`」はメッセージ層で検証)。
/// Redを経ていない後追いの回帰テスト(表の代表2語目)。
#[test]
fn reserved_null_reports_e0104_with_span() {
    // Arrange
    let source = "null";

    // Act
    let err = lex(source).expect_err("誘導用予約語 `null` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0104,
            span: Span { start: 0, end: 4 },
        }
    );
}

/// 誘導用予約語 `new` もE0104になること(仕様1章L-7〔負例: reserved-new〕。
/// 案内「structは `User{...}` で生成します」はメッセージ層で検証)。
/// Redを経ていない後追いの回帰テスト(表の代表3語目)。
#[test]
fn reserved_new_reports_e0104_with_span() {
    // Arrange
    let source = "new";

    // Act
    let err = lex(source).expect_err("誘導用予約語 `new` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0104,
            span: Span { start: 0, end: 3 },
        }
    );
}

/// 誘導用予約語**22語すべて**がE0104(span=語全体)になること(仕様1章1.5・L-7)。
/// 表全体の網羅が1つの検証項目(完全予約語側の22語1本テストと対称)。
/// impl-reviewの変異テストで「代表3語のみでは19語の削除変異が生存する」と
/// 実証された穴を塞ぐ。表の件数driftもこのループが検知する。
#[test]
fn all_guidance_reserved_words_report_e0104() {
    // Arrange
    let words = [
        "while",
        "class",
        "null",
        "undefined",
        "enum",
        "async",
        "await",
        "try",
        "catch",
        "throw",
        "var",
        "const",
        "function",
        "switch",
        "case",
        "do",
        "interface",
        "new",
        "this",
        "typeof",
        "instanceof",
        "defer",
    ];

    // Act & Assert(表全体で1検証項目のため語ごとにループで確認。
    // lexは最初のエラーで停止するため22語を1入力に併合できない)
    for word in words {
        let err = match lex(word) {
            Err(e) => e,
            Ok(tokens) => panic!("誘導用予約語 `{word}` はエラーになること: {tokens:?}"),
        };
        assert_eq!(
            err,
            LexError {
                code: ErrorCode::E0104,
                span: Span {
                    start: 0,
                    end: word.len(),
                },
            },
            "語: {word}"
        );
    }
}

/// 誘導用予約語で**始まる**識別子は予約語でないこと(仕様1章L-2最長一致。
/// lettuceと同じ原理。誤判定は合法コードのハード拒否になるため境界を固定する)。
/// Redを経ていない後追いの回帰テスト。
#[test]
fn identifier_with_guidance_prefix_is_not_reserved() {
    // Arrange
    let source = "newValue";

    // Act
    let tokens = lex(source).expect("`newValue` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "newValue".to_string(),
            span: Span { start: 0, end: 8 },
        }]
    );
}

/// 大文字を含む語は予約語でないこと(仕様1章1.4のletterはA-Z/a-z、1.5の予約語一覧は
/// すべて小文字——大文字小文字非依存の照合に変える退行をこのテストが検知する)。
/// Redを経ていない後追いの回帰テスト。
#[test]
fn capitalized_reserved_words_are_identifiers() {
    // Arrange
    let source = "Let While NULL";

    // Act
    let tokens = lex(source).expect("大文字を含む語の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
        vec![TokenKind::Ident, TokenKind::Ident, TokenKind::Ident]
    );
}

/// コメントの中の誘導用予約語は反応しないこと(仕様1章L-4注2の不透明性)。
/// 先頭コメント行の改行は抑制される。E0104は英字の語で発火する初の字句エラーのため、
/// 不透明性の網に新実例を追加。
#[test]
fn comment_is_opaque_to_reserved_words() {
    // Arrange
    let source = "// while null new\n1";

    // Act
    let tokens = lex(source).expect("コメント内の予約語は無視されること");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "1".to_string(),
            span: Span { start: 18, end: 19 },
        }]
    );
}

/// 文字列の中身の誘導用予約語は反応しないこと(仕様1章1.7: 文字列の中身は字句モードでない)。
#[test]
fn string_is_opaque_to_reserved_words() {
    // Arrange
    let source = "\"while null\"";

    // Act
    let tokens = lex(source).expect("文字列内の予約語は無視されること");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Text {
                text: "while null".to_string(),
                span: Span { start: 1, end: 11 },
            }]),
            text: "\"while null\"".to_string(),
            span: Span { start: 0, end: 12 },
        }]
    );
}

/// 補間の内側の誘導用予約語はE0104のまま優先伝播すること(仕様1章L-18注2の
/// 内側エラー優先にE0104が加わる。終端系読み替えの対象拡大で壊れないことの固定)。
#[test]
fn guidance_reserved_inside_interpolation_reports_e0104_with_span() {
    // Arrange
    let source = "\"${while}\"";

    // Act
    let err = lex(source).expect_err("補間内の誘導用予約語はE0104としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0104,
            span: Span { start: 3, end: 8 },
        }
    );
}

/// 文脈キーワード4語(component/state/view/as)がすべてIdentトークンになること
/// (仕様1章L-27〔正例: contextual-keyword-ident〕: 両予約語表に不在。
/// 誤ってどちらかの表へ足す退行をこのテストが検知する。予約の判定はパーサが
/// component文法の内部でのみ行う)。Redを経ていない後追いの回帰テスト。
#[test]
fn contextual_keyword_words_all_lex_as_ident() {
    // Arrange
    let source = "component state view as";

    // Act
    let tokens = lex(source).expect("文脈キーワード4語の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
        vec![
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
        ]
    );
}

/// 文脈キーワードを含む仕様の正例 `let state = loadState()` の全トークン固定
/// (仕様1章L-27〔正例: contextual-keyword-ident〕)。
#[test]
fn contextual_keywords_lex_as_identifiers() {
    // Arrange
    let source = "let state = loadState()";

    // Act
    let tokens = lex(source).expect("文脈キーワードを含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "state".to_string(),
                span: Span { start: 4, end: 9 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "loadState".to_string(),
                span: Span { start: 12, end: 21 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 21, end: 22 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 22, end: 23 },
            },
        ]
    );
}

/// 仕様L-17の正例3例目 `"${m[k] or 0}"` の完全形(`or` がKwOrトークンになる。
/// 仕様1章L-17〔正例: interpolation-nested〕——予約語テーブル実装により送りを回収)。
/// Redを経ていない後追いの回帰テスト。
#[test]
fn spec_third_interpolation_example_with_keyword_or() {
    // Arrange
    let source = "\"${m[k] or 0}\"";

    // Act
    let tokens = lex(source).expect("`or` 入り補間の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Interp {
                tokens: vec![
                    Token {
                        kind: TokenKind::Ident,
                        text: "m".to_string(),
                        span: Span { start: 3, end: 4 },
                    },
                    Token {
                        kind: TokenKind::LBracket,
                        text: "[".to_string(),
                        span: Span { start: 4, end: 5 },
                    },
                    Token {
                        kind: TokenKind::Ident,
                        text: "k".to_string(),
                        span: Span { start: 5, end: 6 },
                    },
                    Token {
                        kind: TokenKind::RBracket,
                        text: "]".to_string(),
                        span: Span { start: 6, end: 7 },
                    },
                    Token {
                        kind: TokenKind::KwOr,
                        text: "or".to_string(),
                        span: Span { start: 8, end: 10 },
                    },
                    Token {
                        kind: TokenKind::Int,
                        text: "0".to_string(),
                        span: Span { start: 11, end: 12 },
                    },
                ],
                span: Span { start: 1, end: 13 },
            }]),
            text: "\"${m[k] or 0}\"".to_string(),
            span: Span { start: 0, end: 14 },
        }]
    );
}

/// `_` で始まり英字・数字が続く字句は識別子であること(仕様1章1.4: `_tmp` は正規の識別子)。
#[test]
fn underscore_prefixed_name_is_identifier() {
    // Arrange
    let source = "_tmp1";

    // Act
    let tokens = lex(source).expect("`_tmp1` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "_tmp1".to_string(),
            span: Span { start: 0, end: 5 },
        }]
    );
}

/// `_` の**直後に数字**が続く形も識別子であること(仕様1章1.4・L-9注1が名指しする `_1`)。
/// L-9の「先頭の `_` はエラー」は数値リテラル内部の規則で、10進では `_` 始まりに
/// 数値スキャンが到達しないため衝突しない(仕様1章L-9注1)。
/// 「`_` の次が数字なら数値スキャンへ回す」誤実装(→E0105化)を検知する境界テスト。
#[test]
fn underscore_followed_by_digit_is_identifier() {
    // Arrange
    let source = "_1";

    // Act
    let tokens = lex(source).expect("`_1` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "_1".to_string(),
            span: Span { start: 0, end: 2 },
        }]
    );
}

/// `_` 単体も字句段階では識別子トークンになること(仕様1章1.4)。
/// 「値を捨てる」専用の特別扱いは仕様4章(構文解析側)の担当。
#[test]
fn underscore_alone_is_identifier() {
    // Arrange
    let source = "_";

    // Act
    let tokens = lex(source).expect("`_` 単体の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "_".to_string(),
            span: Span { start: 0, end: 1 },
        }]
    );
}

/// 識別子位置の非ASCII文字はE0103として報告され、spanは連続する非ASCII文字の
/// 全体を指すこと(仕様1章L-6〔負例: non-ascii-ident〕)。
/// `合計` は1文字3バイト×2文字=6バイトで、`let ` の4バイトを合わせて 4..10。
/// 先頭1文字だけ(3バイト)を指すと修正箇所が伝わらないため、仕様が全体を要求している。
#[test]
fn non_ascii_identifier_reports_e0103_with_full_span() {
    // Arrange
    let source = "let 合計 = 0";

    // Act
    let err = lex(source).expect_err("非ASCII識別子はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0103,
            span: Span { start: 4, end: 10 },
        }
    );
}

/// 連続する非ASCII字母の読み取りはASCII字母で止まること(仕様1章L-6)。
/// `let 合calc = 0` の非ASCII字母は `合`(3バイト)のみ。`calc` はASCII字母のため
/// E0103のspanに入らず、spanは 4..7。非ASCII判定からASCII条件を外した実装は
/// `calc` まで飲み込んで 4..8 を返すため、この境界で検知する。
#[test]
fn non_ascii_run_stops_at_ascii_letter() {
    // Arrange
    let source = "let 合calc = 0";

    // Act
    let err = lex(source).expect_err("非ASCII字母を含む識別子位置はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0103,
            span: Span { start: 4, end: 7 },
        }
    );
}

/// ASCII識別子に続く非ASCII字母は、識別子全体でなく非ASCII字母だけを指すこと
/// (仕様1章L-6)。`let calc合 = 0` では `calc`(4..8)は正規のASCII識別子として
/// トークン化され、`合`(3バイト)だけがE0103の対象。spanは 8..11。
#[test]
fn non_ascii_after_ascii_identifier_covers_non_ascii_only() {
    // Arrange
    let source = "let calc合 = 0";

    // Act
    let err = lex(source).expect_err("ASCII識別子末尾の非ASCII字母はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0103,
            span: Span { start: 8, end: 11 },
        }
    );
}

/// 字母でない非ASCII(全角数字 `０` = U+FF11)はE0103でなくL-26のE0116を報告すること
/// (仕様1章L-6。「英数字名への変更を促す」案内が数字に意味をなさないため)。
/// `０` は3バイトのため、spanは `let ` の直後の 4..7。
/// E0103の判定から字母条件を外した実装がこの入力をE0103にするため、この境界で検知する。
#[test]
fn fullwidth_digit_reports_e0116_not_e0103() {
    // Arrange
    let source = "let ０ = 0";

    // Act
    let err = lex(source).expect_err("全角数字は字句規則に該当しないためエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 4, end: 7 },
        }
    );
}

/// 各トークンがソース中のバイトオフセット位置(span)を持つこと。
/// 位置つきエラー報告の基盤であり、仕様1章の各E01xx規則が「位置つき」報告を要求する。
#[test]
fn tokens_carry_byte_offset_spans() {
    // Arrange
    let source = "let answer = 42";

    // Act
    let tokens = lex(source).expect("`let answer = 42` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "answer".to_string(),
                span: Span { start: 4, end: 10 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::Int,
                text: "42".to_string(),
                span: Span { start: 13, end: 15 },
            },
        ]
    );
}

/// エラーのspanはバイトオフセットであること(文字数ではない)。
/// マルチバイト文字 `±`(U+00B1、UTF-8で2バイト)のE0116(仕様1章L-26)は2バイト幅のspanを持つ。
/// 仕様1章の位置つき報告(E01xx)がバイト単位で一貫していることの固定。
/// 注: `±` は識別子位置に無い記号なので、L-6(非ASCII識別子=E0103)実装後もE0116のまま。
#[test]
fn multibyte_character_error_span_counts_bytes() {
    // Arrange
    let source = "1 ± 2";

    // Act
    let err = lex(source).expect_err("`±` は字句規則に該当しないためエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 2, end: 4 },
        }
    );
}

/// どの字句規則にも該当しない文字はエラーE0116として位置つきで報告されること(仕様1章L-26)。
#[test]
fn unknown_character_reports_e0116_with_span() {
    // Arrange
    let source = "let @ = 1";

    // Act
    let err = lex(source).expect_err("`@` は字句規則に該当しないためエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 4, end: 5 },
        }
    );
}

/// バッククォート文字列がE0116として位置つきで報告されること
/// (仕様1章L-26〔負例: backtick-string〕)。
/// 「近い正解の案内」(`"` を使う)はエラーメッセージ実装のサイクルで検証する。
#[test]
fn backtick_string_reports_e0116_with_span() {
    // Arrange
    let source = "`abc`";

    // Act
    let err = lex(source).expect_err("バッククォート文字列はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 0, end: 1 },
        }
    );
}

/// シングルクォート文字列がE0116として位置つきで報告されること
/// (仕様1章L-26〔負例: single-quote-string〕)。
#[test]
fn single_quote_string_reports_e0116_with_span() {
    // Arrange
    let source = "'abc'";

    // Act
    let err = lex(source).expect_err("シングルクォート文字列はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 0, end: 1 },
        }
    );
}

/// 桁区切り `_` を含む整数リテラルが1個のIntトークンとして切り出されること(仕様1章L-9)。
/// textはソースの生の字面のまま保持し、`_` を除去しないこと(値への正規化はコード生成側の責務)。
#[test]
fn integer_with_digit_separator_is_single_token() {
    // Arrange
    let source = "1_000";

    // Act
    let tokens = lex(source).expect("桁区切り付き整数リテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "1_000".to_string(),
            span: Span { start: 0, end: 5 },
        }]
    );
}

/// floatリテラル(`decInt "." decInt`)が1個のFloatトークンとして切り出されること(仕様1章1.6)。
/// textはソースの生の字面のまま保持すること。
#[test]
fn float_literal_produces_single_float_token() {
    // Arrange
    let source = "3.14";

    // Act
    let tokens = lex(source).expect("floatリテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Float,
            text: "3.14".to_string(),
            span: Span { start: 0, end: 4 },
        }]
    );
}

/// 整数部が `0` のfloatリテラルも1個のFloatトークンになること(仕様1章1.6の正例 `0.5`)。
#[test]
fn float_with_zero_integer_part_is_single_token() {
    // Arrange
    let source = "0.5";

    // Act
    let tokens = lex(source).expect("`0.5` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Float,
            text: "0.5".to_string(),
            span: Span { start: 0, end: 3 },
        }]
    );
}

/// 整数部が `0` のfloat+指数形式も1個のFloatトークンになること(仕様1章1.6の正例・
/// L-12注2の境界固定。整数部が `0` 1文字+指数は正当)。
#[test]
fn float_with_zero_integer_part_and_exponent_is_single_token() {
    // Arrange
    let source = "0e5";

    // Act
    let tokens = lex(source).expect("`0e5` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Float,
            text: "0e5".to_string(),
            span: Span { start: 0, end: 3 },
        }]
    );
}

/// `..` の消極規則はfloat直後にも適用され、floatリテラルの直後の `..` は
/// 数値に取り込まれず独立したDotDotトークンになること(仕様1章L-3)。
/// dotdot_after_integer_splits_range のdocコメントが予約していたfloatサイクルの追加検証。
#[test]
fn float_before_dotdot_splits_range() {
    // Arrange
    let source = "1.5..2";

    // Act
    let tokens = lex(source).expect("`1.5..2` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Float,
                text: "1.5".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::DotDot,
                text: "..".to_string(),
                span: Span { start: 3, end: 5 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 5, end: 6 },
            },
        ]
    );
}

/// 指数部つきfloatが1個のFloatトークンになること(仕様1章1.6の指数正例4形: 小文字e・大文字E・小数+負符号・正符号)。
#[test]
fn exponent_forms_produce_float_tokens() {
    // Arrange
    let source = "1e6 1E6 2.5e-3 1e+6";

    // Act
    let tokens = lex(source).expect("指数部つきfloatの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Float,
                text: "1e6".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::Float,
                text: "1E6".to_string(),
                span: Span { start: 4, end: 7 },
            },
            Token {
                kind: TokenKind::Float,
                text: "2.5e-3".to_string(),
                span: Span { start: 8, end: 14 },
            },
            Token {
                kind: TokenKind::Float,
                text: "1e+6".to_string(),
                span: Span { start: 15, end: 19 },
            },
        ]
    );
}

/// floatの小数点は両側に数字が必須(仕様1章L-10〔負例: float-dot-edge〕)。数字直後の `.` は
/// 直後が数字でも `.` でもないとき小数部欠落としてE0106になり、メンバアクセスの `.` とは
/// 解釈しない(数値リテラルへのフィールドアクセスは3章X-30がstruct型限定のためそもそも合法でない)。
/// spanは数字列+`.` の全体。
#[test]
fn float_missing_fraction_reports_e0106_with_span() {
    // Arrange
    let source = "0.";

    // Act
    let err = lex(source).expect_err("小数部が無い `0.` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0106,
            span: Span { start: 0, end: 2 },
        }
    );

    // Arrange
    let source = "1.abs";

    // Act
    let err = lex(source).expect_err("数字直後の `.` に識別子が続く `1.abs` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0106,
            span: Span { start: 0, end: 2 },
        }
    );

    // Arrange(仕様L-10注(a)が名指しする優先順位の例: E0106は先頭ゼロのE0113より先に走る)
    let source = "0755.";

    // Act
    let err = lex(source).expect_err("`0755.` は先頭ゼロのE0113より先にE0106になること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0106,
            span: Span { start: 0, end: 5 },
        }
    );
}

/// floatの整数部欠落(仕様1章L-10〔負例: float-dot-edge〕)。`.` の直後が数字のときE0106になり、
/// spanは `.`+後続数字列の全体。仕様L-10注(b)が名指しする「数値リテラルの続きとして
/// 読まれない位置の `.`+数字」(基数リテラル直後の `0xFF.5`・二重小数点 `3.14.5`)も同形。
#[test]
fn float_missing_integer_part_reports_e0106_with_span() {
    // Arrange
    let source = ".5";

    // Act
    let err = lex(source).expect_err("整数部が無い `.5` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0106,
            span: Span { start: 0, end: 2 },
        }
    );

    // Arrange(16進の小数は存在しない=1.6。0xFFの直後の .5 が注(b)で拾われる)
    let source = "0xFF.5";

    // Act
    let err = lex(source).expect_err("`0xFF.5` の `.5` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0106,
            span: Span { start: 4, end: 6 },
        }
    );

    // Arrange(二重の小数点。2個目の .5 が注(b)で拾われる)
    let source = "3.14.5";

    // Act
    let err = lex(source).expect_err("`3.14.5` の2個目の小数点はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0106,
            span: Span { start: 4, end: 6 },
        }
    );
}

/// 基数接頭辞つき整数(仕様1章1.6: `"0x" hexInt | "0b" binInt | "0o" octInt`)が
/// それぞれ1個のIntトークンになり、textは生の字面のままであること。
/// 16進の英字は大文字小文字とも hexDigit として受理する。
#[test]
fn radix_prefixed_integers_produce_int_tokens() {
    // Arrange
    let source = "0xFF 0b1010 0o755";

    // Act
    let tokens = lex(source).expect("基数接頭辞つき整数の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "0xFF".to_string(),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::Int,
                text: "0b1010".to_string(),
                span: Span { start: 5, end: 11 },
            },
            Token {
                kind: TokenKind::Int,
                text: "0o755".to_string(),
                span: Span { start: 12, end: 17 },
            },
        ]
    );
}

/// 基数リテラル内の正しい桁区切り `_`(数字と数字の間)が受理されること
/// (仕様1章1.6の `hexDigit { hexDigit | "_" hexDigit }` 等・L-9)。
/// check_digit_separatorsの数字述語一般化(10進固定だと `0xF_F` を誤ってE0105にする)の
/// 退行防止網。impl-reviewの生存変異(述語を10進固定に戻しても全テスト緑)を受けて追加した
/// Redを経ない後追いテスト(変異の再適用でKILLを確認済み)。
#[test]
fn radix_digit_separators_are_accepted() {
    // Arrange
    let source = "0xF_F 0b1010_1010 0o7_5_5";

    // Act
    let tokens = lex(source).expect("基数リテラルの桁区切りはエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "0xF_F".to_string(),
                span: Span { start: 0, end: 5 },
            },
            Token {
                kind: TokenKind::Int,
                text: "0b1010_1010".to_string(),
                span: Span { start: 6, end: 17 },
            },
            Token {
                kind: TokenKind::Int,
                text: "0o7_5_5".to_string(),
                span: Span { start: 18, end: 25 },
            },
        ]
    );
}

/// 整数部・小数部・指数部それぞれの内側の桁区切り `_` を含むfloatが1個のFloatトークンに
/// なること(仕様1章1.6・L-9)。check_float_overflowのf64パースが `_` 除去を前提にする
/// 構造(除去を外すとexpectがpanicに化ける)の退行防止網。impl-reviewの生存変異を受けて
/// 追加したRedを経ない後追いテスト(変異の再適用でKILLを確認済み)。
#[test]
fn float_digit_separators_are_accepted() {
    // Arrange
    let source = "1_000.000_1e1_0";

    // Act
    let tokens = lex(source).expect("桁区切り入りfloatはエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Float,
            text: "1_000.000_1e1_0".to_string(),
            span: Span { start: 0, end: 15 },
        }]
    );
}

/// 基数接頭辞直後・指数部直後・基数リテラル内の連続 `_` がE0105になり、
/// 違反した `_` 1バイトを位置として報告すること(仕様1章L-9〔負例: underscore-edge〕の
/// `0x_FF`・`1e_6` を基数実装のこのサイクルで回収)。L-9注1のとおり、`_` で始まる字句は
/// 識別子として読まれるため、「先頭」のE0105は基数接頭辞直後と指数部直後の2形でのみ発生する。
#[test]
fn underscore_in_radix_and_exponent_reports_e0105_with_span() {
    // Arrange (1: 基数接頭辞直後の `_`)
    let source = "0x_FF";

    // Act
    let err = lex(source).expect_err("`0x_FF` は基数接頭辞直後の `_` でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 2, end: 3 },
        }
    );

    // Arrange (2: 指数部直後の `_`)
    let source = "1e_6";

    // Act
    let err = lex(source).expect_err("`1e_6` は指数部直後の `_` でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 2, end: 3 },
        }
    );

    // Arrange (3: 基数リテラル内の連続 `_`)
    let source = "0x1__2";

    // Act
    let err = lex(source).expect_err("`0x1__2` は連続する `_` の1個目でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 3, end: 4 },
        }
    );
}

/// 仕様1章L-12〔負例: number-malformed〕。指数部が空・基数接頭辞の後に数字が無い・
/// 基数外の数字・先頭ゼロの10進(int/float/指数)・数字直後の識別子文字の5形はE0113。
/// spanはリテラル全体を指すこと(修正候補が全体置換のため、`0b102` を `0b10`+`2` に分割しない流儀を5形に適用)。
/// 先頭ゼロの変種としてケース6〜15も固定する: 最小の先頭ゼロ `00` / 接頭辞の直後から基数外の
/// `0o8`(基数ガードを外れ10進経路の末尾検査で拾う)/ 大文字接頭辞 `0XFF`(接頭辞は
/// 小文字のみ=1.6のEBNF)/ 先頭ゼロ+識別子文字の複合 `0755abc`(末尾検査が先に発火し
/// spanは字面全体)/ 先頭ゼロのfloat `0755.5` / 先頭ゼロのfloat・最小 `00.5` / 先頭ゼロの指数 `01e5` /
/// 先頭ゼロのfloat+指数 `00.5e1` / 大文字指数 `01E5`(整数部の終わり判定は `E` も境界とする)/
/// 符号つき指数 `01e+5`(指数の符号は整数部に含まない)。
/// 先頭ゼロの適用範囲は10進の全形(int/float/指数)で、整数部が`0`で始まり2文字以上の
/// ケースを網羅する(L-12注2)。ケース14〜15はimpl-review(2026-08-17)の指摘
/// (`E` 落とし・符号混入の退行変異が全テスト緑のまま生存する)を受けて追加した後追い。
#[test]
fn malformed_number_reports_e0113_with_span() {
    // Arrange (1: 指数部が空)
    let source = "1e";

    // Act
    let err = lex(source).expect_err("`1e` は指数部が空でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 2 },
        }
    );

    // Arrange (2: 基数接頭辞の後に数字が無い)
    let source = "0x";

    // Act
    let err = lex(source).expect_err("`0x` は基数接頭辞の後に数字が無くエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 2 },
        }
    );

    // Arrange (3: 基数外の数字)
    let source = "0b102";

    // Act
    let err = lex(source).expect_err("`0b102` は基数外の数字でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 5 },
        }
    );

    // Arrange (4: 先頭ゼロの10進)
    let source = "0755";

    // Act
    let err = lex(source).expect_err("`0755` は先頭ゼロの10進でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 4 },
        }
    );

    // Arrange (5: 数字の直後に識別子文字)
    let source = "123abc";

    // Act
    let err = lex(source).expect_err("`123abc` は数字の直後の識別子文字でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 6 },
        }
    );

    // Arrange (6: 最小の先頭ゼロ)
    let source = "00";

    // Act
    let err = lex(source).expect_err("`00` は先頭ゼロの10進でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 2 },
        }
    );

    // Arrange (7: 接頭辞の直後から基数外の数字)
    let source = "0o8";

    // Act
    let err = lex(source).expect_err("`0o8` は8進の範囲外でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 3 },
        }
    );

    // Arrange (8: 大文字の基数接頭辞)
    let source = "0XFF";

    // Act
    let err = lex(source).expect_err("`0XFF` は大文字接頭辞でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 4 },
        }
    );

    // Arrange (9: 先頭ゼロ+識別子文字の複合)
    let source = "0755abc";

    // Act
    let err =
        lex(source).expect_err("`0755abc` は末尾検査が先に発火して字面全体のエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 7 },
        }
    );

    // Arrange (10: 先頭ゼロのfloat)
    let source = "0755.5";

    // Act
    let err = lex(source).expect_err("`0755.5` は先頭ゼロの10進でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 6 },
        }
    );

    // Arrange (11: 先頭ゼロのfloat・最小)
    let source = "00.5";

    // Act
    let err = lex(source).expect_err("`00.5` は先頭ゼロの10進でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 4 },
        }
    );

    // Arrange (12: 先頭ゼロの指数形式)
    let source = "01e5";

    // Act
    let err = lex(source).expect_err("`01e5` は先頭ゼロの10進でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 4 },
        }
    );

    // Arrange (13: 先頭ゼロのfloat+指数形式)
    let source = "00.5e1";

    // Act
    let err = lex(source).expect_err("`00.5e1` は先頭ゼロの10進でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 6 },
        }
    );

    // Arrange (14: 先頭ゼロの大文字指数形式)
    let source = "01E5";

    // Act
    let err = lex(source).expect_err("`01E5` は大文字 `E` も指数部として先頭ゼロになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 4 },
        }
    );

    // Arrange (15: 先頭ゼロの符号つき指数形式)
    let source = "01e+5";

    // Act
    let err = lex(source).expect_err("`01e+5` は指数の符号を含め字面全体でエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0113,
            span: Span { start: 0, end: 5 },
        }
    );
}

/// 仕様1章L-11〔負例: int-literal-overflow〕。整数リテラルが安全整数域
/// ±2^53−1(=9007199254740991)を超えるとE0107(ADR-0015の静的検査版)。
/// 字句には符号が無いため絶対値で判定する(`-` は別トークンで、リテラル自体は常に非負)。
/// spanはリテラル全体。基数接頭辞つき(16進など)のリテラルにも同じ判定を適用する。
#[test]
fn int_literal_overflow_reports_e0107_with_span() {
    // Arrange (1: 2^53、10進)
    let source = "9007199254740992";

    // Act
    let err = lex(source).expect_err("`9007199254740992` は安全整数域を超えてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0107,
            span: Span { start: 0, end: 16 },
        }
    );

    // Arrange (2: 2^53、16進)
    let source = "0x20000000000000";

    // Act
    let err = lex(source).expect_err("`0x20000000000000` は安全整数域を超えてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0107,
            span: Span { start: 0, end: 16 },
        }
    );

    // Arrange (3: u128でも溢れる47桁。from_str_radixのErr分岐も超過として扱う退行防止網。
    // impl-reviewの生存変異を受けて追加したRedを経ない後追い)
    let source = "99999999999999999999999999999999999999999999999";

    // Act
    let err = lex(source).expect_err("47桁の10進はu128でも溢れてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0107,
            span: Span { start: 0, end: 47 },
        }
    );
}

/// 仕様1章L-11の境界: 安全整数域の上限2^53−1(=9007199254740991)ちょうどは、
/// 10進でも16進(`0x1FFFFFFFFFFFFF`)でも正当なIntトークンになること。
/// E0107の負例テストと同時に追加する境界の退行防止網であり、Redを経ない
/// (16進側はimpl-review指摘で追加: 基数指定の値解釈が下限方向へ誤判定していない証拠)。
#[test]
fn int_literal_at_safe_boundary_is_accepted() {
    // Arrange
    let source = "9007199254740991 0x1FFFFFFFFFFFFF";

    // Act
    let tokens = lex(source).expect("安全整数域の境界ちょうどはエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "9007199254740991".to_string(),
                span: Span { start: 0, end: 16 },
            },
            Token {
                kind: TokenKind::Int,
                text: "0x1FFFFFFFFFFFFF".to_string(),
                span: Span { start: 17, end: 33 },
            },
        ]
    );
}

/// floatリテラルがIEEE754倍精度で表現できない大きさのとき(パースすると無限大になる
/// `1e999` 等)E0114になること(仕様1章L-13〔負例: float-literal-overflow〕)。
/// 「静かにInfinityにしない」。spanはリテラル全体。
#[test]
fn float_literal_overflow_reports_e0114_with_span() {
    // Arrange
    let source = "1e999";

    // Act
    let err = lex(source).expect_err("`1e999` はIEEE754倍精度で表現できないためエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0114,
            span: Span { start: 0, end: 5 },
        }
    );
}

/// floatが表現可能な大きさの境界を確認すること(仕様1章L-13の境界)。
/// `1e308` は有限値として表現できるためエラーにならない。
/// 負例E0114と同時に追加する境界の網で、Redを経ない(現状も通る)。
#[test]
fn float_at_representable_magnitude_is_accepted() {
    // Arrange
    let source = "1e308";

    // Act
    let tokens = lex(source).expect("`1e308` は字句解析エラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Float,
            text: "1e308".to_string(),
            span: Span { start: 0, end: 5 },
        }]
    );
}

/// アンダーフロー方向(`1e-999` は0.0に丸まる)はL-13の対象外でエラーにしないこと
/// (仕様1章L-13: 対象は「表現できない大きさ」=絶対値の上方超過のみ)。
/// 表現域境界(1e308)とは別検証項目のため関数を分ける(impl-review指摘で分割)。
/// Redを経ない挙動固定の網。
#[test]
fn float_underflow_is_not_an_error() {
    // Arrange
    let source = "1e-999";

    // Act
    let tokens = lex(source).expect("`1e-999` は字句解析エラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Float,
            text: "1e-999".to_string(),
            span: Span { start: 0, end: 6 },
        }]
    );
}

/// 桁区切り `_` が複数あっても1個のIntトークンになること(仕様1章の正例列 `1_000_000`)。
/// 位置検査ループの2周目以降を固定する。
#[test]
fn integer_with_multiple_digit_separators_is_single_token() {
    // Arrange
    let source = "1_000_000";

    // Act
    let tokens = lex(source).expect("`1_000_000` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "1_000_000".to_string(),
            span: Span { start: 0, end: 9 },
        }]
    );
}

/// 連続した桁区切り `_` がエラーE0105になり、違反した最初の `_` 1バイトを
/// 位置として報告すること(仕様1章L-9〔負例: underscore-edge〕)。
/// 同じunderscore-edgeの `0x_FF`・`1e_6` は
/// underscore_in_radix_and_exponent_reports_e0105_with_span が担う。
#[test]
fn consecutive_digit_separators_report_e0105_with_span() {
    // Arrange
    let source = "1__0";

    // Act
    let err = lex(source).expect_err("`_` が連続する整数リテラルはエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 1, end: 2 },
        }
    );
}

/// 末尾の桁区切り `_` がエラーE0105になり、その `_` の位置を報告すること
/// (仕様1章L-9〔負例: underscore-edge〕)。
#[test]
fn trailing_digit_separator_reports_e0105_with_span() {
    // Arrange
    let source = "1_";

    // Act
    let err = lex(source).expect_err("末尾が `_` の整数リテラルはエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 1, end: 2 },
        }
    );
}

/// `_` の直後に英字が続く形はL-12(E0113)ではなくE0105になること
/// (仕様1章L-9注2で固定した優先順位〔負例: underscore-edge〕)。
/// 数字直後の `_` は数値リテラルの一部として読むため。
#[test]
fn digit_separator_before_letter_reports_e0105_with_span() {
    // Arrange
    let source = "1_x";

    // Act
    let err = lex(source).expect_err("`1_x` は桁区切りエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 1, end: 2 },
        }
    );
}

/// 改行がNewlineトークンとして切り出されること(仕様1章L-19の字句側の基盤)。
/// textは生の字面 `"\n"` を保持する。文終端の挿入判定(L-19/L-20/L-21)は
/// **字句側**の後続サイクルの責務(ADR-0010のGo方式・ADR-0031の深度スタック)。
/// 現サイクルは改行の事実だけをトークン化する。
#[test]
fn newline_produces_newline_token() {
    // Arrange
    let source = "1\n2";

    // Act
    let tokens = lex(source).expect("改行を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 2, end: 3 },
            },
        ]
    );
}

/// 複数行のプログラム全体が字句解析でき、**2行目以降のspanが改行ぶん正しく
/// オフセットされる**こと(2つ目の `let` が10..13になる。仕様1章L-19の受け入れ確認)。
/// このオフセット検証が固有価値なので、単一行のspanテストと統合しないこと。
/// Newline実装後は最初から通るためRedを経ていない、後追いの回帰テスト。
#[test]
fn multi_line_program_lexes_across_newlines() {
    // Arrange
    let source = "let x = 1\nlet y = 2";

    // Act
    let tokens = lex(source).expect("複数行プログラムの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "x".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 9, end: 10 },
            },
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 10, end: 13 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "y".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 16, end: 17 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 18, end: 19 },
            },
        ]
    );
}

/// 末尾改行つきの入力が最後にNewlineトークンを持つこと(仕様1章L-19)。
/// POSIX準拠のテキストファイルは末尾に改行を持つのが標準で、実際の
/// .meshファイルのほぼすべてがこの形になる。
#[test]
fn trailing_newline_produces_final_newline_token() {
    // Arrange
    let source = "1\n";

    // Act
    let tokens = lex(source).expect("末尾改行つき入力の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 1, end: 2 },
            },
        ]
    );
}

/// 改行のみの入力はトークンを生成しないこと(仕様1章L-19注「直前がNewlineなら終端を挿入しない」)。
/// 入力先頭の改行も抑制対象。空入力との境界。
#[test]
fn newline_only_source_produces_no_tokens() {
    // Arrange
    let source = "\n";

    // Act
    let tokens = lex(source).expect("改行のみの入力の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(tokens, vec![]);
}

/// 行末の空白は改行トークンに影響しないこと(仕様1章1.2・L-19)。
/// 将来のL-20「行末のトークン」判定は、行末の空白がトークンにならないこと
/// を前提に成立する——その字句側の前提をここで固定する。
#[test]
fn trailing_spaces_before_newline_are_skipped() {
    // Arrange
    let source = "1 \n2";

    // Act
    let tokens = lex(source).expect("行末空白+改行の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 3, end: 4 },
            },
        ]
    );
}

/// 直後が `\n` でない単独の `\r` はE0117になること(仕様1章L-29〔負例: lone-cr〕)。
#[test]
fn lone_carriage_return_reports_e0117_with_span() {
    // Arrange
    let source = "\r";

    // Act
    let err = lex(source).expect_err("単独の`\\r`はE0117としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0117,
            span: Span { start: 0, end: 1 },
        }
    );
}

/// 行中のCR(直後が `\n` 以外の文字)もE0117になること(仕様1章L-29〔負例: lone-cr〕)。
/// 単独CRの2形(EOF直前=上のテスト/行中=このテスト)のうち、旧Mac形式
/// `"1\r2"` が踏む実用上の主経路を固定する。
#[test]
fn mid_line_carriage_return_reports_e0117_with_span() {
    // Arrange
    let source = "1\r2";

    // Act
    let err = lex(source).expect_err("行中の`\\r`はE0117としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0117,
            span: Span { start: 1, end: 2 },
        }
    );
}

/// Unicode改行類(U+2028 LINE SEPARATOR)は改行とみなさず、E0116のままであること
/// (仕様1章L-29第3項。ADR-0041が代替案を検討のうえ確定した挙動の固定——
/// 「JSはU+2028を改行扱いするから合わせる」変更が入ったら、このテストが検知する)。
/// U+0085・U+2029も同じ規則。spanは文字のバイト幅(U+2028は3バイト)。
#[test]
fn unicode_line_separator_is_not_newline() {
    // Arrange
    let source = "1\u{2028}2";

    // Act
    let err = lex(source).expect_err("U+2028は改行でないためE0116エラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 1, end: 4 },
        }
    );
}

/// CRLF(`\r\n`)は1個のNewlineトークンになり、textは生の字面 `\r\n` になること
/// (仕様1章L-29〔正例: crlf-newline〕・ADR-0041)。
#[test]
fn crlf_produces_single_newline_token() {
    // Arrange
    let source = "1\r\n2";

    // Act
    let tokens = lex(source).expect("CRLFを含む入力の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\r\n".to_string(),
                span: Span { start: 1, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 3, end: 4 },
            },
        ]
    );
}

/// 文の間の空行は1個のNewlineトークンのみを生成すること(仕様1章L-19)。
/// 直前にNewlineがある改行は抑制される。空文を作らない制約。
#[test]
fn blank_line_between_statements_yields_single_newline_token() {
    // Arrange
    let source = "1\n\n2";

    // Act
    let tokens = lex(source).expect("空行を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 3, end: 4 },
            },
        ]
    );
}

/// CRLF行の空行も同じ抑制規則で1個のNewlineのみを生成すること
/// (仕様1章L-19注・L-29)。抑制判定はトークン種類ベースであり、Newlineの
/// textが `\r\n`(2バイト)でも `\n`(1バイト)でも同じ経路で抑制されることを
/// 固定する(impl-review 2026-08-18 観点A指摘の追加テスト)。
#[test]
fn crlf_blank_line_between_statements_yields_single_newline_token() {
    // Arrange
    let source = "1\r\n\r\n2";

    // Act
    let tokens = lex(source).expect("CRLF空行を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\r\n".to_string(),
                span: Span { start: 1, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 5, end: 6 },
            },
        ]
    );
}

/// `//` から行末まではコメントとして読み飛ばされ、トークンを生成しないこと
/// (仕様1章L-4〔正例: line-comment〕)。改行自体はコメントに含まれず、独立した
/// Newlineトークンとして残る(L-4「行末トークンの判定はコメントを除去した後の
/// 行末に対して行う」の字句側基盤)。
#[test]
fn line_comment_produces_no_tokens() {
    // Arrange
    let source = "1 // c\n2";

    // Act
    let tokens = lex(source).expect("行コメントを含む入力の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 7, end: 8 },
            },
        ]
    );
}

/// 行コメントのスキャンは `\r\n` の手前(`\r` の直前)で止まり、`\r` をコメントに
/// 飲み込まないこと(仕様1章L-4とL-29の相互作用・ADR-0041)。
/// コメントの読み飛ばしを `\n` の手前だけで止める実装だと、CRLF行では `\r` が
/// コメントの一部として消費され、続くNewlineトークンが `\r\n`(2バイト)ではなく
/// `\n`(1バイト)に縮む——L-29「CRLFは1個の改行として扱い、字面は保持する」への
/// 違反となる。この退行を検知する境界テスト。
#[test]
fn line_comment_stops_before_crlf() {
    // Arrange
    let source = "1 // c\r\n2";

    // Act
    let tokens =
        lex(source).expect("CRLF行末の行コメントを含む入力の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\r\n".to_string(),
                span: Span { start: 6, end: 8 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 8, end: 9 },
            },
        ]
    );
}

/// ブロックコメント `/*` がエラーE0102として位置つきで報告されること
/// (仕様1章L-5「ブロックコメントはありません。`//` を使ってください」〔負例: block-comment〕)。
/// spanは `/*` の2バイトぶん。エラーメッセージ文言の検証はメッセージ実装サイクルの担当。
#[test]
fn block_comment_reports_e0102_with_span() {
    // Arrange
    let source = "/* x";

    // Act
    let err = lex(source).expect_err("ブロックコメント `/*` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0102,
            span: Span { start: 0, end: 2 },
        }
    );
}

/// 改行なしでファイル末尾に達したコメントも正しく閉じること(仕様1章L-4)。
/// Redを経ていない後追いの回帰テスト(実装が最初からEOF停止を満たすため)。
#[test]
fn line_comment_at_eof_produces_no_tokens() {
    // Arrange
    let source = "1 // c";

    // Act
    let tokens = lex(source).expect("EOFで終わるコメントの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "1".to_string(),
            span: Span { start: 0, end: 1 },
        }]
    );
}

/// コメントの中身は完全に不透明であること(仕様1章L-4・1.4)。コメント外なら
/// エラーになる字句(`/*`=E0102・`;`・`"`・`` ` ``)や日本語を含んでも反応しない。
/// 後続トークンのspanで、マルチバイトを含むコメントのバイト幅スキップも同時に固定する。
/// 次サイクル以降(`;`→E0110・文字列・E0103)が「コメント内なのに反応する」
/// 退行を起こしたとき、このテストが検知する(変異テストで必要性を実証済み)。
#[test]
fn comment_content_is_opaque_to_all_lexical_rules() {
    // Arrange
    let source = "1 // /* ; \" ` 合計\n2";

    // Act
    let tokens = lex(source).expect("コメント内の字句はすべて無視されること");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 21, end: 22 },
            },
        ]
    );
}

/// コメント中に単独のCRが現れたときはL-29(E0117)がL-4より優先すること
/// (仕様1章L-4注1〔負例: lone-cr〕・ADR-0041「単独のCRは専用エラー」に例外を設けない)。
#[test]
fn lone_cr_inside_comment_reports_e0117_with_span() {
    // Arrange
    let source = "1 // a\rb";

    // Act
    let err = lex(source).expect_err("コメント中の単独`\\r`はE0117としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0117,
            span: Span { start: 6, end: 7 },
        }
    );
}

/// `/` は除算演算子(仕様1章1.9)として1文字トークンになり、`//` はL-4の行コメントとして
/// トークンを生成しないこと(仕様1章L-4)。暫定E0116テスト(lone_slash_reports_e0116_with_span)
/// をこの正例で置き換えた。
#[test]
fn slash_is_division_and_double_slash_is_comment() {
    // Arrange
    let source = "10 / 2 // half";

    // Act
    let tokens = lex(source).expect("`/`は除算、`//`はコメントとしてエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "10".to_string(),
                span: Span { start: 0, end: 2 },
            },
            Token {
                kind: TokenKind::Slash,
                text: "/".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 5, end: 6 },
            },
        ]
    );
}

/// コメントのみの行の改行はトークンを生成しないこと(仕様1章L-19注・L-4)。
/// 直前に出力したトークンがない（入力先頭またはNewline直後）の改行は抑制される。
#[test]
fn comment_only_lines_produce_no_newline_tokens() {
    // Arrange
    let source = "// a\n// b\n1";

    // Act
    let tokens = lex(source).expect("コメントのみの行の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "1".to_string(),
            span: Span { start: 10, end: 11 },
        }]
    );
}

/// `///`(ドキュメンテーションコメント予約)はv1では `//` と同じ扱いであること
/// (仕様1章L-30〔正例: doc-comment-v1〕)。
/// Redを経ていない後追いの回帰テスト(`///` は `//` で始まるため自動的にコメントになる)。
/// 先頭コメント行の改行は抑制される。
#[test]
fn doc_comment_is_treated_as_line_comment_in_v1() {
    // Arrange
    let source = "/// d\n1";

    // Act
    let tokens = lex(source).expect("`///` コメントの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "1".to_string(),
            span: Span { start: 6, end: 7 },
        }]
    );
}

/// セミコロンはエラー E0110 になること(仕様1章L-19〔負例: semicolon〕)。
/// spanは `;` 1バイトをピンポイントで指す。現状は暫定のE0116のため、本テストがRedになる。
#[test]
fn semicolon_reports_e0110_with_span() {
    // Arrange
    let source = "let x = 1;";

    // Act
    let err = lex(source).expect_err("セミコロンはE0110としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0110,
            span: Span { start: 9, end: 10 },
        }
    );
}

/// 二重引用符の単一行文字列リテラルが1個のStrトークンとして切り出されること(仕様1章1.7)。
/// textはクォートを含む生の字面(エスケープ解決した値は持たない。値への変換は
/// コード生成側の責務——桁区切り `_` のtext保持と同じ整理)。
#[test]
fn string_literal_produces_single_str_token() {
    // Arrange
    let source = "\"abc\"";

    // Act
    let tokens = lex(source).expect("文字列リテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Text {
                text: "abc".to_string(),
                span: Span { start: 1, end: 4 },
            }]),
            text: "\"abc\"".to_string(),
            span: Span { start: 0, end: 5 },
        }]
    );
}

/// 文字列リテラル内のエスケープ `\"` は文字列を閉じないこと(仕様1章1.7:
/// エスケープ `\n` `\t` `\r` `\\` `\"` `\$` `\u{H}`)。
/// textはエスケープを解決しない生の字面(値への変換はコード生成側の責務——
/// 桁区切り `_` のtext保持と同じ整理)。
#[test]
fn escaped_quote_does_not_terminate_string() {
    // Arrange
    let source = "\"a\\\"b\"";

    // Act
    let tokens =
        lex(source).expect("エスケープされた `\\\"` を含む文字列の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Text {
                text: "a\\\"b".to_string(),
                span: Span { start: 1, end: 5 },
            }]),
            text: "\"a\\\"b\"".to_string(),
            span: Span { start: 0, end: 6 },
        }]
    );
}

/// 文字列リテラル内のエスケープが一覧(`\n` `\t` `\r` `\\` `\"` `\$` `\u{H}`)に
/// 無い文字のときはエラーE0111になり、`\` から始まる2バイト(バックスラッシュ+
/// 違反文字)を位置として報告すること(仕様1章L-14〔負例: invalid-escape〕)。
/// 「近い正解の案内」はエラーメッセージ実装のサイクルで検証する。
#[test]
fn invalid_escape_reports_e0111_with_span() {
    // Arrange
    let source = "\"a\\qb\"";

    // Act
    let err = lex(source).expect_err("一覧に無いエスケープ `\\q` を含む文字列はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0111,
            span: Span { start: 2, end: 4 },
        }
    );
}

/// 文字列リテラル中に生の改行(LF)が現れたときはエラーE0108になり、
/// 違反した改行1バイトを位置として報告すること
/// (仕様1章L-16〔負例: string-raw-newline〕)。
/// CRLF・単独CRの形は網で追加する(L-16はE0117より優先=ADR-0041)。
#[test]
fn raw_newline_in_string_reports_e0108_with_span() {
    // Arrange
    let source = "\"a\nb\"";

    // Act
    let err = lex(source).expect_err("文字列リテラル中の生の改行はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0108,
            span: Span { start: 2, end: 3 },
        }
    );
}

/// 閉じ `"` の前にファイルが終わったときはエラーE0108になり、開き `"` 1バイトを
/// 位置として報告すること(仕様1章L-16〔負例: string-unterminated-eof〕)。
/// spanが開き `"` を指すのは「どこから始まった文字列が閉じていないか」を示すため
/// (Rustコンパイラの流儀に合わせる)。
#[test]
fn unterminated_string_at_eof_reports_e0108_with_span() {
    // Arrange
    let source = "\"abc";

    // Act
    let err = lex(source).expect_err("閉じクォートの前にEOFに達した文字列はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0108,
            span: Span { start: 0, end: 1 },
        }
    );
}

/// `\u{H}` エスケープのHが16進1〜6桁でない・U+10FFFF超・サロゲート域
/// U+D800〜DFFFのいずれかのときエラーE0112になり、`\u{H}` エスケープ全体
/// (`\` から `}` まで)を位置として報告すること(仕様1章L-15〔負例: unicode-escape-range〕)。
/// 代表ケースとしてU+10FFFF超(`\u{110000}`)を検証する
/// (16進桁数違反・サロゲート域は後続サイクルで同関数に追加する)。
#[test]
fn unicode_escape_out_of_range_reports_e0112_with_span() {
    // Arrange
    let source = "\"\\u{110000}\"";

    // Act
    let err = lex(source).expect_err("U+10FFFFを超える`\\u{H}`はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 1, end: 11 },
        }
    );
}

/// `\u{H}` エスケープのHがサロゲート域U+D800〜U+DFFFのいずれかのときエラーE0112になり、
/// `\u{H}` エスケープ全体(`\` から `}` まで)を位置として報告すること
/// (仕様1章L-15〔負例: unicode-escape-range〕)。
/// サロゲートはUTF-16のペア用符号位置であり単独では文字を表さないため、
/// 範囲としては有効でも(U+10FFFF以下でも)拒否する。
#[test]
fn unicode_escape_surrogate_reports_e0112_with_span() {
    // Arrange
    let source = "\"\\u{D800}\"";

    // Act
    let err = lex(source).expect_err("サロゲート域`\\u{D800}`はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 1, end: 9 },
        }
    );
}

/// `\u{H}` エスケープのHが16進1〜6桁の形式を満たさないとき(代表ケース: 空の波括弧
/// `\u{}` で0桁)エラーE0112になり、`\u{H}` エスケープ全体(`\` から `}` まで)を
/// 位置として報告すること(仕様1章L-15〔負例: unicode-escape-range〕)。
/// 他の形式違反(`{`なし・7桁以上・閉じ`}`なし)はGreen実装後に網で固定する。
#[test]
fn unicode_escape_empty_braces_reports_e0112_with_span() {
    // Arrange
    let source = "\"\\u{}\"";

    // Act
    let err = lex(source).expect_err("空の波括弧`\\u{}`はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 1, end: 5 },
        }
    );
}

/// 空文字列リテラル `""` が1個のStrトークンになること(仕様1章1.7)。
/// Redを経ていない後追いの回帰テスト(開き直後の閉じの境界)。
#[test]
fn empty_string_literal_produces_single_str_token() {
    // Arrange
    let source = "\"\"";

    // Act
    let tokens = lex(source).expect("空文字列リテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![]),
            text: "\"\"".to_string(),
            span: Span { start: 0, end: 2 },
        }]
    );
}

/// 直後が `{` でない `$` はただの文字であること(仕様1章L-28〔正例: dollar-literal〕)。
/// Redを経ていない後追いの回帰テスト(補間実装のサイクルでも壊れてはならない境界)。
#[test]
fn dollar_without_brace_is_literal_in_string() {
    // Arrange
    let source = "\"$5\"";

    // Act
    let tokens = lex(source).expect("`$5` を含む文字列の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Text {
                text: "$5".to_string(),
                span: Span { start: 1, end: 3 },
            }]),
            text: "\"$5\"".to_string(),
            span: Span { start: 0, end: 4 },
        }]
    );
}

/// 許可エスケープ7種のうち `\"` 以外の6種(`\n` `\t` `\r` `\\` `\$` `\u{H}`)が透過され、
/// textが生の字面のまま保持されること(仕様1章1.7。`\"` は
/// escaped_quote_does_not_terminate_string が個別に固定)。
/// Redを経ていない後追いの回帰テスト。
#[test]
fn valid_escapes_pass_through_with_raw_text() {
    // Arrange
    let source = "\"a\\n\\t\\r\\\\\\$\\u{3042}z\"";

    // Act
    let tokens = lex(source).expect("許可エスケープ一式の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Text {
                text: "a\\n\\t\\r\\\\\\$\\u{3042}z".to_string(),
                span: Span { start: 1, end: 21 },
            }]),
            text: "\"a\\n\\t\\r\\\\\\$\\u{3042}z\"".to_string(),
            span: Span { start: 0, end: 22 },
        }]
    );
}

/// 文字列中のCRLF(の `\r`)もE0108になること(仕様1章L-16: CR形を含む
/// 〔負例: string-raw-newline〕。文字列内ではE0108がE0117より優先=ADR-0041)。
/// Redを経ていない後追いの回帰テスト(実装はLF/CRを同分岐で処理)。
#[test]
fn crlf_in_string_reports_e0108_with_span() {
    // Arrange
    let source = "\"a\r\nb\"";

    // Act
    let err = lex(source).expect_err("文字列中のCRLFはエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0108,
            span: Span { start: 2, end: 3 },
        }
    );
}

/// `\u` の直後に `{` が無い形もE0112になること(仕様1章L-15〔負例: unicode-escape-range〕)。
/// spanは `\u` の2バイト。Redを経ていない後追いの回帰テスト(一様形式検証の分岐網羅)。
#[test]
fn unicode_escape_missing_brace_reports_e0112_with_span() {
    // Arrange
    let source = "\"\\uZ\"";

    // Act
    let err = lex(source).expect_err("`\\u` の直後に `{` が無い形はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 1, end: 3 },
        }
    );
}

/// `\u{H}` のHが7桁以上の形もE0112になること(仕様1章L-15〔負例: unicode-escape-range〕)。
/// spanはエスケープ全体。入力は7桁だが**値はU+0041で範囲内**の `0000041` を使い、
/// 範囲チェックに隠れず桁数規則を単独で固定する(impl-reviewの変異テストで検出された穴)。
/// Redを経ていない後追いの回帰テスト。
#[test]
fn unicode_escape_too_many_digits_reports_e0112_with_span() {
    // Arrange
    let source = "\"\\u{0000041}\"";

    // Act
    let err = lex(source).expect_err("`\\u{H}` のHが7桁の形はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 1, end: 12 },
        }
    );
}

/// `\u{H}` の**有効側の境界値**(最大U+10FFFF・サロゲート直下U+D7FF・直上U+E000)が
/// すべて通ること(仕様1章L-15の通る側)。境界を1つずらす退行(>= 化・端の増減)は
/// この1本のどれかのエスケープがエラー化して検知される(変異テストで検出された穴)。
#[test]
fn unicode_escape_boundary_values_are_accepted() {
    // Arrange
    let source = "\"\\u{10FFFF}\\u{D7FF}\\u{E000}\"";

    // Act
    let tokens = lex(source).expect("境界値の `\\u{H}` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Text {
                text: "\\u{10FFFF}\\u{D7FF}\\u{E000}".to_string(),
                span: Span { start: 1, end: 27 },
            }]),
            text: "\"\\u{10FFFF}\\u{D7FF}\\u{E000}\"".to_string(),
            span: Span { start: 0, end: 28 },
        }]
    );
}

/// サロゲート域の**上端**U+DFFFもE0112になること(仕様1章L-15〔負例: unicode-escape-range〕。
/// 下端U+D800は既存テストが固定。上端の縮小退行を検知する境界テスト)。
#[test]
fn unicode_escape_surrogate_upper_end_reports_e0112_with_span() {
    // Arrange
    let source = "\"\\u{DFFF}\"";

    // Act
    let err = lex(source).expect_err("サロゲート上端`\\u{DFFF}`はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 1, end: 9 },
        }
    );
}

/// `\u{` の後に閉じ `}` を得られないまま文字列が終わる形のspanが「消費済みの末尾」で
/// 止まること(仕様1章L-15注・E0112がL-16より優先。span規則はcheck_unicode_escapeのdocに固定)。
/// Redを経ていない後追いの回帰テスト(一様形式検証の3番目のspan規則)。
#[test]
fn unicode_escape_unclosed_reports_e0112_with_span() {
    // Arrange
    let source = "\"\\u{12";

    // Act
    let err = lex(source).expect_err("閉じ `}` の無い `\\u{` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 1, end: 6 },
        }
    );
}

/// `\$` に波括弧が続く形 `\${x}` が補間にならず、生の字面のままStrになること
/// (仕様1章L-28・1.7サンプル `"値は \${price} で参照"`)。
/// 補間(L-17)実装が `$` の先読みで直前の `\` を見落とす退行をこのテストが検知する。
#[test]
fn escaped_dollar_before_brace_is_literal_in_string() {
    // Arrange
    let source = "\"\\${x}\"";

    // Act
    let tokens = lex(source).expect("`\\${x}` を含む文字列の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Text {
                text: "\\${x}".to_string(),
                span: Span { start: 1, end: 6 },
            }]),
            text: "\"\\${x}\"".to_string(),
            span: Span { start: 0, end: 7 },
        }]
    );
}

/// `\` の直後の**単独CR**もE0108(CR1バイトのspan)になること(仕様1章L-16)。
/// LF形は backslash_before_raw_newline_reports_e0108_with_span が固定済みで、
/// CR側だけ検知を外す退行(変異N03)をこのテストが殺す。
#[test]
fn backslash_before_lone_cr_reports_e0108_with_span() {
    // Arrange
    let source = "\"\\\r\"";

    // Act
    let err = lex(source).expect_err("`\\` 直後の単独`\\r`はE0108としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0108,
            span: Span { start: 2, end: 3 },
        }
    );
}

/// `\u{` の内側に生の改行が来てもE0112がL-16(E0108)より優先すること
/// (仕様1章L-15注の改行形。EOF形は unicode_escape_unclosed_... が固定済み)。
#[test]
fn unicode_escape_broken_by_newline_reports_e0112_with_span() {
    // Arrange
    let source = "\"\\u{41\n}\"";

    // Act
    let err = lex(source).expect_err("`\\u{` の内側の改行はE0112としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 1, end: 6 },
        }
    );
}

/// 文字列中の**単独CR**もE0108になること(仕様1章L-16のCR形〔負例: string-raw-newline〕)。
/// CRLFは文字列外でも合法なため、E0108がE0117(L-29)より優先する事実を
/// 検証できるのはこの単独CR形だけ(impl-reviewの指摘による追加)。
#[test]
fn lone_cr_in_string_reports_e0108_with_span() {
    // Arrange
    let source = "\"a\rb\"";

    // Act
    let err = lex(source).expect_err("文字列中の単独`\\r`はE0108としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0108,
            span: Span { start: 2, end: 3 },
        }
    );
}

/// 文字列リテラルの日本語は制限されないこと(仕様1章L-6の但し書き)。
/// spanはバイト単位(5文字×3バイト+クォート2=17バイト)。
/// Redを経ていない後追いの回帰テスト。
#[test]
fn japanese_string_literal_is_allowed() {
    // Arrange
    let source = "\"こんにちは\"";

    // Act
    let tokens = lex(source).expect("日本語文字列の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Text {
                text: "こんにちは".to_string(),
                span: Span { start: 1, end: 16 },
            }]),
            text: "\"こんにちは\"".to_string(),
            span: Span { start: 0, end: 17 },
        }]
    );
}

/// `\` の直後に生の改行が来たときは、一覧に無いエスケープ文字(E0111)ではなく
/// L-16の生の改行として扱われ、エラーE0108になること。spanは違反した改行1バイト
/// (`\` を含まない)を指すこと(仕様1章L-16〔負例: string-raw-newline〕)。
/// 実装バグの再現: 現状は`\`直後の1文字を無条件に「一覧に無いエスケープ」とみなし
/// E0111・span 1..3(`\`から改行直後まで)を返す。この span は改行をまたぐため、
/// エラー表示が違反行を単独で抜き出せない不正な形になる。
#[test]
fn backslash_before_raw_newline_reports_e0108_with_span() {
    // Arrange
    let source = "\"\\\n\"";

    // Act
    let err = lex(source).expect_err("`\\` の直後に生の改行が来た文字列はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0108,
            span: Span { start: 2, end: 3 },
        }
    );
}

/// `\` の直後にファイルが終わったときは、一覧に無いエスケープ文字(E0111)ではなく
/// L-16の未終端文字列(EOF形)として扱われ、エラーE0108になること。spanは開き `"`
/// 1バイト(未終端文字列の流儀。仕様1章L-16〔負例: string-unterminated-eof〕)。
/// 実装バグの再現: 現状は「一覧に無いエスケープ文字」の分岐に落ち、存在しない
/// 違反文字を指そうとしてE0111・span 1..2(`\` の1バイトのみ)を返す。
#[test]
fn backslash_at_eof_reports_e0108_with_span() {
    // Arrange
    let source = "\"\\";

    // Act
    let err = lex(source).expect_err("`\\` の直後にEOFに達した文字列はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0108,
            span: Span { start: 0, end: 1 },
        }
    );
}

/// 区切り記号7種 `( ) [ ] { } ,` がそれぞれ1文字のトークンになること(仕様1章1.9)。
/// 補間L-17の「トークン列上での括弧対応」の前提工事。
#[test]
fn punctuation_tokens_are_lexed_individually() {
    // Arrange
    let source = "([{,}])";

    // Act
    let tokens = lex(source).expect("区切り記号7種の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::LBracket,
                text: "[".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::RBracket,
                text: "]".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 6, end: 7 },
            },
        ]
    );
}

/// 1文字演算子12種 `+ - * / % < > ! ? . | :` がそれぞれ個別のトークンになること(仕様1章1.9)。
/// 1.9の1文字演算子は `=` を含めて13種だが、`=`(Eq)は既存のlet系テスト
/// (keyword_let_is_distinguished_from_identifiers 等)が担保するため、この正例からは除く。
#[test]
fn one_char_operators_are_lexed_individually() {
    // Arrange
    let source = "+ - * / % < > ! ? . | :";

    // Act
    let tokens = lex(source).expect("1文字演算子12種の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Plus,
                text: "+".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Minus,
                text: "-".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Star,
                text: "*".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Slash,
                text: "/".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Percent,
                text: "%".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Lt,
                text: "<".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Gt,
                text: ">".to_string(),
                span: Span { start: 12, end: 13 },
            },
            Token {
                kind: TokenKind::Bang,
                text: "!".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::Question,
                text: "?".to_string(),
                span: Span { start: 16, end: 17 },
            },
            Token {
                kind: TokenKind::Dot,
                text: ".".to_string(),
                span: Span { start: 18, end: 19 },
            },
            Token {
                kind: TokenKind::Pipe,
                text: "|".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::Colon,
                text: ":".to_string(),
                span: Span { start: 22, end: 23 },
            },
        ]
    );
}

/// 隣接する2文字演算子は1文字の前置きに勝つ最長一致で切り出されること
/// (仕様1章L-2〔正例: longest-match〕)。空白なしの `a<=b==c` は `a` `<=` `b` `==` `c` の
/// 5トークンになり、`<` と `=` に分割されないこと。
#[test]
fn adjacent_two_char_operators_win_over_one_char_prefix() {
    // Arrange
    let source = "a<=b==c";

    // Act
    let tokens = lex(source).expect("`a<=b==c` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::LtEq,
                text: "<=".to_string(),
                span: Span { start: 1, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::EqEq,
                text: "==".to_string(),
                span: Span { start: 4, end: 6 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "c".to_string(),
                span: Span { start: 6, end: 7 },
            },
        ]
    );
}

/// `..` の消極規則により、整数直後の `..` は数値に取り込まれず独立した
/// DotDotトークンになること(仕様1章L-3〔正例: longest-match〕)。`0..10` は
/// `0` `..` `10` の3トークンになる。
/// 注: float実装後の境界(`1.5..2` 等)はfloatサイクルで追加検証する。
#[test]
fn dotdot_after_integer_splits_range() {
    // Arrange
    let source = "0..10";

    // Act
    let tokens = lex(source).expect("`0..10` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "0".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::DotDot,
                text: "..".to_string(),
                span: Span { start: 1, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "10".to_string(),
                span: Span { start: 3, end: 5 },
            },
        ]
    );
}

/// 2文字演算子13種がそれぞれ1個のトークンとして切り出されること(仕様1章1.9)。
#[test]
fn two_char_operators_are_lexed_individually() {
    // Arrange
    let source = "== != <= >= && || += -= *= /= %= => ..";

    // Act
    let tokens = lex(source).expect("2文字演算子13種の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::EqEq,
                text: "==".to_string(),
                span: Span { start: 0, end: 2 },
            },
            Token {
                kind: TokenKind::BangEq,
                text: "!=".to_string(),
                span: Span { start: 3, end: 5 },
            },
            Token {
                kind: TokenKind::LtEq,
                text: "<=".to_string(),
                span: Span { start: 6, end: 8 },
            },
            Token {
                kind: TokenKind::GtEq,
                text: ">=".to_string(),
                span: Span { start: 9, end: 11 },
            },
            Token {
                kind: TokenKind::AmpAmp,
                text: "&&".to_string(),
                span: Span { start: 12, end: 14 },
            },
            Token {
                kind: TokenKind::PipePipe,
                text: "||".to_string(),
                span: Span { start: 15, end: 17 },
            },
            Token {
                kind: TokenKind::PlusEq,
                text: "+=".to_string(),
                span: Span { start: 18, end: 20 },
            },
            Token {
                kind: TokenKind::MinusEq,
                text: "-=".to_string(),
                span: Span { start: 21, end: 23 },
            },
            Token {
                kind: TokenKind::StarEq,
                text: "*=".to_string(),
                span: Span { start: 24, end: 26 },
            },
            Token {
                kind: TokenKind::SlashEq,
                text: "/=".to_string(),
                span: Span { start: 27, end: 29 },
            },
            Token {
                kind: TokenKind::PercentEq,
                text: "%=".to_string(),
                span: Span { start: 30, end: 32 },
            },
            Token {
                kind: TokenKind::FatArrow,
                text: "=>".to_string(),
                span: Span { start: 33, end: 35 },
            },
            Token {
                kind: TokenKind::DotDot,
                text: "..".to_string(),
                span: Span { start: 36, end: 38 },
            },
        ]
    );
}

/// `===` はE0116として記号列全体のspanで報告されること
/// (仕様1章L-26〔負例: triple-equals〕)。
/// `==`+`=` の正当な2トークンに分割せず、記号列全体をエラーにする
/// (エラーメッセージ層が `==` への修正候補を出すための精度)。
#[test]
fn triple_equals_reports_e0116_with_full_span() {
    // Arrange
    let source = "a === b";

    // Act
    let err = lex(source).expect_err("`===` は記号列全体がエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 2, end: 5 },
        }
    );
}

/// `!==` はE0116として記号列全体のspanで報告されること
/// (仕様1章L-26〔負例: triple-not-equals〕・ADR-0047決定2)。
/// `!`+`==` の2トークンに分割せず `!==` 全体をエラーにする
/// (`!=` への修正候補を出すための精度。`===` と対になる)。
#[test]
fn triple_not_equals_reports_e0116_with_full_span() {
    // Arrange
    let source = "a !== b";

    // Act
    let err = lex(source).expect_err("`!==` は記号列全体がエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 2, end: 5 },
        }
    );
}

/// `->` はE0116として記号列全体のspanで報告されること(仕様1章L-26〔負例: arrow-token〕)。
/// `-`+`>` に分割せず `->` 全体をエラーにする
/// (`=>`・空白区切り戻り値型への誘導のための精度)。
#[test]
fn arrow_reports_e0116_with_full_span() {
    // Arrange
    let source = "a -> b";

    // Act
    let err = lex(source).expect_err("`->` は記号列全体がエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 2, end: 4 },
        }
    );
}

/// 単独の `&`(直後が `&` でない)はE0116のままであること(仕様1章1.9に単独 `&` は
/// 無い——`|` は union型で単独が正当だが `&` は `&&` のみ、という非対称の固定)。
/// Redを経ていない後追いの回帰テスト(`&&` の実装が単独 `&` を誤って受理しないことの網)。
#[test]
fn lone_ampersand_reports_e0116_with_span() {
    // Arrange
    let source = "a & b";

    // Act
    let err = lex(source).expect_err("単独の `&` はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 2, end: 3 },
        }
    );
}

/// 文字列中の `${式}` がInterpセグメント(再帰トークン化した式を内包)になること
/// (仕様1章L-17〔正例: interpolation-nested の基本形〕・ADR-0042)。
/// Interpのspanは `${` から対応する `}` まで、内側トークンのspanはソース絶対位置。
#[test]
fn interpolation_produces_interp_segment_with_recursive_tokens() {
    // Arrange
    let source = "\"a${x}b\"";

    // Act
    let tokens = lex(source).expect("補間を含む文字列リテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![
                StrSegment::Text {
                    text: "a".to_string(),
                    span: Span { start: 1, end: 2 },
                },
                StrSegment::Interp {
                    tokens: vec![Token {
                        kind: TokenKind::Ident,
                        text: "x".to_string(),
                        span: Span { start: 4, end: 5 },
                    }],
                    span: Span { start: 2, end: 6 },
                },
                StrSegment::Text {
                    text: "b".to_string(),
                    span: Span { start: 6, end: 7 },
                },
            ]),
            text: "\"a${x}b\"".to_string(),
            span: Span { start: 0, end: 8 },
        }]
    );
}

/// 補間内にネストした文字列リテラルを書けること(仕様1章L-17〔正例: interpolation-nested〕)。
/// ネスト文字列は再帰トークン化が文字列分岐を再度通ることで特別扱いなしに成立する。
/// Redを経ていない後追いの回帰テスト(実装が最初から満たすことを実測済み)。
#[test]
fn nested_string_inside_interpolation_is_tokenized() {
    // Arrange
    let source = "\"${f(\"x\")}\"";

    // Act
    let tokens = lex(source).expect("ネスト文字列入り補間の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Interp {
                tokens: vec![
                    Token {
                        kind: TokenKind::Ident,
                        text: "f".to_string(),
                        span: Span { start: 3, end: 4 },
                    },
                    Token {
                        kind: TokenKind::LParen,
                        text: "(".to_string(),
                        span: Span { start: 4, end: 5 },
                    },
                    Token {
                        kind: TokenKind::Str(vec![StrSegment::Text {
                            text: "x".to_string(),
                            span: Span { start: 6, end: 7 },
                        }]),
                        text: "\"x\"".to_string(),
                        span: Span { start: 5, end: 8 },
                    },
                    Token {
                        kind: TokenKind::RParen,
                        text: ")".to_string(),
                        span: Span { start: 8, end: 9 },
                    },
                ],
                span: Span { start: 1, end: 10 },
            }]),
            text: "\"${f(\"x\")}\"".to_string(),
            span: Span { start: 0, end: 11 },
        }]
    );
}

/// ネスト文字列の**中身の括弧文字**は補間の括弧対応に数えられないこと
/// (仕様1章L-17「エスケープの中の括弧『文字』は数えない」の文字列側。
/// 対応はトークン列上で数えるため、文字列内の `(` はStrの中身であり深度に影響しない)。
/// Redを経ていない後追いの回帰テスト。
#[test]
fn paren_inside_nested_string_is_not_counted_for_matching() {
    // Arrange
    let source = "\"${f(\"(\")}\"";

    // Act
    let tokens = lex(source).expect("文字列内括弧入り補間の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Interp {
                tokens: vec![
                    Token {
                        kind: TokenKind::Ident,
                        text: "f".to_string(),
                        span: Span { start: 3, end: 4 },
                    },
                    Token {
                        kind: TokenKind::LParen,
                        text: "(".to_string(),
                        span: Span { start: 4, end: 5 },
                    },
                    Token {
                        kind: TokenKind::Str(vec![StrSegment::Text {
                            text: "(".to_string(),
                            span: Span { start: 6, end: 7 },
                        }]),
                        text: "\"(\"".to_string(),
                        span: Span { start: 5, end: 8 },
                    },
                    Token {
                        kind: TokenKind::RParen,
                        text: ")".to_string(),
                        span: Span { start: 8, end: 9 },
                    },
                ],
                span: Span { start: 1, end: 10 },
            }]),
            text: "\"${f(\"(\")}\"".to_string(),
            span: Span { start: 0, end: 11 },
        }]
    );
}

/// 補間内の角括弧 `[ ]` が深度として対応づけられること(仕様1章L-17
/// 〔正例: interpolation-nested〕。仕様の3例目=`or` 入りの完全形は
/// spec_third_interpolation_example_with_keyword_or が固定)。
/// Redを経ていない後追いの回帰テスト。
#[test]
fn brackets_inside_interpolation_are_depth_matched() {
    // Arrange
    let source = "\"${m[k]}\"";

    // Act
    let tokens = lex(source).expect("角括弧入り補間の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Interp {
                tokens: vec![
                    Token {
                        kind: TokenKind::Ident,
                        text: "m".to_string(),
                        span: Span { start: 3, end: 4 },
                    },
                    Token {
                        kind: TokenKind::LBracket,
                        text: "[".to_string(),
                        span: Span { start: 4, end: 5 },
                    },
                    Token {
                        kind: TokenKind::Ident,
                        text: "k".to_string(),
                        span: Span { start: 5, end: 6 },
                    },
                    Token {
                        kind: TokenKind::RBracket,
                        text: "]".to_string(),
                        span: Span { start: 6, end: 7 },
                    },
                ],
                span: Span { start: 1, end: 8 },
            }]),
            text: "\"${m[k]}\"".to_string(),
            span: Span { start: 0, end: 9 },
        }]
    );
}

/// 補間 `${` を開いたまま対応する `}` を得ずにファイルが終わったときはエラーE0109になり、
/// `${` 2バイトを位置として報告すること(仕様1章L-18〔負例: unterminated-interpolation〕。
/// 補間の内側ではE0109がL-16=E0108より優先する)。spanが `${` を指すのは
/// 「どこで始まった補間が閉じていないか」を示すため(未終端文字列が開き `"` を指す流儀と統一)。
#[test]
fn unterminated_interpolation_at_eof_reports_e0109_with_span() {
    // Arrange
    let source = "\"${x";

    // Act
    let err = lex(source).expect_err("閉じ`}`の前にEOFに達した補間はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 1, end: 3 },
        }
    );
}

/// 補間内の改行はNewlineトークンにせずE0109(L-17(a)の物理1行制約)にすること。
/// 補間 `${...}` は1物理行に収まらなければならず(仕様1章L-17(a))、内側に生の改行が
/// 現れた時点で対応する `}` を得られない未終端として扱う(仕様1章L-18
/// 〔負例: unterminated-interpolation〕)。spanは `${` 2バイト
/// (未終端文字列が開き `"` を指す流儀と統一)。
#[test]
fn raw_newline_inside_interpolation_reports_e0109_with_span() {
    // Arrange
    let source = "\"${a\nb}\"";

    // Act
    let err = lex(source).expect_err("補間内の生の改行はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 1, end: 3 },
        }
    );
}

/// 補間の内側にコメント `//` を書けないこと(仕様1章L-17(b)〔負例: interpolation-comment〕)。
/// 補間 `${...}` の内側は通常の字句モードでトークン化するが、行コメントは対応外——
/// `//` を許すと閉じ`}`まで(実装次第ではファイル末尾まで)コメント扱いで飲み込まれ、
/// 補間が閉じないまま黙って壊れるため、E0115として明示的に拒否する。
/// spanは `//` の2バイトを指す。
#[test]
fn comment_inside_interpolation_reports_e0115_with_span() {
    // Arrange
    let source = "\"${1 // c}\"";

    // Act
    let err = lex(source).expect_err("補間内の`//`はエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0115,
            span: Span { start: 5, end: 7 },
        }
    );
}

/// 空の補間 `${}` は空のトークン列を持つInterpセグメントになること(仕様1章L-17)。
/// 空の式を拒否するのはパーサの担当で、字句は事実をそのまま伝える。
/// Redを経ていない後追いの回帰テスト(境界: 補間ループが1周も回らない形)。
#[test]
fn empty_interpolation_produces_interp_with_no_tokens() {
    // Arrange
    let source = "\"${}\"";

    // Act
    let tokens = lex(source).expect("空の補間の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Interp {
                tokens: vec![],
                span: Span { start: 1, end: 4 },
            }]),
            text: "\"${}\"".to_string(),
            span: Span { start: 0, end: 5 },
        }]
    );
}

/// 補間の内側で発生した字句エラー(ネスト文字列のE0112)がE0109より優先されること
/// (仕様1章L-18注: より具体的な原因を指す。実装は内側エラーの素直な伝播)。
/// Redを経ていない後追いの回帰テスト(宿題だった優先順位の固定)。
#[test]
fn inner_lexical_error_takes_priority_over_e0109() {
    // Arrange
    let source = "\"${\"\\u{}\"}\"";

    // Act
    let err = lex(source).expect_err("補間内のネスト文字列の\\u{}はE0112としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0112,
            span: Span { start: 4, end: 8 },
        }
    );
}

/// 補間内で対応する開き括弧を持たない閉じ括弧(`)` `]` `}`)が現れたときはE0109に
/// なり、spanはその閉じ括弧1バイトを指すこと(仕様1章L-18〔負例: unterminated-interpolation〕。
/// L-18は現状「対応するE0109」の分岐が未実装で、この閉じ括弧はサイレントに無視されてOkに
/// なる——本テストが再現するimpl-review検出バグ)。
#[test]
fn unmatched_closer_in_interpolation_reports_e0109() {
    // Arrange
    let source = "\"${)}\"";

    // Act
    let err = lex(source).expect_err("対応する開きを持たない`)`はE0109としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 3, end: 4 },
        }
    );
}

/// 補間内で開き括弧と種類が一致しない閉じ括弧が現れたときもE0109になり、spanは
/// その閉じ括弧1バイトを指すこと(`(` に対する `}` は種類不一致。仕様1章L-18
/// 〔負例: unterminated-interpolation〕。現状は種類を照合せず深度だけで対応づけるため、
/// この`}`を`(`の対応閉じとして飲み込んでしまい、後続の閉じ`"`まで補間扱いで消費し、
/// 離れた位置のE0108(未終端文字列)になる——本テストが再現するimpl-review検出バグ)。
#[test]
fn mismatched_closer_in_interpolation_reports_e0109() {
    // Arrange
    let source = "\"${f(a}\"";

    // Act
    let err = lex(source).expect_err("`(`に対して種類の異なる`}`はE0109としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 6, end: 7 },
        }
    );
}

/// 補間内で開き括弧 `(` に対して種類の異なる閉じ `]` が現れたときもE0109になること
/// (仕様1章L-18〔負例: unterminated-interpolation〕。現状は種類を照合しないため
/// `]`をただの不均衡減算として無視し、後続の`}`を`(`の対応閉じとして受理してOkに
/// なる——本テストが再現するimpl-review検出バグ)。
#[test]
fn mismatched_bracket_pair_in_interpolation_reports_e0109() {
    // Arrange
    let source = "\"${(]}\"";

    // Act
    let err = lex(source).expect_err("`(`に対して種類の異なる`]`はE0109としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 4, end: 5 },
        }
    );
}

/// 補間内の括弧照合は、スタックの最も内側の開き括弧とだけ対応すること。`"${([a)b]}"`
/// では `)` が現れた時点の対応待ちが `[` であるため、種類の合わない閉じ括弧として
/// 最初に違反するのは `)` であり、E0109 の span はその `)` 1バイトを指すこと
/// (仕様1章L-18「種類の合わない閉じ括弧…が現れた状態…このときのspanはその閉じ括弧を
/// 指す」〔負例: unterminated-interpolation〕・ADR-0047決定1)。
/// 既存の mismatched_bracket_pair_in_interpolation_reports_e0109 は深さ1(`"${(]}"`)
/// しか検証しないため、閉じ括弧をスタック末尾から探索して同種の開きが見つかればそこまで
/// 戻す実装(この入力を `8..9` の `]` でE0109にする誤実装)を検出できない——
/// impl-review 2026-08-18 の持ち帰りを塞ぐピン(真の原因より後ろにずれたspanを拒む)。
#[test]
fn mismatched_closer_under_deeper_stack_reports_e0109_at_first_violation() {
    // Arrange
    let source = "\"${([a)b]}\"";

    // Act
    let err = lex(source).expect_err("深さ2のスタックでの種類不一致はE0109としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 6, end: 7 },
        }
    );
}

/// 文字列と補間のネストが実装上限64段を超えたときはE0118になり、spanは上限を超えた
/// 65段目の開き `"` 1バイトを指すこと(仕様1章L-31〔負例: nesting-limit〕)。
/// 上限64段は、深さ約500段で `cargo test` がスタックオーバーフローにより
/// SIGABRTでプロセスごと落ちる実測に基づき、安全側に十分な余裕を持たせて設定した値。
/// 入力は手書きできる長さでないため`repeat`で構築する(このテストのみ許容)。
#[test]
fn nesting_depth_limit_reports_e0118() {
    // Arrange
    // `"${` を65回・`x`・`}"` を65回で、文字列→補間→文字列→…と65段ネストさせる。
    // 65段目(0始まりで64段目)の開き`"`は 64 * 3 = 192バイト目。上限64段を1段超える。
    let source = "\"${".repeat(65) + "x" + &"}\"".repeat(65);

    // Act
    let err = lex(&source).expect_err("ネスト65段はE0118としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0118,
            span: Span {
                start: 192,
                end: 193
            },
        }
    );
}

/// 補間内の単独CR(直後が`\n`でない`\r`)は、LF・CRLFと同じ「補間内の生の改行」として
/// E0109(未終端。L-17(a)の物理1行制約)に統一すること(仕様1章L-17(a)・L-18
/// 〔負例: unterminated-interpolation〕。現状はCRを補間内の他の文字と同様の通常字句
/// モードで扱ってしまい、単独CR自体の規則であるE0117(L-29)が先に発生する——
/// 本テストが再現するimpl-review検出バグ)。
#[test]
fn lone_cr_inside_interpolation_reports_e0109() {
    // Arrange
    let source = "\"${a\rb}\"";

    // Act
    let err = lex(source).expect_err("補間内の単独`\\r`はE0109としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 1, end: 3 },
        }
    );
}

/// 補間内の `{ }` ブロックが深度として対応づけられ、内側の `}` で補間が閉じないこと
/// (仕様1章L-17「トークン列上で括弧類の対応を数える」の`{}`側。
/// impl-reviewの変異テストで「LBraceを深度から外しても全テスト緑」と実証された穴を塞ぐ)。
#[test]
fn brace_block_inside_interpolation_is_depth_matched() {
    // Arrange
    let source = "\"${a{b}c}\"";

    // Act
    let tokens = lex(source).expect("補間内の{}ブロックの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Interp {
                tokens: vec![
                    Token {
                        kind: TokenKind::Ident,
                        text: "a".to_string(),
                        span: Span { start: 3, end: 4 },
                    },
                    Token {
                        kind: TokenKind::LBrace,
                        text: "{".to_string(),
                        span: Span { start: 4, end: 5 },
                    },
                    Token {
                        kind: TokenKind::Ident,
                        text: "b".to_string(),
                        span: Span { start: 5, end: 6 },
                    },
                    Token {
                        kind: TokenKind::RBrace,
                        text: "}".to_string(),
                        span: Span { start: 6, end: 7 },
                    },
                    Token {
                        kind: TokenKind::Ident,
                        text: "c".to_string(),
                        span: Span { start: 7, end: 8 },
                    },
                ],
                span: Span { start: 1, end: 9 },
            }]),
            text: "\"${a{b}c}\"".to_string(),
            span: Span { start: 0, end: 10 },
        }]
    );
}

/// 連続した補間 `${a}${b}` がテキスト区分を挟まず隣接するInterp 2個になること(仕様1章L-17)。
/// Redを経ていない後追いの回帰テスト(区分の隣接境界)。
#[test]
fn adjacent_interpolations_produce_two_interp_segments() {
    // Arrange
    let source = "\"${a}${b}\"";

    // Act
    let tokens = lex(source).expect("連続補間の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![
                StrSegment::Interp {
                    tokens: vec![Token {
                        kind: TokenKind::Ident,
                        text: "a".to_string(),
                        span: Span { start: 3, end: 4 },
                    }],
                    span: Span { start: 1, end: 5 },
                },
                StrSegment::Interp {
                    tokens: vec![Token {
                        kind: TokenKind::Ident,
                        text: "b".to_string(),
                        span: Span { start: 7, end: 8 },
                    }],
                    span: Span { start: 5, end: 9 },
                },
            ]),
            text: "\"${a}${b}\"".to_string(),
            span: Span { start: 0, end: 10 },
        }]
    );
}

/// 2段ネスト(補間内のネスト文字列がさらに補間を含む)が入れ子のまま成立すること
/// (仕様1章L-17・ADR-0042の再帰表現の核心形)。
#[test]
fn two_level_nested_interpolation_is_tokenized() {
    // Arrange
    let source = "\"${f(\"${g}\")}\"";

    // Act
    let tokens = lex(source).expect("2段ネスト補間の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Str(vec![StrSegment::Interp {
                tokens: vec![
                    Token {
                        kind: TokenKind::Ident,
                        text: "f".to_string(),
                        span: Span { start: 3, end: 4 },
                    },
                    Token {
                        kind: TokenKind::LParen,
                        text: "(".to_string(),
                        span: Span { start: 4, end: 5 },
                    },
                    Token {
                        kind: TokenKind::Str(vec![StrSegment::Interp {
                            tokens: vec![Token {
                                kind: TokenKind::Ident,
                                text: "g".to_string(),
                                span: Span { start: 8, end: 9 },
                            }],
                            span: Span { start: 6, end: 10 },
                        }]),
                        text: "\"${g}\"".to_string(),
                        span: Span { start: 5, end: 11 },
                    },
                    Token {
                        kind: TokenKind::RParen,
                        text: ")".to_string(),
                        span: Span { start: 11, end: 12 },
                    },
                ],
                span: Span { start: 1, end: 13 },
            }]),
            text: "\"${f(\"${g}\")}\"".to_string(),
            span: Span { start: 0, end: 14 },
        }]
    );
}

/// E0115(補間内コメント)と内側エラーは**出現順**で先のものが報告されること
/// (仕様1章L-18注2の「E0115とは出現順」の固定。この入力では `//` が
/// ネスト文字列の不正エスケープ `\q` より先に現れるためE0115)。
#[test]
fn e0115_and_inner_error_are_reported_in_order_of_appearance() {
    // Arrange
    let source = "\"${ // c\"\\q\" }\"";

    // Act
    let err = lex(source).expect_err("補間内の先行する`//`がE0115としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0115,
            span: Span { start: 4, end: 6 },
        }
    );
}

/// ネスト64段ちょうどは上限内で受理されること(仕様1章L-31の境界。65段=E0118は
/// nesting_depth_limit_reports_e0118 が固定)。
/// 注: 64段の期待トークン木の丸ごと明示は非現実的なため、例外的に
/// 「Okかつ最上位が1トークン」の形状検証に留める(理由をここに明記する規約運用)。
#[test]
fn nesting_depth_at_limit_is_accepted() {
    // Arrange
    let source = format!("{}x{}", "\"${".repeat(64), "}\"".repeat(64));

    // Act
    let tokens = lex(&source).expect("64段ちょうどのネストはエラーにならないこと");

    // Assert
    assert_eq!(tokens.len(), 1);
}

/// 補間内で閉じ`}`を忘れて直後に閉じ`"`を書いたタイポは、その閉じ`"`が
/// 幻のネスト文字列の開きとして飲み込まれるのではなく、根本原因である
/// 未終端補間としてE0109(`${`2バイト)で報告されること(仕様1章L-18改訂方針:
/// 補間内で「行または入力が終わったこと」に起因するエラーはすべてE0109に統一する。
/// 補間はL-17(a)によりこの行で閉じなければならないため、行終端系のエラーは
/// 根本原因=未終端補間を指す。仕様1章L-18〔負例: unterminated-interpolation〕。
/// 現状はこの閉じ`"`をネスト文字列の開きとして消費してしまい、ファイル末尾で
/// L-16のEOF形としてE0108(4..5、原因から離れた位置)を報告する——
/// 本テストが再現するimpl-review検出バグ(N1)。
#[test]
fn missing_brace_before_closing_quote_reports_e0109() {
    // Arrange
    let source = "\"${x\"";

    // Act
    let err = lex(source).expect_err("`}`閉じ忘れ+閉じ`\"`はE0109としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 1, end: 3 },
        }
    );
}

/// 補間内のネスト文字列の中に生の改行(LF)が現れたときも、ネスト文字列自身の
/// L-16(E0108)ではなく、根本原因である未終端補間としてE0109(最も内側の`${`
/// 2バイト)で報告されること(仕様1章L-18改訂方針・L-17(a)〔負例:
/// unterminated-interpolation〕)。現状はネスト文字列のL-16が先に効いてしまい、
/// E0108(5..6、改行1バイトの位置)を報告する——
/// 本テストが再現するimpl-review検出バグ(N1)。
#[test]
fn raw_newline_in_nested_string_inside_interpolation_reports_e0109() {
    // Arrange
    let source = "\"${\"a\nb\"}\"";

    // Act
    let err = lex(source).expect_err("補間内ネスト文字列の生の改行はE0109としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 1, end: 3 },
        }
    );
}

/// 補間内のネスト文字列の中に単独CR(直後が`\n`でない`\r`)が現れたときも、
/// L-16(E0108)ではなく根本原因である未終端補間としてE0109(最も内側の`${`
/// 2バイト)で報告されること(仕様1章L-18改訂方針・L-17(a)〔負例:
/// unterminated-interpolation〕。単独CR自体の規則E0117(L-29)ではなく、
/// 補間内の生の改行として扱う点は lone_cr_inside_interpolation_reports_e0109
/// と同じ考え方)。現状はネスト文字列のL-16が先に効いてしまい、
/// E0108(5..6、CR1バイトの位置)を報告する——
/// 本テストが再現するimpl-review検出バグ(N1)。
#[test]
fn lone_cr_in_nested_string_inside_interpolation_reports_e0109() {
    // Arrange
    let source = "\"${\"a\rb\"}\"";

    // Act
    let err = lex(source).expect_err("補間内ネスト文字列の単独`\\r`はE0109としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0109,
            span: Span { start: 1, end: 3 },
        }
    );
}

/// 回帰の網: ここまでのサイクルで実装した代表トークン種(KwLet/Ident/Eq/Int/Float/Str
/// /Newline/演算子)と桁区切り(独立したIntトークンとして)・改行2形(LF/CRLF)・
/// 日本語入り行コメント・エスケープ・補間入り文字列・演算子(1文字 `%`、2文字
/// `== && <= => ..` の最長一致)・float(小数・指数)・基数リテラル(16進)を
/// 1入力に含むスナップショット。
/// TDDサイクルの検証は上の明示的assertが担い、これは出力全体の固定のみを担う
/// (スナップショットテストはAAAマーカーの対象外)。
#[test]
fn snapshot_token_stream() {
    insta::assert_debug_snapshot!(mesh::lexer::lex(
        "let mut n = 1_000 // 合計\r\nlet msg = \"答え: ${n}円\\n\"\nn % 2 == 0 && n <= 10 => 0..n\nlet r = 2.5e-3 * 0xFF"
    ));
}

/// 二項演算子を行末に置くと文は次行へ継続し、Newlineトークンが生成されないこと
/// (仕様1章L-20〔正例: continuation-operators〕)。L-20の二項演算子14種を全列挙する——
/// 継続トークン表から1種でも漏れる実装をこの網で殺すため、代表抽出しない。
#[test]
fn binary_operators_at_line_end_continue() {
    // Arrange
    let source =
        "1 +\n2 -\n3 *\n4 /\n5 %\n6 ==\n7 !=\n8 <\n9 <=\n10 >\n11 >=\n12 &&\n13 ||\n14 |\n15";

    // Act
    let tokens = lex(source).expect("二項演算子の行末継続の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Plus,
                text: "+".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Minus,
                text: "-".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Int,
                text: "3".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Star,
                text: "*".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Int,
                text: "4".to_string(),
                span: Span { start: 12, end: 13 },
            },
            Token {
                kind: TokenKind::Slash,
                text: "/".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::Int,
                text: "5".to_string(),
                span: Span { start: 16, end: 17 },
            },
            Token {
                kind: TokenKind::Percent,
                text: "%".to_string(),
                span: Span { start: 18, end: 19 },
            },
            Token {
                kind: TokenKind::Int,
                text: "6".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::EqEq,
                text: "==".to_string(),
                span: Span { start: 22, end: 24 },
            },
            Token {
                kind: TokenKind::Int,
                text: "7".to_string(),
                span: Span { start: 25, end: 26 },
            },
            Token {
                kind: TokenKind::BangEq,
                text: "!=".to_string(),
                span: Span { start: 27, end: 29 },
            },
            Token {
                kind: TokenKind::Int,
                text: "8".to_string(),
                span: Span { start: 30, end: 31 },
            },
            Token {
                kind: TokenKind::Lt,
                text: "<".to_string(),
                span: Span { start: 32, end: 33 },
            },
            Token {
                kind: TokenKind::Int,
                text: "9".to_string(),
                span: Span { start: 34, end: 35 },
            },
            Token {
                kind: TokenKind::LtEq,
                text: "<=".to_string(),
                span: Span { start: 36, end: 38 },
            },
            Token {
                kind: TokenKind::Int,
                text: "10".to_string(),
                span: Span { start: 39, end: 41 },
            },
            Token {
                kind: TokenKind::Gt,
                text: ">".to_string(),
                span: Span { start: 42, end: 43 },
            },
            Token {
                kind: TokenKind::Int,
                text: "11".to_string(),
                span: Span { start: 44, end: 46 },
            },
            Token {
                kind: TokenKind::GtEq,
                text: ">=".to_string(),
                span: Span { start: 47, end: 49 },
            },
            Token {
                kind: TokenKind::Int,
                text: "12".to_string(),
                span: Span { start: 50, end: 52 },
            },
            Token {
                kind: TokenKind::AmpAmp,
                text: "&&".to_string(),
                span: Span { start: 53, end: 55 },
            },
            Token {
                kind: TokenKind::Int,
                text: "13".to_string(),
                span: Span { start: 56, end: 58 },
            },
            Token {
                kind: TokenKind::PipePipe,
                text: "||".to_string(),
                span: Span { start: 59, end: 61 },
            },
            Token {
                kind: TokenKind::Int,
                text: "14".to_string(),
                span: Span { start: 62, end: 64 },
            },
            Token {
                kind: TokenKind::Pipe,
                text: "|".to_string(),
                span: Span { start: 65, end: 66 },
            },
            Token {
                kind: TokenKind::Int,
                text: "15".to_string(),
                span: Span { start: 67, end: 69 },
            },
        ]
    );
}

/// キーワード演算子 or/is/in を行末に置くと継続すること(仕様1章L-20)。
/// 継続しない文(`y`・`w` の行末)ではNewlineが復活することも同時に固定する。
#[test]
fn keyword_operators_at_line_end_continue() {
    // Arrange
    let source = "let a = x or\ny\nlet b = v is\nw\nfor i in\nxs";

    // Act
    let tokens = lex(source).expect("キーワード演算子の行末継続の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "x".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::KwOr,
                text: "or".to_string(),
                span: Span { start: 10, end: 12 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "y".to_string(),
                span: Span { start: 13, end: 14 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 15, end: 18 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 19, end: 20 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 21, end: 22 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "v".to_string(),
                span: Span { start: 23, end: 24 },
            },
            Token {
                kind: TokenKind::KwIs,
                text: "is".to_string(),
                span: Span { start: 25, end: 27 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "w".to_string(),
                span: Span { start: 28, end: 29 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 29, end: 30 },
            },
            Token {
                kind: TokenKind::KwFor,
                text: "for".to_string(),
                span: Span { start: 30, end: 33 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "i".to_string(),
                span: Span { start: 34, end: 35 },
            },
            Token {
                kind: TokenKind::KwIn,
                text: "in".to_string(),
                span: Span { start: 36, end: 38 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "xs".to_string(),
                span: Span { start: 39, end: 41 },
            },
        ]
    );
}

/// 複合代入・`.`・`..`・カンマ・`=`・`=>`・開き括弧を行末に置くと継続すること
/// (仕様1章L-20。メソッドチェーンの折り返し `x.` 形を含む)。
#[test]
fn compound_assignment_and_punctuators_at_line_end_continue() {
    // Arrange
    let source = "n +=\n1\nx.\ny\n0..\nn\nq =\n1\n1,\n2\nk =>\nv\nfoo(\n1)";

    // Act
    let tokens =
        lex(source).expect("複合代入と区切り記号の行末継続の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "n".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::PlusEq,
                text: "+=".to_string(),
                span: Span { start: 2, end: 4 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "x".to_string(),
                span: Span { start: 7, end: 8 },
            },
            Token {
                kind: TokenKind::Dot,
                text: ".".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "y".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::Int,
                text: "0".to_string(),
                span: Span { start: 12, end: 13 },
            },
            Token {
                kind: TokenKind::DotDot,
                text: "..".to_string(),
                span: Span { start: 13, end: 15 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "n".to_string(),
                span: Span { start: 16, end: 17 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 17, end: 18 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "q".to_string(),
                span: Span { start: 18, end: 19 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 22, end: 23 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 23, end: 24 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 24, end: 25 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 25, end: 26 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 27, end: 28 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 28, end: 29 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "k".to_string(),
                span: Span { start: 29, end: 30 },
            },
            Token {
                kind: TokenKind::FatArrow,
                text: "=>".to_string(),
                span: Span { start: 31, end: 33 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "v".to_string(),
                span: Span { start: 34, end: 35 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 35, end: 36 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "foo".to_string(),
                span: Span { start: 36, end: 39 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 39, end: 40 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 41, end: 42 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 42, end: 43 },
            },
        ]
    );
}

/// 行末トークンの判定はコメントを除去した後のトークンに対して行うこと
/// (仕様1章L-4〔正例: comment-before-continuation〕+L-20)。
/// `+` の行末コメントを挟んでも継続する。
#[test]
fn comment_before_continuation_operator() {
    // Arrange
    let source = "1 +  // 合計\n2";

    // Act
    let tokens = lex(source).expect("コメント前の継続演算子の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Plus,
                text: "+".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 15, end: 16 },
            },
        ]
    );
}

/// 行頭に演算子を置いても継続は起きないこと(仕様1章L-23〔負例:
/// no-leading-operator-continuation〕)。継続判定は前行の行末のみ。
/// Green実装が「次行の行頭トークン」を見る誤実装に変わったとき殺すピンで、
/// 現行実装では最初から緑(Redを経ていない後追いの回帰テスト)。
#[test]
fn leading_operator_does_not_continue_previous_line() {
    // Arrange
    let source = "1\n+ 2";

    // Act
    let tokens = lex(source).expect("行頭演算子の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::Plus,
                text: "+".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 4, end: 5 },
            },
        ]
    );
}

/// 後置 `?` の行末は継続せず文を終端すること(仕様1章L-24〔挙動検証:
/// postfix-question-newline〕)。`?` はL-20の継続トークンでない)。
/// 継続トークン表にQuestionを混入させた実装を殺すピンで、
/// 現行実装では最初から緑(Redを経ていない後追いの回帰テスト)。
#[test]
fn postfix_question_terminates_statement() {
    // Arrange
    let source = "find(id)?\nlog(1)";

    // Act
    let tokens = lex(source).expect("後置?の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "find".to_string(),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "id".to_string(),
                span: Span { start: 5, end: 7 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 7, end: 8 },
            },
            Token {
                kind: TokenKind::Question,
                text: "?".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 9, end: 10 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "log".to_string(),
                span: Span { start: 10, end: 13 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 13, end: 14 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 15, end: 16 },
            },
        ]
    );
}

/// `(` の内側では行末が何であれ改行は文を終端しないこと(仕様1章L-21
/// 〔正例: multiline-call〕)。トレーリングカンマ無しの複数行呼び出し——
/// 行末が `(`・識別子・引数名いずれでもNewlineトークンは生成されない。
#[test]
fn multiline_call_without_trailing_comma_produces_no_newline() {
    // Arrange
    let source = "foo(\n    a,\n    b\n)";

    // Act
    let tokens = lex(source).expect("括弧内の改行を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "foo".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 9, end: 10 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 16, end: 17 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 18, end: 19 },
            },
        ]
    );
}

/// `[` の内側でも改行は文を終端しないこと(仕様1章L-21)。
/// `(`/`[` が別トークン種であるため、スタックが両方に対応していることを固定する。
#[test]
fn newline_inside_brackets_is_suppressed() {
    // Arrange
    let source = "[1,\n2\n]";

    // Act
    let tokens = lex(source).expect("括弧内の改行を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::LBracket,
                text: "[".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::RBracket,
                text: "]".to_string(),
                span: Span { start: 6, end: 7 },
            },
        ]
    );
}

/// CRLF行末でも `(` の内側の改行は抑制されること(仕様1章L-21・L-29)。
/// 改行の表現がLFでもCRLFでも抑制の判定が同一であることの固定
/// (impl-review 2026-08-18の改善。Redを経ていない後追いの回帰テスト)。
#[test]
fn crlf_inside_parentheses_is_suppressed() {
    // Arrange
    let source = "foo(\r\n    a,\r\n    b\r\n)";

    // Act
    let tokens = lex(source).expect("CRLF改行を含む括弧内の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "foo".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 18, end: 19 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 21, end: 22 },
            },
        ]
    );
}

/// 2段にネストした `(` の内側の改行も抑制されること(仕様1章L-21)。
/// 深度スタックが複数要素を正しく保持・消費することの固定
/// (impl-review 2026-08-18の改善。Redを経ていない後追いの回帰テスト)。
#[test]
fn nested_parens_suppress_newline() {
    // Arrange
    let source = "foo((a,\nb))";

    // Act
    let tokens = lex(source).expect("ネストした括弧内の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "foo".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 9, end: 10 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 10, end: 11 },
            },
        ]
    );
}

/// 異種括弧 `(` `[` `{` の混在下でも改行抑制の判定がスタックトップで正しく行われること
/// (仕様1章L-21)。`]` でpopした後のトップが `(` であるケースを固定する
/// (impl-review 2026-08-18の改善。Redを経ていない後追いの回帰テスト)。
#[test]
fn mixed_brackets_suppress_newline() {
    // Arrange
    let source = "f(([a],\n{b}))";

    // Act
    let tokens = lex(source).expect("異種括弧の混在する字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "f".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::LBracket,
                text: "[".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::RBracket,
                text: "]".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 9, end: 10 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 12, end: 13 },
            },
        ]
    );
}

/// `(` の内側でも `{` ブロックに入れば終端規則が復活すること(仕様1章L-21
/// 〔正例: multiline-call〕)。引数の無名fn本体に複数文を含む形——
/// L-21の正例テストが必ず含める形。本体の各文の行末だけNewlineが生成される。
#[test]
fn anonymous_fn_body_inside_call_terminates_statements() {
    // Arrange
    let source = "call(fn() {\n    let a = 1\n    let b = 2\n    a\n})";

    // Act
    let tokens = lex(source).expect("括弧内のブロックを含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "call".to_string(),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::KwFn,
                text: "fn".to_string(),
                span: Span { start: 5, end: 7 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 7, end: 8 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 16, end: 19 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 22, end: 23 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 24, end: 25 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 25, end: 26 },
            },
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 30, end: 33 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 34, end: 35 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 36, end: 37 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 38, end: 39 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 39, end: 40 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 44, end: 45 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 45, end: 46 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 46, end: 47 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 47, end: 48 },
            },
        ]
    );
}

/// 複数行structリテラルは各フィールドの行末カンマで継続すること(仕様1章L-22
/// 〔正例: multiline-struct-literal〕)。最終フィールドのトレーリングカンマを
/// 含め、Newlineトークンは生成されない。
#[test]
fn multiline_struct_literal_with_trailing_commas_continues() {
    // Arrange
    let source = "let p = Point {\n    x: 1,\n    y: 2,\n}";

    // Act
    let tokens = lex(source).expect("複数行structリテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "p".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "Point".to_string(),
                span: Span { start: 8, end: 13 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "x".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::Colon,
                text: ":".to_string(),
                span: Span { start: 21, end: 22 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 23, end: 24 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 24, end: 25 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "y".to_string(),
                span: Span { start: 30, end: 31 },
            },
            Token {
                kind: TokenKind::Colon,
                text: ":".to_string(),
                span: Span { start: 31, end: 32 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 33, end: 34 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 34, end: 35 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 36, end: 37 },
            },
        ]
    );
}

/// 最終フィールドのトレーリングカンマ欠落では、そのフィールド行の行末に
/// Newlineが生成されること(仕様1章L-22〔負例: struct-literal-missing-trailing-
/// comma〕の字句側の挙動)。`{` の内側は終端規則が復活しているため。この形を
/// エラーにするのはパーサの担当(字句は事実をトークンで伝えるだけ)。
#[test]
fn struct_literal_missing_trailing_comma_emits_newline() {
    // Arrange
    let source = "let p = Point {\n    x: 1,\n    y: 2\n}";

    // Act
    let tokens = lex(source)
        .expect("トレーリングカンマなしのstructリテラルの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "p".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "Point".to_string(),
                span: Span { start: 8, end: 13 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "x".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::Colon,
                text: ":".to_string(),
                span: Span { start: 21, end: 22 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 23, end: 24 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 24, end: 25 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "y".to_string(),
                span: Span { start: 30, end: 31 },
            },
            Token {
                kind: TokenKind::Colon,
                text: ":".to_string(),
                span: Span { start: 31, end: 32 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 33, end: 34 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 34, end: 35 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 35, end: 36 },
            },
        ]
    );
}

/// 深度0で閉じ括弧の直後の改行は文を終端すること(仕様1章L-19/L-21。
/// 閉じ括弧はL-20の継続トークンでない)。スタックのpop漏れ(閉じた後も
/// 深度が残る実装)を殺すピン。
#[test]
fn newline_after_closing_paren_terminates() {
    // Arrange
    let source = "foo(a)\nfoo(b)";

    // Act
    let tokens = lex(source).expect("閉じ括弧直後の改行を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "foo".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "foo".to_string(),
                span: Span { start: 7, end: 10 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 12, end: 13 },
            },
        ]
    );
}

/// `}` でブロックを抜けると `(`/`[` の深度に戻り、以後の改行は再び抑制される
/// こと(仕様1章L-21の「深度は `{` で退避し、対応する `}` で復元するスタック」)。
/// 本体内の1文の行末のみNewlineが生成される。
#[test]
fn brace_close_restores_paren_depth_suppression() {
    // Arrange
    let source = "call(fn() {\n    a\n},\n1)";

    // Act
    let tokens =
        lex(source).expect("ブロック閉じ後の改行抑制を確認する字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "call".to_string(),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::KwFn,
                text: "fn".to_string(),
                span: Span { start: 5, end: 7 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 7, end: 8 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 16, end: 17 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 17, end: 18 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 18, end: 19 },
            },
            Token {
                kind: TokenKind::Comma,
                text: ",".to_string(),
                span: Span { start: 19, end: 20 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 21, end: 22 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 22, end: 23 },
            },
        ]
    );
}

/// 深度0で `[`・`{` を行末に置くと継続し、閉じ括弧 `]` の直後の改行は終端すること
/// (仕様1章L-20の開き括弧+L-19)。`[` の直後の改行はL-21の深度抑制、`{` の直後は
/// L-20の継続トークン判定(`{` はスタックトップでもある)で抑える——実装上の二重の
/// 経路を観測される挙動として固定する(impl-review 2026-08-18 観点A/B指摘)。
#[test]
fn open_brackets_at_line_end_continue_and_close_terminates() {
    // Arrange
    let source = "xs[\n0]\np{\nk: 1}";

    // Act
    let tokens = lex(source).expect("開き括弧の行末継続の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "xs".to_string(),
                span: Span { start: 0, end: 2 },
            },
            Token {
                kind: TokenKind::LBracket,
                text: "[".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Int,
                text: "0".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::RBracket,
                text: "]".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "p".to_string(),
                span: Span { start: 7, end: 8 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "k".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::Colon,
                text: ":".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 13, end: 14 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 14, end: 15 },
            },
        ]
    );
}

/// 複合代入の残り4種(`-=` `*=` `/=` `%=`)を行末に置くと継続すること(仕様1章L-20)。
/// `+=` は複合代入と区切り記号の行末テストで既に網羅済み——L-20の全継続トークンが
/// いずれかのテストで行末に置かれた状態を完成させる(impl-review 2026-08-18 指摘)。
#[test]
fn remaining_compound_assignments_at_line_end_continue() {
    // Arrange
    let source = "n -=\n1\nn *=\n2\nn /=\n3\nn %=\n4";

    // Act
    let tokens = lex(source).expect("複合代入残り4種の行末継続の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "n".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::MinusEq,
                text: "-=".to_string(),
                span: Span { start: 2, end: 4 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "n".to_string(),
                span: Span { start: 7, end: 8 },
            },
            Token {
                kind: TokenKind::StarEq,
                text: "*=".to_string(),
                span: Span { start: 9, end: 11 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 12, end: 13 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 13, end: 14 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "n".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::SlashEq,
                text: "/=".to_string(),
                span: Span { start: 16, end: 18 },
            },
            Token {
                kind: TokenKind::Int,
                text: "3".to_string(),
                span: Span { start: 19, end: 20 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "n".to_string(),
                span: Span { start: 21, end: 22 },
            },
            Token {
                kind: TokenKind::PercentEq,
                text: "%=".to_string(),
                span: Span { start: 23, end: 25 },
            },
            Token {
                kind: TokenKind::Int,
                text: "4".to_string(),
                span: Span { start: 26, end: 27 },
            },
        ]
    );
}

/// `}` で `{` ブロックを抜けた直後の改行が、外側の `(` の深度に復元されて抑制される
/// こと(仕様1章L-21「深度は `{` で退避し、対応する `}` で復元するスタック構造」)。
/// 既存の brace_close_restores_paren_depth_suppression は `}` の直後がカンマのため
/// L-20の継続トークン判定でも緑になる——本テストは `}` の直後を**直接改行**にして、
/// 復元経路(popとその対象に `}` が含まれること)だけが緑を決めるようにする。
#[test]
fn newline_after_block_close_inside_call_is_suppressed() {
    // Arrange
    let source = "call(\n  fn() {\n    a\n  }\n)";

    // Act
    let tokens =
        lex(source).expect("ブロック閉じ直後の改行を含む括弧内の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "call".to_string(),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::KwFn,
                text: "fn".to_string(),
                span: Span { start: 8, end: 10 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 13, end: 14 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 19, end: 20 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 20, end: 21 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 23, end: 24 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 25, end: 26 },
            },
        ]
    );
}

/// `{` の書き忘れ: 種類の合わない閉じ括弧は丸括弧の深度を戻さず、`}` 直後の改行が
/// 抑制されたままであること(仕様1章L-21「閉じ括弧は同種の開き括弧とだけ対応する」
/// 〔挙動検証: unmatched-close-bracket〕・
/// ADR-0047決定1)。上の newline_after_block_close_inside_call_is_suppressed から `{` を
/// 除いた形——`{` が開かれていないため `}` はスタック上の `(` と対応しない。
/// 釣り合わない括弧自体はパーサが報告するため、字句解析はエラーにせずトークン列を通す。
#[test]
fn close_brace_without_open_brace_keeps_newline_suppressed() {
    // Arrange
    let source = "call(\n  fn()\n    a\n  }\n)\n";

    // Act
    let tokens = lex(source).expect("釣り合わない `}` を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "call".to_string(),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::KwFn,
                text: "fn".to_string(),
                span: Span { start: 8, end: 10 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 17, end: 18 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 21, end: 22 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 23, end: 24 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 24, end: 25 },
            },
        ]
    );
}

/// 余分な `}`: 種類が合わない閉じ括弧の後も丸括弧の深度が続き、Newlineの区切りが
/// 増えないこと(仕様1章L-21〔挙動検証: unmatched-close-bracket〕・ADR-0047決定1)。
/// 種類を照合しない実装では `}` が `(` を
/// popして区切りが3個入り、引数リストが3つの文に割れる。修正後はNewlineが `)` の
/// 直後の1個のみ(`a`・`b` が同じ引数リストにとどまる)。
#[test]
fn extra_close_brace_between_args_keeps_newlines_suppressed() {
    // Arrange
    let source = "call(\n  a\n  }\n  b\n)\n";

    // Act
    let tokens = lex(source).expect("余分な `}` を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "call".to_string(),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 12, end: 13 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 16, end: 17 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 18, end: 19 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 19, end: 20 },
            },
        ]
    );
}

/// `}` は文を終端すること(仕様1章L-25)。`}` はL-20の継続トークン一覧に無く、
/// 深度0では L-19 の原則どおり直後の改行がNewlineトークンになる——
/// 継続トークン表にRBraceを混入させた実装(ブロック直後の行が前文に飲み込まれる)を殺すピン。
#[test]
fn brace_close_terminates_statement() {
    // Arrange
    let source = "if x {\n  a\n}\nb";

    // Act
    let tokens = lex(source).expect("ブロック直後に文が続く字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::KwIf,
                text: "if".to_string(),
                span: Span { start: 0, end: 2 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "x".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::LBrace,
                text: "{".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 9, end: 10 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 12, end: 13 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 13, end: 14 },
            },
        ]
    );
}

/// 行末の `:` は継続を起こさないこと(仕様1章L-20・ADR-0031決定2の確定一覧)。
/// L-20の一覧は閉じており `:` を含まないため、L-19の原則どおり改行が文を終端する。
#[test]
fn colon_at_line_end_does_not_continue() {
    // Arrange
    let source = "a:\nb";

    // Act
    let tokens = lex(source).expect("行末コロンを含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Colon,
                text: ":".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 3, end: 4 },
            },
        ]
    );
}

/// 行末の `!` は継続を起こさないこと(仕様1章L-20・ADR-0031決定2の確定一覧)。
/// `!` は単項演算子であり、L-20が挙げる二項演算子14種にも他のどの分類にも含まれない。
#[test]
fn bang_at_line_end_does_not_continue() {
    // Arrange
    let source = "a!\nb";

    // Act
    let tokens = lex(source).expect("行末の `!` を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Bang,
                text: "!".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 3, end: 4 },
            },
        ]
    );
}

/// 基数リテラル直後の `.` は通常のDotとして読まれること(仕様1章L-10注(a)・ADR-0044決定4)。
/// `0xFF.abs` は `Int` `Dot` `Ident` に割れる——E0106(注(b))になるのは `.` の直後が
/// 数字のとき(`0xFF.5`)だけであり、識別子開始文字のときは字句を通す。
#[test]
fn dot_after_hex_literal_splits_into_dot_and_ident() {
    // Arrange
    let source = "0xFF.abs";

    // Act
    let tokens = lex(source).expect("`0xFF.abs` の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Int,
                text: "0xFF".to_string(),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::Dot,
                text: ".".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "abs".to_string(),
                span: Span { start: 5, end: 8 },
            },
        ]
    );
}

/// `[` の内側に種類の違う閉じ `)` が現れても深度を戻さず、Newlineは `]` の直後の
/// 1個のみであること(仕様1章L-21「閉じ括弧は同種の開き括弧とだけ対応する」
/// 〔挙動検証: unmatched-close-bracket〕・ADR-0047決定1)。
/// impl-reviewの変異解析で「`(` と `[` を同一視する実装(M1)・`)` が何でも閉じる
/// 実装(M3)のどちらも既存テストを全部通す」と実証された穴を塞ぐピン——
/// 両変異では `)` が `[` をpopしてNewlineが3個(2個増)になる。`}` 方向は
/// close_brace_without_open_brace_keeps_newline_suppressed が固定済み。
#[test]
fn mismatched_close_paren_keeps_newlines_suppressed() {
    // Arrange
    let source = "xs[\n  a\n)\n  b\n]\n";

    // Act
    let tokens = lex(source).expect("種類の違う閉じ括弧を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "xs".to_string(),
                span: Span { start: 0, end: 2 },
            },
            Token {
                kind: TokenKind::LBracket,
                text: "[".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 12, end: 13 },
            },
            Token {
                kind: TokenKind::RBracket,
                text: "]".to_string(),
                span: Span { start: 14, end: 15 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 15, end: 16 },
            },
        ]
    );
}

/// `(` の内側に種類の違う閉じ `]` が現れても深度を戻さず、Newlineは `)` の直後の
/// 1個のみであること(仕様1章L-21〔挙動検証: unmatched-close-bracket〕・ADR-0047決定1)。
/// ここまでで `)`・`]`・`}` の3方向が揃う(残る2方向は
/// mismatched_close_paren_keeps_newlines_suppressed と
/// extra_close_brace_between_args_keeps_newlines_suppressed)。種類の照合を
/// 深度の数え合わせに簡略化した実装は3方向のどこかで必ず落ちる。
#[test]
fn mismatched_close_bracket_keeps_newlines_suppressed() {
    // Arrange
    let source = "f(\n  a\n]\n  b\n)\n";

    // Act
    let tokens = lex(source).expect("種類の違う閉じ括弧を含む字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "f".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::RBracket,
                text: "]".to_string(),
                span: Span { start: 7, end: 8 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 13, end: 14 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 14, end: 15 },
            },
        ]
    );
}

/// `====` は先頭3文字 `===` の時点でE0116が確定し、spanは最初の3文字を指すこと
/// (仕様1章L-26〔負例: triple-equals〕)。4文字目まで読んでから `=` を貪欲に
/// `==` 2トークンへ分割する実装・spanを0..4に広げる実装の両方を殺すピン
/// (impl-reviewの変異解析で「`=` を貪欲に食う実装が全テスト緑で生存」と実証済み)。
#[test]
fn quadruple_equals_reports_e0116_at_first_three_chars() {
    // Arrange
    let source = "====";

    // Act
    let err = lex(source).expect_err("`====` の先頭3文字は `===` としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 0, end: 3 },
        }
    );
}

/// `!====` も先頭3文字 `!==` の時点でE0116が確定し、spanは最初の3文字を指すこと
/// (仕様1章L-26〔負例: triple-not-equals〕)。`!=` を先に最長一致で確定させる実装は
/// 残りの `==` を正当なトークンとして通してしまう——`!==` の判定が `!=` より
/// 優先することの固定(triple_not_equals_reports_e0116_with_full_span の4文字版)。
#[test]
fn bang_quadruple_equals_reports_e0116_at_first_three_chars() {
    // Arrange
    let source = "!====";

    // Act
    let err = lex(source).expect_err("`!====` の先頭3文字は `!==` としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 0, end: 3 },
        }
    );
}

/// 釣り合わない開き括弧があると、以後の改行はEOFまで抑制されること
/// (仕様1章L-21・ADR-0047「逆向きの副作用」として明記された帰結)。
/// `(` が開いたままの `}` は種類が違うため深度を戻さず、`let b = 2` の行末も
/// Newlineにならない(修正前は `}` が `(` を誤って戻すことで区切りが「たまたま」
/// 復活しNewlineが2個入った)。エラー回復=複数エラー報告の実装後に効く挙動の固定。
#[test]
fn unclosed_open_paren_suppresses_newlines_until_eof() {
    // Arrange
    let source = "let a = (1\n}\nlet b = 2\n";

    // Act
    let tokens = lex(source).expect("開き括弧が余る入力も字句解析はエラーにしないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 0, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 6, end: 7 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 9, end: 10 },
            },
            Token {
                kind: TokenKind::RBrace,
                text: "}".to_string(),
                span: Span { start: 11, end: 12 },
            },
            Token {
                kind: TokenKind::KwLet,
                text: "let".to_string(),
                span: Span { start: 13, end: 16 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 17, end: 18 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 19, end: 20 },
            },
            Token {
                kind: TokenKind::Int,
                text: "2".to_string(),
                span: Span { start: 21, end: 22 },
            },
        ]
    );
}

/// 深度スタックが空のときに閉じ括弧が来ても、何もせず(エラーにせず)通常どおりの
/// トークン列を返すこと(仕様1章L-21・ADR-0047案B: 括弧の不均衡の報告はパーサの
/// 担当であり、字句解析器はスタックを覗いて空ならpopを試みない)。深度0の改行は
/// 通常どおりNewlineになる。
#[test]
fn close_paren_on_empty_stack_does_not_error() {
    // Arrange
    let source = "a)\nb";

    // Act
    let tokens = lex(source).expect("スタック空での閉じ括弧は字句エラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 3, end: 4 },
            },
        ]
    );
}

/// 補間の内側の `!==` は、補間が閉じないこと(E0109)より先にE0116として報告される
/// こと(仕様1章L-18注2「終端系以外の内側エラーはE0109より優先」+L-26
/// 〔負例: triple-not-equals〕)。入力は閉じ `"` も `}` も持たないが、より具体的な
/// 原因(誤綴り)を指すほうが修正しやすいため優先する。spanは内側の `!==` の3バイト。
#[test]
fn triple_not_equals_inside_interpolation_reports_e0116() {
    // Arrange
    let source = "\"${a !== b";

    // Act
    let err = lex(source).expect_err("補間内の `!==` はE0116としてエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 5, end: 8 },
        }
    );
}

/// 不一致の閉じ括弧の照合は**スタックの先頭だけ**を見ること
/// (仕様1章L-21〔挙動検証: unmatched-close-bracket〕・ADR-0047決定1)。
/// 深さ2(`(` の内側の `[`)での `)` は、下に同種の `(` があっても戻りに探さない
/// ——末尾から探して同種が見つかればそこまで戻す実装(パーサでよくある回復
/// ヒューリスティック)ではNewlineが4個(3個増)になる。impl-reviewの解消検証で
/// 「深追い実装が全テスト緑で生存する」と実証された穴を塞ぐピン。
#[test]
fn mismatched_close_paren_at_nested_depth_keeps_newlines_suppressed() {
    // Arrange
    let source = "f([a\n)\n b\n]\n)\n";

    // Act
    let tokens = lex(source).expect("深さ2での種類の違う閉じ括弧もエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "f".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::LParen,
                text: "(".to_string(),
                span: Span { start: 1, end: 2 },
            },
            Token {
                kind: TokenKind::LBracket,
                text: "[".to_string(),
                span: Span { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 3, end: 4 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 8, end: 9 },
            },
            Token {
                kind: TokenKind::RBracket,
                text: "]".to_string(),
                span: Span { start: 10, end: 11 },
            },
            Token {
                kind: TokenKind::RParen,
                text: ")".to_string(),
                span: Span { start: 12, end: 13 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 13, end: 14 },
            },
        ]
    );
}

/// `<=` の直後の `=` はE0116にしないこと(仕様1章L-2最長一致・L-26)。
/// `<=` は1.9に実在する演算子なので2文字で引き、残りの `=` は独立したEqトークンに
/// なる。`===`/`!==` の3文字目先読みガードは種別を `==`/`!=` に限定したものであり、
/// 「2文字演算子全般の直後が `=` ならE0116」は誤り——impl-reviewの解消検証で
/// 「ガードの種別条件を外しても全テスト緑で生存する」と実証された穴を塞ぐピン。
#[test]
fn less_equal_followed_by_eq_splits_into_two_tokens() {
    // Arrange
    let source = "a <== b";

    // Act
    let tokens = lex(source).expect("`<==` はE0116ではなく2トークンに分割されること");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Ident,
                text: "a".to_string(),
                span: Span { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::LtEq,
                text: "<=".to_string(),
                span: Span { start: 2, end: 4 },
            },
            Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Ident,
                text: "b".to_string(),
                span: Span { start: 6, end: 7 },
            },
        ]
    );
}
