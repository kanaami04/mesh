// トークン = ソースコードを意味のある最小単位に分解したもの。
// 例: `x := 10` -> [ident "x"] [":="] [int "10"]
//
// TS版(src/token.ts)からの移植メモ:
// - TSの`TokenType`は文字列リテラルのunion(値そのものが"fn"のような文字列)だったが、
//   Rustでは列挙型(enum)にする。文字列の集合をコンパイラに保証させたいときはenumが定石
//   (TSでも本当はstring literal unionでほぼ同じ効果を狙っていた、という意味では発想は近い)
// - フィールド名`type`はRustの予約語なので`kind`に改名した(実はMeshのAST自体が
//   判別可能unionのタグ名に`kind`を使っているので、こちらの呼び方の方がプロジェクト全体の
//   流儀に合っている)
// - `parts?: StringPart[]`のような「無くてもよいフィールド」はRustでは`Option<T>`で表す。
//   Meshの`T | none`がRustの`Option<T>`そのものだと感じられるはず(実際、影響を受けている)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

// 行コメント1つ分。mesh fmt(将来)がASTへ再合成するための素材。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentInfo {
    pub text: String,
    pub pos: Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    // リテラル・識別子
    Ident,
    Int,
    Float,
    Str, // TSでは"string"。std::string::Stringと紛れないようStrに改名
    // キーワード
    Fn,
    Return,
    If,
    Else,
    For,
    Spawn,
    Detach,
    Wait,
    Mut,
    Chan,
    Map,
    Range,
    NoneKw, // TSでは"none"。Option::Noneと視覚的に紛れないようNoneKwに改名
    Is,
    Or,
    Match,
    Select,
    Type,
    Struct,
    Import,
    Export,
    True,
    False,
    Break,
    Continue,
    Defer,
    // 記号・演算子
    ColonEq, // :=
    EqEq,    // ==
    NotEq,   // !=
    Le,      // <=
    Ge,      // >=
    AndAnd,  // &&
    OrOr,    // ||
    Pipe,    // |
    Arrow,   // <-
    PlusPlus,
    MinusMinus,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    FatArrow, // =>
    Eq,
    Lt,
    Gt,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,     // !
    Question, // ?
    Comma,
    Colon,
    Semi,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Dot,
    Eof,
}

// parser.rsのエラーメッセージ用(`expected ':' after field name`のように、期待した
// トークン種別を人が読める形に戻す)。TS版はTokenType自体が文字列そのものだったので
// 素通しで済んでいたが、Rustのenumはここで文字列表現を明示的に用意する必要がある。
// 各キーワード・記号のTS側の綴りと一致させている(挙動の同一性を保つため)
impl std::fmt::Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TokenType::*;
        let s = match self {
            Ident => "ident",
            Int => "int",
            Float => "float",
            Str => "string",
            Fn => "fn",
            Return => "return",
            If => "if",
            Else => "else",
            For => "for",
            Spawn => "spawn",
            Detach => "detach",
            Wait => "wait",
            Mut => "mut",
            Chan => "chan",
            Map => "map",
            Range => "range",
            NoneKw => "none",
            Is => "is",
            Or => "or",
            Match => "match",
            Select => "select",
            Type => "type",
            Struct => "struct",
            Import => "import",
            Export => "export",
            True => "true",
            False => "false",
            Break => "break",
            Continue => "continue",
            Defer => "defer",
            ColonEq => ":=",
            EqEq => "==",
            NotEq => "!=",
            Le => "<=",
            Ge => ">=",
            AndAnd => "&&",
            OrOr => "||",
            Pipe => "|",
            Arrow => "<-",
            PlusPlus => "++",
            MinusMinus => "--",
            PlusEq => "+=",
            MinusEq => "-=",
            StarEq => "*=",
            SlashEq => "/=",
            PercentEq => "%=",
            FatArrow => "=>",
            Eq => "=",
            Lt => "<",
            Gt => ">",
            Plus => "+",
            Minus => "-",
            Star => "*",
            Slash => "/",
            Percent => "%",
            Bang => "!",
            Question => "?",
            Comma => ",",
            Colon => ":",
            Semi => ";",
            LParen => "(",
            RParen => ")",
            LBrace => "{",
            RBrace => "}",
            LBracket => "[",
            RBracket => "]",
            Dot => ".",
            Eof => "eof",
        };
        f.write_str(s)
    }
}

// 文字列補間: "worker ${id} done" は
// [Text("worker "), Expr("id", ...), Text(" done")] という部品列に分解される
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringPart {
    Text { text: String },
    Expr { source: String, pos: Pos }, // 式は未パースのソース断片として持つ
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenType,
    pub value: String,
    pub pos: Pos,
    pub parts: Option<Vec<StringPart>>, // 補間を含む文字列トークンだけが持つ
}

// キーワード判定。TSは`KEYWORDS: Set<TokenType>`だったが、Rustでは
// 「identとして読んだ文字列がキーワードならそのTokenTypeを返す」関数の方が素直
// (文字列→enumの変換をここに一箇所へ集約できる)
pub fn keyword_from_str(s: &str) -> Option<TokenType> {
    use TokenType::*;
    Some(match s {
        "fn" => Fn,
        "return" => Return,
        "if" => If,
        "else" => Else,
        "for" => For,
        "spawn" => Spawn,
        "detach" => Detach,
        "wait" => Wait,
        "mut" => Mut,
        "chan" => Chan,
        "map" => Map,
        "range" => Range,
        "none" => NoneKw,
        "is" => Is,
        "or" => Or,
        "match" => Match,
        "select" => Select,
        "type" => Type,
        "struct" => Struct,
        "import" => Import,
        "export" => Export,
        "true" => True,
        "false" => False,
        "break" => Break,
        "continue" => Continue,
        "defer" => Defer,
        _ => return None,
    })
}

// コンパイルエラー(構文エラーなど)を位置情報つきで表す。lexer/parser共通(TS版と同じ —
// parser.tsもtoken.tsのCompileErrorをそのまま使っている)。
// TS版のDiagnosticCode(diagnostic-codes.ts、87種)はまだ移植していない(checker.ts移植時に
// まとめて持ってくる)ので、今は各段が投げるコードを直接文字列で持たせる、意図的な簡略化
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    pub pos: Pos,
    pub code: &'static str,
    pub fix: Option<Fix>, // F-13: 機械適用可能な自動修正(範囲+置換テキスト)。無いことの方が多い
}

// diagnostic-codes.tsのFixと同じ形
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub description: String,
    pub range: Range,
    pub replacement: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Pos,
    pub end: Pos,
}
