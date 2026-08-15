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

/// 1個のトークン。種類と、ソース上の元の文字列を持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

/// 字句解析エラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError;

/// ソース文字列を字句解析してトークン列を返す。
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_ascii_digit() {
            let text = scan_while(source, &mut chars, start, c, |d| d.is_ascii_digit());
            tokens.push(Token {
                kind: TokenKind::Int,
                text: text.to_string(),
            });
        } else if c.is_ascii_alphabetic() || c == '_' {
            let text = scan_while(source, &mut chars, start, c, |d| {
                d.is_ascii_alphanumeric() || d == '_'
            });
            let kind = if text == "let" {
                TokenKind::KwLet
            } else {
                TokenKind::Ident
            };
            tokens.push(Token {
                kind,
                text: text.to_string(),
            });
        } else if c == '=' {
            chars.next();
            tokens.push(Token {
                kind: TokenKind::Eq,
                text: "=".to_string(),
            });
        } else if c == ' ' || c == '\t' {
            chars.next();
        } else {
            return Err(LexError);
        }
    }
    Ok(tokens)
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
