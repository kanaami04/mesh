//! 字句解析器(lexer)の統合テスト。
//! docs/spec/01-lexical.md を正とし、TDDサイクルごとに1振る舞いずつ追加する。
//! 書き方はAAAパターン+1テスト1assert(規約: .claude/skills/test-writing/SKILL.md)。

use mesh::lexer::{ErrorCode, LexError, Span, Token, TokenKind, lex};

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
/// 注: 同じL-26の負例 triple-equals(`===`)は演算子未実装の現状では `Eq`×3 に分かれて
/// エラーにならないため、演算子実装のサイクルで追加する。
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
/// 注: 同じunderscore-edgeの `0x_FF`・`1e_6` は16進・指数部実装のサイクルで追加する。
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

/// `_` の直後に英字が続く形はL-12(E0113、将来実装)ではなくE0105になること
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

/// 改行だけの入力が空でなく1個のNewlineトークンになること(仕様1章L-19)。
/// 空入力(トークンゼロ)との境界。オフセット0のNewlineもここで固定する。
#[test]
fn newline_only_source_produces_single_newline_token() {
    // Arrange
    let source = "\n";

    // Act
    let tokens = lex(source).expect("改行のみの入力の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![Token {
            kind: TokenKind::Newline,
            text: "\n".to_string(),
            span: Span { start: 0, end: 1 },
        }]
    );
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

/// 連続した改行がそれぞれ独立したNewlineトークンになること(仕様1章L-19)。
/// 空行(空文)のスキップはパーサの担当で、字句は事実をそのまま伝える。
#[test]
fn consecutive_newlines_each_produce_token() {
    // Arrange
    let source = "1\n\n2";

    // Act
    let tokens = lex(source).expect("連続改行を含む字句解析はエラーにならないこと");

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

/// `//` から行末まではコメントとして読み飛ばされ、トークンを生成しないこと
/// (仕様1章L-4)。改行自体はコメントに含まれず、独立したNewlineトークンとして残る
/// (L-4「行末トークンの判定はコメントを除去した後の行末に対して行う」の字句側基盤)。
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

/// 回帰の網: ここまでのサイクルで実装した全トークン種(KwLet/Ident/Eq/Int/Newline)と
/// 桁区切り・改行2形(LF/CRLF)を1入力に含むスナップショット。
/// TDDサイクルの検証は上の明示的assertが担い、これは出力全体の固定のみを担う
/// (スナップショットテストはAAAマーカーの対象外)。
#[test]
fn snapshot_token_stream() {
    insta::assert_debug_snapshot!(mesh::lexer::lex("let answer = 1_000\r\nanswer\n"));
}
