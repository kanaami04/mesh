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

/// 単独の `/`(直後が `/` でも `*` でもない)は暫定E0116のままであること
/// (除算演算子は演算子サイクルで実装。仕様1章L-26)。
/// span退行(範囲外化)とコード取り違えの両方を変異テストで検知できることを確認済み。
#[test]
fn lone_slash_reports_e0116_with_span() {
    // Arrange
    let source = "1 / 2";

    // Act
    let err = lex(source).expect_err("単独の`/`は演算子未実装の現状ではエラーになること");

    // Assert
    assert_eq!(
        err,
        LexError {
            code: ErrorCode::E0116,
            span: Span { start: 2, end: 3 },
        }
    );
}

/// コメントのみの行は裸のNewlineトークンを並べること(仕様1章L-4)。
/// 将来の終端挿入(ADR-0010)は「直前がNewlineなら挿入しない」を備える必要がある——
/// その前提となる字句側の出力形をここで固定する。
#[test]
fn comment_only_lines_produce_bare_newline_tokens() {
    // Arrange
    let source = "// a\n// b\n1";

    // Act
    let tokens = lex(source).expect("コメントのみの行の字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 4, end: 5 },
            },
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 9, end: 10 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 10, end: 11 },
            },
        ]
    );
}

/// `///`(ドキュメンテーションコメント予約)はv1では `//` と同じ扱いであること
/// (仕様1章L-30〔正例: doc-comment-v1〕)。
/// Redを経ていない後追いの回帰テスト(`///` は `//` で始まるため自動的にコメントになる)。
#[test]
fn doc_comment_is_treated_as_line_comment_in_v1() {
    // Arrange
    let source = "/// d\n1";

    // Act
    let tokens = lex(source).expect("`///` コメントの字句解析はエラーにならないこと");

    // Assert
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Newline,
                text: "\n".to_string(),
                span: Span { start: 5, end: 6 },
            },
            Token {
                kind: TokenKind::Int,
                text: "1".to_string(),
                span: Span { start: 6, end: 7 },
            },
        ]
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

/// 回帰の網: ここまでのサイクルで実装した全トークン種(KwLet/Ident/Eq/Int/Str/Newline)と
/// 桁区切り(独立したIntトークンとして)・改行2形(LF/CRLF)・日本語入り行コメント・
/// エスケープ入り文字列を1入力に含むスナップショット。
/// TDDサイクルの検証は上の明示的assertが担い、これは出力全体の固定のみを担う
/// (スナップショットテストはAAAマーカーの対象外)。
#[test]
fn snapshot_token_stream() {
    insta::assert_debug_snapshot!(mesh::lexer::lex(
        "let n = 1_000 // 合計\r\nlet msg = \"答え\\n\"\nn"
    ));
}
