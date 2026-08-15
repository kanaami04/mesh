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
    /// 改行(仕様1章L-19の文終端の基盤)。
    Newline,
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
    /// 桁区切り `_` の位置違反(仕様1章L-9)。
    E0105,
    /// どの字句規則にも該当しない文字(仕様1章L-26キャッチオール)。
    /// 注意: 固有の規則を持つが未実装の文字(`;`=E0110、非ASCII識別子=E0103、
    /// `/*`=E0102、`\r`=CRLFは仕様1章が未定義)も現状は暫定でこのコードになる。
    /// 各規則の実装サイクルで正しいコードに置き換える。
    /// また未実装の正当なトークン(演算子 `+` `(` 等、文字列開始 `"`)も
    /// 現状はこのエラーで落ちる(実装が進めばエラーではなくなる別カテゴリ)。
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
            let (text, end) = scan_while(source, &mut chars, start, c, |d| {
                d.is_ascii_digit() || d == '_'
            });
            check_digit_separators(text, start)?;
            tokens.push(token(TokenKind::Int, text, Span { start, end }));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let (text, end) = scan_while(source, &mut chars, start, c, |d| {
                d.is_ascii_alphanumeric() || d == '_'
            });
            let kind = if text == "let" {
                TokenKind::KwLet
            } else {
                TokenKind::Ident
            };
            tokens.push(token(kind, text, Span { start, end }));
        } else if c == '=' {
            chars.next();
            let end = start + '='.len_utf8();
            tokens.push(token(TokenKind::Eq, "=", Span { start, end }));
        } else if c == '\n' {
            chars.next();
            let end = start + '\n'.len_utf8();
            tokens.push(token(TokenKind::Newline, "\n", Span { start, end }));
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

/// 数値リテラルの桁区切り `_` が「数字と数字の間」にあるか検査する(仕様1章L-9)。
/// 左から走査し、直前と直後の両方がASCII数字でない最初の `_` をE0105として報告する。
/// spanはその `_` 1バイトぶん(`_` はASCIIなので1バイト固定)。
fn check_digit_separators(text: &str, start: usize) -> Result<(), LexError> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'_' {
            continue;
        }
        let prev_is_digit = i > 0 && bytes[i - 1].is_ascii_digit();
        let next_is_digit = bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
        if !(prev_is_digit && next_is_digit) {
            return Err(LexError {
                code: ErrorCode::E0105,
                span: Span {
                    start: start + i,
                    end: start + i + 1,
                },
            });
        }
    }
    Ok(())
}

/// トークンを構築する。spanはソース上の実位置を呼び出し側が渡す。
/// textの長さからspanを逆算してはいけない: 将来textが正規化されて
/// ソースの字面と長さが変わったとき(文字列エスケープ等)、spanが静かに壊れるため。
fn token(kind: TokenKind, text: &str, span: Span) -> Token {
    Token {
        kind,
        text: text.to_string(),
        span,
    }
}

/// 先頭文字 `first`(位置 `start`)から、`pred` を満たす限り読み進めて
/// 最長一致の部分文字列と終端バイトオフセット(半開区間のend)を返す(仕様1章L-2の共通部品)。
fn scan_while<'s>(
    source: &'s str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    start: usize,
    first: char,
    pred: impl Fn(char) -> bool,
) -> (&'s str, usize) {
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
    (&source[start..end], end)
}
