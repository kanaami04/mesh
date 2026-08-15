//! 字句解析器。Meshソース文字列をトークン列に変換する。
//! 仕様は docs/spec/01-lexical.md が正。

/// トークンの種類。TDDサイクルで振る舞いを追加するたびにバリアントを増やす。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// 10進整数リテラル(例: `42`)。
    Int,
    /// 予約語 `let`(仕様1章1.5)。
    KwLet,
    /// 識別子(仕様1章1.4)。
    Ident,
    /// 代入記号 `=`。
    Eq,
}

/// ソース中の位置(バイトオフセットの半開区間)。位置つきエラー報告の基盤(仕様1章の各E01xx規則)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// バイトオフセット(含む)。
    pub start: usize,
    /// バイトオフセット(含まない)。
    pub end: usize,
}

/// 1個のトークン。種類と、ソース上の元の文字列・位置を持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

/// 字句エラーのコード(仕様1章のE01xx)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// どの字句規則にも該当しない文字(仕様1章L-26キャッチオール)。
    E0116,
}

/// 字句解析エラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub code: ErrorCode,
    pub span: Span,
}

/// ソース文字列を字句解析してトークン列を返す。
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_ascii_digit() {
            let text = scan_while(source, &mut chars, start, c, |d| {
                d.is_ascii_digit() || d == '_'
            });
            tokens.push(token(TokenKind::Int, text, start));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let text = scan_while(source, &mut chars, start, c, |d| {
                d.is_ascii_alphanumeric() || d == '_'
            });
            let kind = if text == "let" {
                TokenKind::KwLet
            } else {
                TokenKind::Ident
            };
            tokens.push(token(kind, text, start));
        } else if c == '=' {
            chars.next();
            tokens.push(token(TokenKind::Eq, "=", start));
        } else if c == ' ' || c == '\t' {
            chars.next();
        } else {
            return Err(LexError {
                code: ErrorCode::E0116,
                span: Span {
                    start,
                    end: start + c.len_utf8(),
                },
            });
        }
    }
    Ok(tokens)
}

/// `start` 位置から `text` の長さぶんを占めるトークンを構築する(spanはバイト長から算出)。
fn token(kind: TokenKind, text: &str, start: usize) -> Token {
    Token {
        kind,
        text: text.to_string(),
        span: Span {
            start,
            end: start + text.len(),
        },
    }
}

/// 先頭文字 `first`(位置 `start`)から、`pred` を満たす限り読み進めて
/// 最長一致の部分文字列を返す(仕様1章L-2の共通部品)。
fn scan_while<'s>(
    source: &'s str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    start: usize,
    first: char,
    pred: impl Fn(char) -> bool,
) -> &'s str {
    let mut end = start + first.len_utf8();
    chars.next();
    while let Some(&(i, d)) = chars.peek() {
        if pred(d) {
            end = i + d.len_utf8();
            chars.next();
        } else {
            break;
        }
    }
    &source[start..end]
}
