//! 字句解析器(lexer)の統合テスト。
//! docs/spec/01-lexical.md を正とし、TDDサイクルごとに1振る舞いずつ追加する。

use mesh::lexer::{Span, Token, TokenKind, lex};

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
/// 予約語に誤認しないこと。先頭 `_` も正規の識別子として扱うこと(仕様1章L-6)。
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
    let tokens =
        lex("let answer = 42").expect("`let answer = 42` の字句解析はエラーにならないこと");
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

/// 回帰の網: ここまでのサイクルで実装した字句全体のスナップショット。
/// TDDサイクルの検証は上の明示的assertが担い、これは出力全体の固定のみを担う。
#[test]
fn snapshot_token_stream() {
    insta::assert_debug_snapshot!(mesh::lexer::lex("let answer = 42"));
}
