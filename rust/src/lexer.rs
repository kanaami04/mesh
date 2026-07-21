// TS版(src/lexer.ts)からの移植。
//
// 一番大きな設計判断: TSはクロージャ(advance/pos/last)でi・line・colという外側の変数を
// 直接書き換えていたが、Rustの借用チェッカはこの「複数のクロージャが同じ可変変数を
// 好き勝手に捕まえる」形と相性が悪い。Rustでは代わりに、状態をひとまとめにした構造体
// (Lexer)を作り、その上にメソッド(&mut self)を生やすのが定石。慣れると
// 「クロージャの代わりに構造体+implを使う」というパターンとして色々な場面で効いてくる。
//
// もう1つ: TSは`throw`で構文エラーを投げていたが、Rustにはそれに相当する仕組みが無い
// (パニックはある。ここではあくまで「予期されるエラー」)。代わりに`Result<T, E>`を返し、
// 呼び出し側は`?`で伝播させる。これはMesh自身の`T | error` + `?`が着想を得た元ネタそのもの
// なので、書いていて答え合わせをしている感覚になるはず。

use crate::token::{keyword_from_str, CommentInfo, LexError, Pos, StringPart, Token, TokenType};

#[derive(Debug)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub comments: Vec<CommentInfo>,
}

// Goと同じ「セミコロン自動挿入」ルール: 行末のトークンがこの集合に含まれるとき、
// 改行を`;`として扱う。これによりMeshのコードは行末セミコロン不要になる。
fn asi_after(t: TokenType) -> bool {
    use TokenType::*;
    matches!(
        t,
        Ident | Int | Float | Str | NoneKw | True | False | Return | Break | Continue
            | RParen | RBracket | RBrace | PlusPlus | MinusMinus | Question // 後置の`?`はASI対象(前置の`!`は対象外)
    )
}

// 長い記号から先に照合する(":="を":" "="に分解しないため)。順序はTS版のOPERATORS配列と揃えてある
const OPERATORS: &[(&str, TokenType)] = &[
    (":=", TokenType::ColonEq),
    ("==", TokenType::EqEq),
    ("!=", TokenType::NotEq),
    ("<=", TokenType::Le),
    (">=", TokenType::Ge),
    ("&&", TokenType::AndAnd),
    ("||", TokenType::OrOr),
    ("<-", TokenType::Arrow),
    ("++", TokenType::PlusPlus),
    ("--", TokenType::MinusMinus),
    ("=>", TokenType::FatArrow),
    ("+=", TokenType::PlusEq),
    ("-=", TokenType::MinusEq),
    ("*=", TokenType::StarEq),
    ("/=", TokenType::SlashEq),
    ("%=", TokenType::PercentEq),
    ("=", TokenType::Eq),
    ("<", TokenType::Lt),
    (">", TokenType::Gt),
    ("+", TokenType::Plus),
    ("-", TokenType::Minus),
    ("*", TokenType::Star),
    ("/", TokenType::Slash),
    ("%", TokenType::Percent),
    ("!", TokenType::Bang),
    ("?", TokenType::Question),
    ("|", TokenType::Pipe),
    (",", TokenType::Comma),
    (":", TokenType::Colon),
    (";", TokenType::Semi),
    ("(", TokenType::LParen),
    (")", TokenType::RParen),
    ("{", TokenType::LBrace),
    ("}", TokenType::RBrace),
    ("[", TokenType::LBracket),
    ("]", TokenType::RBracket),
    (".", TokenType::Dot),
];

fn escape_char(c: char) -> Option<char> {
    match c {
        'n' => Some('\n'),
        't' => Some('\t'),
        'r' => Some('\r'),
        '"' => Some('"'),
        '\\' => Some('\\'),
        '$' => Some('$'), // リテラルの$が欲しいとき: "\$"(補間させないための唯一のエスケープ)
        _ => None,
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
    tokens: Vec<Token>,
    comments: Vec<CommentInfo>,
}

impl Lexer {
    fn pos(&self) -> Pos {
        Pos { line: self.line, col: self.col }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.i + offset).copied()
    }

    fn starts_with(&self, op: &str) -> bool {
        let op_chars: Vec<char> = op.chars().collect();
        if self.i + op_chars.len() > self.chars.len() {
            return false;
        }
        self.chars[self.i..self.i + op_chars.len()] == op_chars[..]
    }

    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            if self.i >= self.chars.len() {
                break;
            }
            if self.chars[self.i] == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.i += 1;
        }
    }

    // 文字列リテラル(補間 ${式} 対応)。最初の`"`はすでに読んでいない前提(呼び出し元でchar自体は
    // まだ消費前 — TS版と同じく、この関数の先頭でadvance(1)して開き"を読み飛ばす)
    fn lex_string(&mut self) -> Result<Token, LexError> {
        let start = self.pos();
        self.advance(1); // 開きの"
        let mut parts: Vec<StringPart> = Vec::new();
        let mut text = String::new();

        while self.i < self.chars.len() && self.chars[self.i] != '"' {
            if self.chars[self.i] == '\n' {
                return Err(LexError {
                    message: "string literal not terminated".into(),
                    pos: start,
                    code: "unterminated-string",
                });
            }
            if self.chars[self.i] == '\\' {
                let next = self.peek(1);
                let esc = next.and_then(escape_char);
                match esc {
                    None => {
                        return Err(LexError {
                            message: format!("unknown escape \\{}", next.unwrap_or(' ')),
                            pos: self.pos(),
                            code: "unknown-escape",
                        });
                    }
                    Some(c) => {
                        text.push(c);
                        self.advance(2);
                        continue;
                    }
                }
            }
            // ${式} — 対応する}までを式のソース断片として切り出す
            if self.chars[self.i] == '$' && self.peek(1) == Some('{') {
                self.advance(2);
                let expr_pos = self.pos();
                let mut expr_src = String::new();
                let mut depth = 1i32;
                while self.i < self.chars.len() && self.chars[self.i] != '\n' {
                    let c = self.chars[self.i];
                    // 補間式の中の入れ子文字列("x${m["k"]}"など)。中の{}は深さに数えない
                    if c == '"' {
                        expr_src.push(c);
                        self.advance(1);
                        while self.i < self.chars.len() && self.chars[self.i] != '"' && self.chars[self.i] != '\n' {
                            if self.chars[self.i] == '\\' {
                                expr_src.push(self.chars[self.i]);
                                if let Some(next) = self.peek(1) {
                                    expr_src.push(next);
                                }
                                self.advance(2);
                            } else {
                                expr_src.push(self.chars[self.i]);
                                self.advance(1);
                            }
                        }
                        if self.i >= self.chars.len() || self.chars[self.i] == '\n' {
                            return Err(LexError {
                                message: "string literal not terminated".into(),
                                pos: expr_pos,
                                code: "unterminated-string",
                            });
                        }
                        expr_src.push('"');
                        self.advance(1);
                        continue;
                    }
                    if c == '{' {
                        depth += 1;
                    }
                    if c == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    expr_src.push(c);
                    self.advance(1);
                }
                if depth > 0 {
                    return Err(LexError {
                        message: "interpolation not terminated — missing '}'".into(),
                        pos: expr_pos,
                        code: "unterminated-interpolation",
                    });
                }
                self.advance(1); // 閉じの}
                if expr_src.trim().is_empty() {
                    return Err(LexError {
                        message: "empty interpolation '${}'".into(),
                        pos: expr_pos,
                        code: "empty-interpolation",
                    });
                }
                if !text.is_empty() {
                    parts.push(StringPart::Text { text: std::mem::take(&mut text) });
                }
                parts.push(StringPart::Expr { source: expr_src, pos: expr_pos });
                continue;
            }
            text.push(self.chars[self.i]);
            self.advance(1);
        }
        if self.i >= self.chars.len() {
            return Err(LexError {
                message: "string literal not terminated".into(),
                pos: start,
                code: "unterminated-string",
            });
        }
        self.advance(1); // 閉じの"
        if !parts.is_empty() {
            if !text.is_empty() {
                parts.push(StringPart::Text { text });
            }
            Ok(Token { kind: TokenType::Str, value: String::new(), pos: start, parts: Some(parts) })
        } else {
            Ok(Token { kind: TokenType::Str, value: text, pos: start, parts: None })
        }
    }

    fn run(mut self) -> Result<LexOutput, LexError> {
        while self.i < self.chars.len() {
            let ch = self.chars[self.i];

            // 改行: セミコロン自動挿入の判定
            if ch == '\n' {
                if let Some(prev) = self.tokens.last()
                    && asi_after(prev.kind) {
                        self.tokens.push(Token { kind: TokenType::Semi, value: ";".into(), pos: self.pos(), parts: None });
                    }
                self.advance(1);
                continue;
            }

            // 空白
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance(1);
                continue;
            }

            // 行コメント(改行は消費しない = セミコロン挿入は生きる)
            if ch == '/' && self.peek(1) == Some('/') {
                let start = self.pos();
                let mut text = String::new();
                while self.i < self.chars.len() && self.chars[self.i] != '\n' {
                    text.push(self.chars[self.i]);
                    self.advance(1);
                }
                self.comments.push(CommentInfo { text, pos: start });
                continue;
            }

            // 文字列リテラル
            if ch == '"' {
                let token = self.lex_string()?;
                self.tokens.push(token);
                continue;
            }

            // 数値リテラル(int / float)
            if ch.is_ascii_digit() {
                let start = self.pos();
                let mut value = String::new();
                while self.i < self.chars.len() && self.chars[self.i].is_ascii_digit() {
                    value.push(self.chars[self.i]);
                    self.advance(1);
                }
                let mut is_float = false;
                if self.chars.get(self.i) == Some(&'.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
                    is_float = true;
                    value.push('.');
                    self.advance(1);
                    while self.i < self.chars.len() && self.chars[self.i].is_ascii_digit() {
                        value.push(self.chars[self.i]);
                        self.advance(1);
                    }
                }
                let kind = if is_float { TokenType::Float } else { TokenType::Int };
                self.tokens.push(Token { kind, value, pos: start, parts: None });
                continue;
            }

            // 識別子・キーワード
            if is_ident_start(ch) {
                let start = self.pos();
                let mut value = String::new();
                while self.i < self.chars.len() && is_ident_continue(self.chars[self.i]) {
                    value.push(self.chars[self.i]);
                    self.advance(1);
                }
                let kind = keyword_from_str(&value).unwrap_or(TokenType::Ident);
                self.tokens.push(Token { kind, value, pos: start, parts: None });
                continue;
            }

            // 記号・演算子
            if let Some(&(op, kind)) = OPERATORS.iter().find(|(op, _)| self.starts_with(op)) {
                self.tokens.push(Token { kind, value: op.into(), pos: self.pos(), parts: None });
                self.advance(op.chars().count());
                continue;
            }

            return Err(LexError {
                message: format!("unexpected character '{ch}'"),
                pos: self.pos(),
                code: "unexpected-character",
            });
        }

        // 最終行の文もセミコロンで閉じる
        if let Some(prev) = self.tokens.last()
            && asi_after(prev.kind) {
                self.tokens.push(Token { kind: TokenType::Semi, value: ";".into(), pos: self.pos(), parts: None });
            }
        self.tokens.push(Token { kind: TokenType::Eof, value: String::new(), pos: self.pos(), parts: None });
        Ok(LexOutput { tokens: self.tokens, comments: self.comments })
    }
}

// start_pos: 文字列補間の式断片を再字句解析するとき、元ソース上の位置から数え始めるために使う
pub fn lex(source: &str, start_pos: Option<Pos>) -> Result<LexOutput, LexError> {
    let start = start_pos.unwrap_or(Pos { line: 1, col: 1 });
    let lexer = Lexer {
        chars: source.chars().collect(),
        i: 0,
        line: start.line,
        col: start.col,
        tokens: Vec::new(),
        comments: Vec::new(),
    };
    lexer.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(src: &str) -> Vec<TokenType> {
        lex(src, None).unwrap().tokens.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn 基本的なトークン分解() {
        use TokenType::*;
        assert_eq!(types("x := 10"), vec![Ident, ColonEq, Int, Semi, Eof]);
    }

    #[test]
    fn キーワードと識別子を区別する() {
        use TokenType::*;
        assert_eq!(types("fn foo"), vec![Fn, Ident, Semi, Eof]);
    }

    #[test]
    fn セミコロン自動挿入_式の行末にだけ入る() {
        use TokenType::*;
        let src = "fn main() {\n\tprint(\"hi\")\n}\n";
        assert_eq!(
            types(src),
            vec![Fn, Ident, LParen, RParen, LBrace, Ident, LParen, Str, RParen, Semi, RBrace, Semi, Eof]
        );
    }

    #[test]
    fn セミコロン自動挿入_中括弧の後には入らない() {
        use TokenType::*;
        assert_eq!(types("if x {\n}"), vec![If, Ident, LBrace, RBrace, Semi, Eof]);
    }

    #[test]
    fn 矢印は隣接時のみアロー_離れていれば比較演算() {
        use TokenType::*;
        assert_eq!(types("ch <- v"), vec![Ident, Arrow, Ident, Semi, Eof]);
        assert_eq!(types("a < -1"), vec![Ident, Lt, Minus, Int, Semi, Eof]);
    }

    #[test]
    fn floatとintを区別する() {
        use TokenType::*;
        assert_eq!(types("1.5 2"), vec![Float, Int, Semi, Eof]);
    }

    #[test]
    fn 文字列のエスケープ() {
        let out = lex(r#""a\nb""#, None).unwrap();
        assert_eq!(out.tokens[0].value, "a\nb");
    }

    #[test]
    fn コメントはトークン列には乗らない() {
        use TokenType::*;
        assert_eq!(types("x // comment\ny"), vec![Ident, Semi, Ident, Semi, Eof]);
    }

    #[test]
    fn コメントは別配列へ位置つきで退避される() {
        let out = lex("x := 1 // trailing\n// leading\ny := 2", None).unwrap();
        assert_eq!(
            out.comments,
            vec![
                CommentInfo { text: "// trailing".into(), pos: Pos { line: 1, col: 8 } },
                CommentInfo { text: "// leading".into(), pos: Pos { line: 2, col: 1 } },
            ]
        );
    }

    #[test]
    fn 文字列補間_text_exprの部品に分解される() {
        let out = lex(r#""worker ${id} done""#, None).unwrap();
        assert_eq!(
            out.tokens[0].parts,
            Some(vec![
                StringPart::Text { text: "worker ".into() },
                StringPart::Expr { source: "id".into(), pos: Pos { line: 1, col: 11 } },
                StringPart::Text { text: " done".into() },
            ])
        );
    }

    #[test]
    fn 文字列補間_入れ子の文字列と波括弧を正しく数える() {
        let out = lex(r#""x${f({"k": 1})}y""#, None).unwrap();
        let parts = out.tokens[0].parts.as_ref().unwrap();
        assert_eq!(parts[1], StringPart::Expr { source: r#"f({"k": 1})"#.into(), pos: Pos { line: 1, col: 5 } });
    }

    #[test]
    fn 補間なしの文字列は従来どおり() {
        let out = lex(r#""plain""#, None).unwrap();
        assert_eq!(out.tokens[0].parts, None);
        assert_eq!(out.tokens[0].value, "plain");
    }

    #[test]
    fn バックスラッシュドルで補間をエスケープできる() {
        let out = lex(r#""price \$100""#, None).unwrap();
        assert_eq!(out.tokens[0].parts, None);
        assert_eq!(out.tokens[0].value, "price $100");
    }

    #[test]
    fn 空の補間はエラー() {
        let err = lex(r#""a${}b""#, None).unwrap_err();
        assert!(err.message.contains("empty interpolation"));
    }

    #[test]
    fn 閉じていない補間はエラー() {
        let err = lex(r#""a${x"#, None).unwrap_err();
        assert!(err.message.contains("interpolation not terminated"));
    }
}
