//! 字句解析器(lexer)の統合テスト。
//! docs/spec/01-lexical.md を正とし、TDDサイクルごとに1振る舞いずつ追加する。

use mesh::lexer::{ErrorCode, LexError, Span, Token, TokenKind, lex};

/// 空のソース文字列を字句解析すると、空のトークン列が返ること。
#[test]
fn empty_source_produces_no_tokens() {
    let tokens = lex("").expect("空入力の字句解析はエラーにならないこと");
    assert_eq!(tokens, Vec::<Token>::new());
}

/// 10進整数リテラルが1個のIntトークンとして切り出されること。
/// 空白で区切られた複数リテラルも、それぞれ独立したIntトークンになり、
/// 空白自体はトークンにならないこと(仕様1章1.2)。
#[test]
fn decimal_integer_literal_produces_single_int_token() {
    let tokens = lex("42").expect("整数リテラルの字句解析はエラーにならないこと");
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "42".to_string(),
            span: Span { start: 0, end: 2 },
        }]
    );

    let tokens = lex("1 22").expect("空白区切りの整数リテラルの字句解析はエラーにならないこと");
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
                span: Span { start: 2, end: 4 },
            },
        ]
    );
}

/// 識別子と予約語が切り出され、区別されること(仕様1章1.4・1.5)。
/// `let x = 1` は KwLet / Ident / Eq / Int の4トークンになること。
/// 予約語は最長一致で判定し、`lettuce` のような「letで始まる識別子」を
/// 予約語に誤認しないこと。先頭 `_` も正規の識別子として扱うこと(仕様1章1.4)。
#[test]
fn identifiers_and_keyword_let_are_distinguished() {
    let tokens = lex("let x = 1").expect("`let x = 1` の字句解析はエラーにならないこと");
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
        ]
    );

    let tokens = lex("lettuce").expect("`lettuce` の字句解析はエラーにならないこと");
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "lettuce".to_string(),
            span: Span { start: 0, end: 7 },
        }]
    );

    let tokens = lex("_tmp").expect("`_tmp` の字句解析はエラーにならないこと");
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "_tmp".to_string(),
            span: Span { start: 0, end: 4 },
        }]
    );
}

/// 各トークンがソース中のバイトオフセット位置(span)を持つこと。
/// 位置つきエラー報告の基盤であり、仕様1章の各E01xx規則が「位置つき」報告を要求する。
#[test]
fn tokens_carry_byte_offset_spans() {
    let source = "let answer = 42";
    let tokens = lex(source).expect("`let answer = 42` の字句解析はエラーにならないこと");
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

    // 不変条件: 現段階の全トークン種で text はソースのspan位置の切り出しと一致する。
    // (将来、文字列エスケープ等でtextが正規化されるトークン種はこの限りではない)
    for t in &tokens {
        assert_eq!(
            t.text,
            &source[t.span.start..t.span.end],
            "textとsource[span]が一致すること"
        );
    }
}

/// エラーのspanはバイトオフセットであること(文字数ではない)。
/// マルチバイト文字 `±`(U+00B1、UTF-8で2バイト)のE0116は2バイト幅のspanを持つ。
/// 仕様1章の位置つき報告(E01xx)がバイト単位で一貫していることの固定。
#[test]
fn multibyte_character_error_span_counts_bytes() {
    let err = lex("1 ± 2").expect_err("`±` は字句規則に該当しないためエラーになること");
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
    let err = lex("let @ = 1").expect_err("`@` は字句規則に該当しないためエラーになること");
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 4, end: 5 },
        }
    );
}

/// バッククォートとシングルクォートがE0116として位置つきで報告されること
/// (仕様1章L-26〔負例: backtick-string、single-quote-string〕)。
/// 「近い正解の案内」(`"` を使う)はエラーメッセージ実装のサイクルで検証する。
/// 注: 同じL-26の負例 triple-equals(`===`)は演算子未実装の現状では
/// `Eq`×3に分かれてエラーにならないため、演算子実装のサイクルで追加する。
#[test]
fn backtick_and_single_quote_report_e0116_with_span() {
    let err = lex("`abc`").expect_err("バッククォート文字列はエラーになること");
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 0, end: 1 },
        }
    );

    let err = lex("'abc'").expect_err("シングルクォート文字列はエラーになること");
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 0, end: 1 },
        }
    );
}

/// `_` で始まる字句は識別子として解釈されること(仕様1章1.4: `_1`・`_tmp` は正規の識別子)。
/// L-9の「先頭の `_` はエラー」は数値リテラル内部の規則で、10進では `_` 始まりに
/// 数値スキャンが到達しないため衝突しない(仕様1章L-9の注記)。
/// `_` 単体も字句段階ではIdent(「値を捨てる」特別扱いは仕様4章の担当)。
#[test]
fn leading_underscore_lexes_as_identifier() {
    let tokens = lex("_1").expect("`_1` の字句解析はエラーにならないこと");
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "_1".to_string(),
            span: Span { start: 0, end: 2 },
        }]
    );

    let tokens = lex("_").expect("`_` 単体の字句解析はエラーにならないこと");
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Ident,
            text: "_".to_string(),
            span: Span { start: 0, end: 1 },
        }]
    );
}

/// 桁区切り `_` を含む整数リテラルが1個のIntトークンとして切り出されること(仕様1章L-9)。
/// textはソースの生の字面のまま保持し、`_` を除去しないこと(値への正規化はコード生成側の責務)。
#[test]
fn integer_with_digit_separator_is_single_token() {
    let tokens = lex("1_000").expect("桁区切り付き整数リテラルの字句解析はエラーにならないこと");
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "1_000".to_string(),
            span: Span { start: 0, end: 5 },
        }]
    );

    // 区切りが複数ある正例(仕様1章の正例列 `1_000_000`)。検査ループの2周目以降を固定する。
    let tokens = lex("1_000_000").expect("`1_000_000` の字句解析はエラーにならないこと");
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Int,
            text: "1_000_000".to_string(),
            span: Span { start: 0, end: 9 },
        }]
    );
}

/// 桁区切り `_` が不正な位置(数字と数字の間以外)にあるとエラーE0105が
/// 位置つきで報告されること(仕様1章L-9・負例ID underscore-edge)。
/// 数値リテラル中を左から走査し、「直前と直後の両方が数字」でない最初の `_`
/// を報告する。spanはその `_` 1バイトのみ。
/// 注: `0x_FF` も仕様上のunderscore-edge負例だが、16進リテラル未実装のため
/// 今回は対象外(0x_FFのケースは16進実装のサイクルで追加する)。
#[test]
fn misplaced_digit_separator_reports_e0105_with_span() {
    let err = lex("1__0").expect_err("`_`が連続する整数リテラルはエラーになること");
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 1, end: 2 },
        }
    );

    let err = lex("1_").expect_err("末尾が`_`の整数リテラルはエラーになること");
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 1, end: 2 },
        }
    );

    // 優先順位の固定: 数値スキャンは `_` を貪欲に取り込むため、`1_x` は
    // L-12(E0113、将来実装)ではなくE0105と報告する(仕様1章L-9の注記)。
    let err = lex("1_x").expect_err("`1_x` は桁区切りエラーになること");
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0105,
            span: Span { start: 1, end: 2 },
        }
    );
}

/// 回帰の網: ここまでのサイクルで実装した全トークン種(KwLet/Ident/Eq/Int)と
/// 桁区切りを1入力に含むスナップショット。
/// TDDサイクルの検証は上の明示的assertが担い、これは出力全体の固定のみを担う。
#[test]
fn snapshot_token_stream() {
    insta::assert_debug_snapshot!(mesh::lexer::lex("let answer = 1_000"));
}
