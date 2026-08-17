//! 字句解析器。Meshソース文字列をトークン列に変換する。
//! 仕様は docs/spec/01-lexical.md が正。

/// トークンの種類。TDDサイクルで振る舞いを追加するたびにバリアントを増やす。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// 10進整数リテラル(例: `42`)。
    Int,
    /// 予約語 `let`(仕様1章1.5)。
    KwLet,
    /// 予約語 `mut`(仕様1章1.5)。
    KwMut,
    /// 予約語 `fn`(仕様1章1.5)。
    KwFn,
    /// 予約語 `struct`(仕様1章1.5)。
    KwStruct,
    /// 予約語 `type`(仕様1章1.5)。
    KwType,
    /// 予約語 `if`(仕様1章1.5)。
    KwIf,
    /// 予約語 `else`(仕様1章1.5)。
    KwElse,
    /// 予約語 `match`(仕様1章1.5)。
    KwMatch,
    /// 予約語 `for`(仕様1章1.5)。
    KwFor,
    /// 予約語 `in`(仕様1章1.5)。
    KwIn,
    /// 予約語 `return`(仕様1章1.5)。
    KwReturn,
    /// 予約語 `import`(仕様1章1.5)。
    KwImport,
    /// 予約語 `export`(仕様1章1.5)。
    KwExport,
    /// 予約語 `or`(仕様1章1.5)。
    KwOr,
    /// 予約語 `is`(仕様1章1.5)。
    KwIs,
    /// 予約語 `none`(仕様1章1.5)。
    KwNone,
    /// 予約語 `error`(仕様1章1.5)。
    KwError,
    /// 予約語 `extern`(仕様1章1.5)。
    KwExtern,
    /// 予約語 `true`(仕様1章1.5)。
    KwTrue,
    /// 予約語 `false`(仕様1章1.5)。
    KwFalse,
    /// 予約語 `break`(仕様1章1.5)。
    KwBreak,
    /// 予約語 `continue`(仕様1章1.5)。
    KwContinue,
    /// 識別子(仕様1章1.4)。
    Ident,
    /// 代入記号 `=`。
    Eq,
    /// 改行(仕様1章L-19の文終端の基盤)。
    Newline,
    /// 文字列リテラル(仕様1章1.7)。textはクォート込みの生の字面。
    /// 区分(セグメント)列を内包する(ADR-0042: 入れ子方式)。
    Str(Vec<StrSegment>),
    /// `(`(仕様1章1.9)。
    LParen,
    /// `)`(仕様1章1.9)。
    RParen,
    /// `[`(仕様1章1.9)。
    LBracket,
    /// `]`(仕様1章1.9)。
    RBracket,
    /// `{`(仕様1章1.9)。
    LBrace,
    /// `}`(仕様1章1.9)。
    RBrace,
    /// `,`(仕様1章1.9)。
    Comma,
    /// `+`(仕様1章1.9)。
    Plus,
    /// `-`(仕様1章1.9)。
    Minus,
    /// `*`(仕様1章1.9)。
    Star,
    /// `/`(仕様1章1.9)。
    Slash,
    /// `%`(仕様1章1.9)。
    Percent,
    /// `<`(仕様1章1.9)。
    Lt,
    /// `>`(仕様1章1.9)。
    Gt,
    /// `!`(仕様1章1.9)。
    Bang,
    /// `?`(仕様1章1.9)。
    Question,
    /// `.`(仕様1章1.9)。
    Dot,
    /// `|`(仕様1章1.9)。
    Pipe,
    /// `:`(仕様1章1.9)。
    Colon,
    /// `<=`(仕様1章1.9)。
    LtEq,
    /// `==`(仕様1章1.9)。
    EqEq,
    /// `..`(仕様1章1.9)。
    DotDot,
    /// `!=`(仕様1章1.9)。
    BangEq,
    /// `>=`(仕様1章1.9)。
    GtEq,
    /// `&&`(仕様1章1.9)。
    AmpAmp,
    /// `||`(仕様1章1.9)。
    PipePipe,
    /// `+=`(仕様1章1.9)。
    PlusEq,
    /// `-=`(仕様1章1.9)。
    MinusEq,
    /// `*=`(仕様1章1.9)。
    StarEq,
    /// `/=`(仕様1章1.9)。
    SlashEq,
    /// `%=`(仕様1章1.9)。
    PercentEq,
    /// `=>`(仕様1章1.9)。
    FatArrow,
    /// 10進floatリテラル(例: `3.14`)。仕様1章1.6・L-3。
    /// textは桁区切り `_` も含めたソースの生の字面のまま(Intと同じく正規化しない)。
    Float,
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

/// 文字列リテラルの区分(ADR-0042: 入れ子方式)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrSegment {
    /// テキスト片。textはエスケープ未解決の生の字面、spanはソース絶対位置(クォートを含まない)。
    Text { text: String, span: Span },
    /// 補間 `${...}`。tokensは通常の字句モードで再帰トークン化した列(仕様1章L-17)、
    /// spanは `${` から対応する `}` まで。
    Interp { tokens: Vec<Token>, span: Span },
}

/// 字句エラーのコード(仕様1章のE01xx)。バリアントは番号の昇順に並べる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// ブロックコメント `/*` は存在しない(仕様1章L-5)。
    E0102,
    /// 予約語の誤用(仕様1章L-7)。字句段階で発火するのは**誘導用予約語**の出現のみ。
    /// 完全予約語が識別子位置に現れた場合のE0104(L-8変種・6章H-2 error-as-value・
    /// H-4 error-type-declの変種を含む)は、Kw*トークンを受け取った**パーサが担当**する。
    E0104,
    /// 桁区切り `_` の位置違反(仕様1章L-9)。
    E0105,
    /// floatの小数点の隣に数字が無い(仕様1章L-10)。
    E0106,
    /// 整数リテラルが安全整数域±2^53−1を超えた(仕様1章L-11)。
    E0107,
    /// 文字列リテラル中の生の改行・閉じ`"`前のEOF(仕様1章L-16)。
    E0108,
    /// 補間 `${` が対応する `}` を得ないまま終わった(仕様1章L-18)。
    E0109,
    /// 文末のセミコロン(仕様1章L-19)。セミコロンは存在しない。
    E0110,
    /// 一覧に無いエスケープ(仕様1章L-14)。
    E0111,
    /// `\u{H}` の範囲・形式違反(仕様1章L-15)。
    E0112,
    /// 不正な数値リテラル(仕様1章L-12)。
    E0113,
    /// floatリテラルがIEEE754倍精度で表現できない大きさ(仕様1章L-13)。
    E0114,
    /// 補間の内側のコメント `//`(仕様1章L-17(b))。
    E0115,
    /// どの字句規則にも該当しない文字(仕様1章L-26キャッチオール)。
    /// 注意: 固有の規則を持つが未実装の文字(非ASCII識別子=E0103)も
    /// 現状は暫定でこのコードになる。各規則の実装サイクルで正しいコードに置き換える。
    /// 単独の `&`(直後が `&` でない)は仕様1.9に無いため**恒久的に**このコード
    /// (`&&` の一部としてのみ有効。単独が正当な `|` との非対称)。
    /// 1.9の演算子・区切り記号33種は全実装済みで、「未実装の正当なトークン」による
    /// 暫定E0116はもう無い。
    /// Unicode改行類(U+0085/U+2028/U+2029)は**確定で**このコード
    /// (L-29が非改行と規定=ADR-0041。上の暫定と違い置き換え予定なし)。
    E0116,
    /// 単独のCR——直後が `\n` でない `\r`(仕様1章L-29)。
    E0117,
    /// 文字列と補間のネストが実装上限64段を超えた(仕様1章L-31)。
    E0118,
}

/// 字句解析エラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub code: ErrorCode,
    pub span: Span,
}

/// 文字列と補間のネストの実装上限(仕様1章L-31)。
/// 「文字列1個」を1段と数える(`"${"..."}"` は2段)。
/// 再帰下降の字句解析はネスト1段ぶんスタックを消費するため、
/// 深すぎる入力でスタックオーバーフロー(プロセスごとの異常終了)になる前に
/// E0118として正常なエラーで打ち切る。
const MAX_NEST_DEPTH: usize = 64;

/// 整数リテラルの安全整数域の上限、2^53−1(仕様1章L-11・ADR-0015)。
/// IEEE754倍精度浮動小数点数(JS出力先のNumber型)が誤差なく表現できる整数の上限で、
/// Meshの整数リテラルはこれを超えるとE0107になる。字句には符号が無く
/// (`-` は別トークン)リテラル自体は常に非負のため、この定数は絶対値の上限として働く。
/// 下限側(`-9007199254740991` を下回る式)の判定は字句解析の対象外で、
/// パーサ・型検査が式全体を評価してから担当する。
const MAX_SAFE_INT: u128 = 9_007_199_254_740_991;

/// トークン種類が行末継続トークンかどうかを判定する(仕様1章L-20)。
/// これらのトークンが行末にあるとき、改行はNewlineトークンを生成しない。
/// コメントはトークンを生成しないため、判定は「コメント除去後の行末」で自動的に成立する。
fn is_continuation_token(kind: &TokenKind) -> bool {
    matches!(
        kind,
        // 二項演算子(14種)
        TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::EqEq
            | TokenKind::BangEq
            | TokenKind::Lt
            | TokenKind::LtEq
            | TokenKind::Gt
            | TokenKind::GtEq
            | TokenKind::AmpAmp
            | TokenKind::PipePipe
            | TokenKind::Pipe
            // キーワード演算子(3種)
            | TokenKind::KwOr
            | TokenKind::KwIs
            | TokenKind::KwIn
            // 複合代入(5種)
            | TokenKind::PlusEq
            | TokenKind::MinusEq
            | TokenKind::StarEq
            | TokenKind::SlashEq
            | TokenKind::PercentEq
            // 記号類(8種)
            | TokenKind::Dot
            | TokenKind::DotDot
            | TokenKind::Comma
            | TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::LBrace
            | TokenKind::Eq
            | TokenKind::FatArrow
    )
}

/// 括弧深度スタックの要素(仕様1章L-21・ADR-0031)。
/// TokenKindでなく専用enumで持つことで、「スタックに入るのは開き括弧3種だけ」を型で保証する
/// (TokenKindは `Str(Vec<_>)` を含むためCopy不可で、pushのたびにcloneが必要になってしまう)。
#[derive(Clone, Copy)]
enum BracketKind {
    Paren,
    Bracket,
    Brace,
}

impl BracketKind {
    /// 開き括弧トークンから括弧種への変換。開き括弧3種以外はNone。
    fn from_open(kind: &TokenKind) -> Option<Self> {
        match kind {
            TokenKind::LParen => Some(Self::Paren),
            TokenKind::LBracket => Some(Self::Bracket),
            TokenKind::LBrace => Some(Self::Brace),
            _ => None,
        }
    }

    /// この括弧の内側で改行を抑制するか(仕様1章L-21)。
    /// `(`/`[` は抑制、`{` はブロック内で終端規則を復活させるため抑制しない。
    fn suppresses_newline(self) -> bool {
        matches!(self, Self::Paren | Self::Bracket)
    }
}

/// ソース文字列を字句解析してトークン列を返す。
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    // 括弧深度スタック(仕様1章L-21・ADR-0031)。`(` または `[` の内側では改行を抑制し、
    // `{ ... }` ブロックに入ったら終端規則を復活させるため、開き括弧の種類を記録する。
    let mut brackets: Vec<BracketKind> = Vec::new();
    while chars.peek().is_some() {
        if let Some(t) = next_token(source, &mut chars, false, 0)? {
            // 終端する改行だけをNewlineトークンとして出力する(ADR-0010のGo方式)。
            // 直前のトークンが無い(入力先頭)かNewlineか継続トークン(仕様1章L-20)のとき、
            // 改行はトークンを生成しない。L-21の括弧深度抑制もここで判定する。
            if t.kind == TokenKind::Newline {
                if should_emit_newline(&tokens, &brackets) {
                    tokens.push(t);
                }
            } else {
                // 括弧スタックの更新(仕様1章L-21)。開き括弧はpushして保持、閉じ括弧はpopして破棄。
                // 空スタックでのpopは括弧の不均衡検査が字句の責務でないため何もしない
                // (補間の内側以外では不均衡はパーサが担当する)。
                if let Some(b) = BracketKind::from_open(&t.kind) {
                    brackets.push(b);
                } else if matches!(
                    t.kind,
                    TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace
                ) {
                    brackets.pop();
                }
                tokens.push(t);
            }
        }
    }
    Ok(tokens)
}

/// Newlineトークンを生成すべきかを判定する。
/// トークン列が空でなく、直前のトークンがNewlineでも継続トークンでもないときtrue。
/// さらに、括弧深度スタックのトップが `(` または `[` のときは抑制する
/// (仕様1章L-21。`(`/`[` の内側では改行は終端しない)。`{` またはスタック空のときは
/// 終端規則が復活する( `{` ブロック内では文の行末にNewlineを生成する)。
fn should_emit_newline(tokens: &[Token], brackets: &[BracketKind]) -> bool {
    // 仕様1章L-21: `(`/`[` の内側では改行を抑制する(スタックのトップで判定)。
    // トップが `{` のときはこの抑制を通過し、下の終端判定が復活する。
    if brackets.last().is_some_and(|top| top.suppresses_newline()) {
        return false;
    }
    // 既存の終端判定(ADR-0010・仕様1章L-19/L-20)。
    if let Some(last) = tokens.last() {
        !matches!(last.kind, TokenKind::Newline) && !is_continuation_token(&last.kind)
    } else {
        false
    }
}

/// 基数接頭辞つき整数リテラルの基数判定結果: (その基数での数字述語, 数値としての基数)。
/// clippyのtype_complexity対策で `next_token` 内のインライン型をここに切り出した。
type RadixInfo = (fn(char) -> bool, u32);

/// 現在位置から1トークンぶん読み進める(字句解析の1ステップ)。
/// 戻り値の `None` は「このステップではトークンを生成しなかった」ことを表す:
/// 空白・行コメントを読み飛ばした場合と、EOFに達している場合の両方。
/// 呼び出し側はEOFの判定を `chars.peek()` で行う(空白の読み飛ばしでは位置が進むため
/// ループは止まらない)。補間 `${...}` の内側も同じ通常の字句モードでトークン化するため、
/// メインループとこの関数の両方から呼ばれる(仕様1章L-17)。
/// `in_interpolation` は呼び出し元が補間 `${...}` の内側かどうかを伝える。
/// コメントはトークンを生成しないため補間ループ側からは検知できず、
/// 行コメント分岐自身がこのフラグを見てE0115を判定する(仕様1章L-17(b))。
/// `depth` は「今いる位置を囲んでいる文字列リテラルの段数」(最外側は0)。
/// 文字列分岐がこれを見てネスト上限(MAX_NEST_DEPTH)を判定するため、
/// メインループ → 文字列 → 補間 → メインループ相当 の経路を貫通して受け渡す。
fn next_token(
    source: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    in_interpolation: bool,
    depth: usize,
) -> Result<Option<Token>, LexError> {
    let Some(&(start, c)) = chars.peek() else {
        return Ok(None);
    };
    if c.is_ascii_digit() {
        scan_number(source, chars, start, c).map(Some)
    } else if c.is_ascii_alphabetic() || c == '_' {
        let (text, end) = scan_while(source, chars, start, c, |d| {
            d.is_ascii_alphanumeric() || d == '_'
        });
        if let Some(kind) = keyword_kind(text) {
            Ok(Some(token(kind, text, Span { start, end })))
        } else if guidance_reserved(text) {
            // 誘導用予約語(仕様1章1.5・L-7)。文法上の正当な出現位置が存在しない
            // ため、字句段階で即エラーにする。spanはスキャン済みの語全体。
            Err(LexError {
                code: ErrorCode::E0104,
                span: Span { start, end },
            })
        } else {
            Ok(Some(token(TokenKind::Ident, text, Span { start, end })))
        }
    } else if c == '/' && matches!(peek_at(source, start + '/'.len_utf8()), Some('/' | '*')) {
        // `/` の2文字目を**消費せずに**先読み(peek_at)して、コメント(`//`・`/*`)だけを
        // ここで捌く(仕様1章L-4・L-5)。単独の `/` はこの分岐に入らず、下の1文字演算子表で
        // Slashトークン(除算)になる(仕様1章1.9)。
        chars.next();
        let (_, second) = chars.next().expect("ガードで2文字目の存在を確認済み");
        if second == '/' {
            if in_interpolation {
                // 補間 `${...}` の内側では行コメントを許さない(仕様1章L-17(b))。
                // spanは `//` の2バイト(両方ASCIIなので各1バイト)。
                return Err(LexError {
                    code: ErrorCode::E0115,
                    span: Span {
                        start,
                        end: start + '/'.len_utf8() + '/'.len_utf8(),
                    },
                });
            }
            // 行コメント(仕様1章L-4)。`\n` または `\r` の手前まで読み飛ばし、トークンは生成しない。
            // `\n`/`\r` 自体は消費せず、既存のNewline/CR分岐に処理を委ねる
            // (CRLF判定とE0117=孤立CRの検出は既存の `\r` 分岐が担う)。
            while let Some(&(_, d)) = chars.peek() {
                if d == '\n' || d == '\r' {
                    break;
                }
                chars.next();
            }
            Ok(None)
        } else {
            // ブロックコメント `/*`(仕様1章L-5)。ブロックコメントは未サポート。
            // spanは `/` と `*` の2バイトぶん(両方ASCIIなので各1バイト)。
            Err(LexError {
                code: ErrorCode::E0102,
                span: Span {
                    start,
                    end: start + '/'.len_utf8() + '*'.len_utf8(),
                },
            })
        }
    } else if c == '-' && peek_at(source, start + c.len_utf8()) == Some('>') {
        // `->` は仕様に存在しない記号列(仕様1章L-26: 近い正解への誘導)。
        // `=>`(FatArrow)と紛らわしいアロー風の綴りのため、`-`+`>` に分割せず
        // 記号列全体をE0116として報告する。2文字演算子表(two_char_operator_kind)には
        // この組は載せない——載せると正当な2文字演算子と区別が付かなくなるため、
        // 表の手前に専用ガードとして置く。修正候補の案内文言はエラーメッセージ層の担当。
        chars.next();
        chars.next();
        let end = start + '-'.len_utf8() + '>'.len_utf8();
        Err(LexError {
            code: ErrorCode::E0116,
            span: Span { start, end },
        })
    } else if let Some((kind, second)) = peek_at(source, start + c.len_utf8())
        .and_then(|second| two_char_operator_kind(c, second).map(|kind| (kind, second)))
    {
        // 2文字演算子の最長一致(仕様1章L-2)。1文字目を**消費する前に**2文字目を
        // 先読み(peek_at)し、表に載っていれば2文字まとめて1トークンにする。
        // 該当しなければこの分岐に入らず、下の1文字表(punctuation_kind・operator_kind)に落ちる。
        chars.next();
        chars.next();
        let end = start + c.len_utf8() + second.len_utf8();
        if kind == TokenKind::EqEq && peek_at(source, end) == Some('=') {
            // `===` は仕様に存在しない記号列(仕様1章L-26)。JS由来の厳密等価演算子への
            // 誘導のため、2文字表で `==`(EqEq)が引けた後さらに3文字目を**非消費先読み**
            // して判定する(`====` 以降の続きは考えない。最初の3文字でエラー確定)。
            // `==`+`=` に分割せず記号列全体をE0116として報告する。
            // 修正候補の案内文言はエラーメッセージ層の担当。
            // 3文字目の消費は現状観測不能(直後にErrで打ち切るため)だが、
            // 将来のエラー回復(複数エラー報告)で位置がspanと一致するよう進めておく。
            chars.next();
            Err(LexError {
                code: ErrorCode::E0116,
                span: Span {
                    start,
                    end: end + '='.len_utf8(),
                },
            })
        } else {
            Ok(Some(token(kind, &source[start..end], Span { start, end })))
        }
    } else if c == '.'
        && peek_at(source, start + '.'.len_utf8()).is_some_and(|d| d.is_ascii_digit())
    {
        // 整数部が無い `.5`(仕様1章L-10)。数字直後の `.` はメンバアクセスと解釈しない
        // ——数値リテラルへのフィールドアクセスはstruct限定(仕様3章X-30)によりそもそも
        // 合法でないため、「メンバアクセスの `.`」候補は最初から存在しない。書きかけの
        // 浮動小数点リテラルとみなしE0106として報告する。
        // `..5` はこの分岐より前にある2文字演算子表(two_char_operator_kind)で
        // `..`(DotDot)が先に引かれるため、ここに落ちてくるのは `.` の直後が
        // 数字のとき(`..` の2文字目には該当しない)に限られる。
        // spanは `.`+数字列全体(`scan_while` で数字列の終端まで測る。`.5`→0..2、`.55`→0..3)。
        chars.next();
        let &(digit_start, digit_first) = chars.peek().expect("先読みで数字を確認済み");
        let (_, end) = scan_while(source, chars, digit_start, digit_first, |d| {
            d.is_ascii_digit() || d == '_'
        });
        Err(LexError {
            code: ErrorCode::E0106,
            span: Span { start, end },
        })
    } else if c == ';' {
        // セミコロン(仕様1章L-19)。セミコロンは存在しない。
        chars.next();
        let end = start + ';'.len_utf8();
        Err(LexError {
            code: ErrorCode::E0110,
            span: Span { start, end },
        })
    } else if let Some(kind) = punctuation_kind(c).or_else(|| operator_kind(c)) {
        chars.next();
        let end = start + c.len_utf8();
        // textはリテラルでなくソースから切り出す(text==source[span]の不変条件を
        // 分岐条件との二重管理でなく構造で守る)
        Ok(Some(token(kind, &source[start..end], Span { start, end })))
    } else if c == '\n' {
        chars.next();
        let end = start + '\n'.len_utf8();
        Ok(Some(token(
            TokenKind::Newline,
            &source[start..end],
            Span { start, end },
        )))
    } else if c == '\r' {
        chars.next();
        if chars.peek().map(|&(_, d)| d) == Some('\n') {
            chars.next();
            // CRLFは1個の改行(仕様1章L-29)。endは他分岐と同じくバイト長から導出する
            let end = start + '\r'.len_utf8() + '\n'.len_utf8();
            Ok(Some(token(
                TokenKind::Newline,
                &source[start..end],
                Span { start, end },
            )))
        } else {
            Err(LexError {
                code: ErrorCode::E0117,
                span: Span {
                    start,
                    end: start + '\r'.len_utf8(),
                },
            })
        }
    } else if c == '"' {
        scan_string(source, chars, start, depth).map(Some)
    } else if c == ' ' || c == '\t' {
        chars.next();
        Ok(None)
    } else {
        Err(LexError {
            code: ErrorCode::E0116,
            span: Span {
                start,
                end: start + c.len_utf8(),
            },
        })
    }
}

/// 数値リテラル1個(10進int・基数int・float)を読み進めてトークンにする(仕様1章1.6)。
/// `start` は先頭数字の位置、`c` はその文字。呼び出し側は `c.is_ascii_digit()` を確認済み。
/// 基数接頭辞(0x/0b/0o)→整数部→小数点(L-3の消極規則)→指数部の順に読み、
/// 桁区切り(L-9/E0105)→末尾の貼り付き・先頭ゼロ(L-12/E0113)→値域(L-11/E0107・
/// L-13/E0114)の順で検査する。検査の優先順位の理由は各所のコメントを参照。
fn scan_number(
    source: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    start: usize,
    c: char,
) -> Result<Token, LexError> {
    // 基数接頭辞つき整数リテラル(仕様1章1.6: `int = decInt | "0x" hexInt | "0b" binInt | "0o" octInt`)。
    // `0` の直後が `x`/`b`/`o` で、かつその次が対応する基数の有効数字のときだけ
    // 基数リテラルとして読む(2文字先読みの最長一致)。この条件を満たさない場合
    // (`0x` の後に有効数字が無い、`0z`・`0o8`・大文字接頭辞 `0X` 等)は下の10進読み取りに
    // 委ねる。落ちた先では `0` の直後に識別子文字が残るため、末尾検査
    // (check_trailing_alnum)がE0113として拾う(仕様1章L-12)。
    if c == '0' {
        let prefix = peek_at(source, start + '0'.len_utf8());
        // 数字述語(桁区切り検査用)と基数(値域検査用、E0107)を組で持つ。
        let radix_info: Option<RadixInfo> = match prefix {
            Some('x') => Some((|d: char| d.is_ascii_hexdigit(), 16)),
            Some('b') => Some((|d: char| matches!(d, '0' | '1'), 2)),
            Some('o') => Some((|d: char| matches!(d, '0'..='7'), 8)),
            _ => None,
        };
        if let Some((is_radix_digit, radix)) = radix_info {
            let prefix_char = prefix.expect("is_radix_digitがSomeならprefixもSome");
            let digits_at = start + '0'.len_utf8() + prefix_char.len_utf8();
            // 接頭辞の次が有効数字**または桁区切り `_`** なら基数リテラルとして読む。
            // `_` を条件に含めるのは `0x_FF` を「Int("0")+Ident("x_FF")」に割らず、
            // 桁区切りの位置違反(E0105)として診断するため(仕様1章L-9注1)。
            if peek_at(source, digits_at).is_some_and(|d| is_radix_digit(d) || d == '_') {
                // `0` と接頭辞(x/b/o)を消費し、基数の有効数字列(と桁区切り `_`)を読む。
                chars.next();
                chars.next();
                let &(digit_start, digit_first) = chars
                    .peek()
                    .expect("先読みで本体1文字目の存在を確認済みのため必ず1文字ある");
                let (_, end) = scan_while(source, chars, digit_start, digit_first, |d| {
                    is_radix_digit(d) || d == '_'
                });
                let text = &source[start..end];
                // 桁区切り `_` の検査を**基数別の数字判定**で行う(仕様1章L-9)。
                // 「数字」の集合は基数ごとに違う(hexInt=0-9a-fA-F、binInt=0|1、
                // octInt=0-7)ため、10進固定の判定を流用すると `0xF_F` を誤って
                // E0105にしてしまう。そこでcheck_digit_separatorsを数字述語で
                // 一般化し、ここでは基数の述語を渡す。検査対象は接頭辞を除いた
                // **本体**(`0x` の後ろ)で、接頭辞 `0x` の `0` を「直前の数字」と
                // 誤認しないためにこう切る。オフセットにdigits_atを渡すので
                // spanは違反した `_` の実位置を指す。
                check_digit_separators(&source[digits_at..end], digits_at, |b| {
                    is_radix_digit(char::from(b))
                })?;
                // 末尾検査(仕様1章L-12)。基数スキャンが止まった直後に英数字・`_` が
                // 続くのは基数外の数字(`0b102` の `2`)か語の貼り付き(`0xFFg`)で、
                // どちらも不正な数値リテラル。詳細は check_trailing_alnum を参照。
                check_trailing_alnum(source, chars, start)?;
                // 値の範囲検査(仕様1章L-11・E0107)は末尾検査・先頭ゼロ相当の形式検査より後。
                // 対象は接頭辞を除いた本体(digits_at以降)で、基数はここで確定している。
                check_int_overflow(&source[digits_at..end], Span { start, end }, radix)?;
                return Ok(token(TokenKind::Int, text, Span { start, end }));
            }
        }
    }
    // 数値リテラル(仕様1章1.6)。まず整数部(数字と桁区切り `_`)を最長一致で読む。
    // (ここに到達するのは基数接頭辞リテラルでない場合のみ。上のブロックで
    // 早期returnするため、以降のfloat/指数部判定に基数リテラルが混入することはない。)
    let (_, mut end) = scan_while(source, chars, start, c, |d| d.is_ascii_digit() || d == '_');
    // `.` を小数点として取り込むのは**直後がASCII数字のときだけ**(仕様1章L-3)。
    // `1..5` の `..`(DotDot)や `1.method` の `.`(Dot)を数値に飲み込まないための
    // 消極規則で、判定は2文字ぶんの非消費先読み(peek_at)で行う。`.` も次の文字も
    // ASCIIなので `end + 1` は必ず文字境界。取り込まない場合は `.` を消費せず
    // Intで止め、演算子側の分岐(DotDot/Dot)に処理を委ねる。
    let dot_at = end;
    let is_dot = peek_at(source, dot_at) == Some('.');
    // `after_dot` は `is_dot` が真のとき(=dot_atに `.` がある)だけ求める。
    // `.` は1バイトASCIIなので `dot_at + 1` は常に文字境界だが、`.` が
    // 無い(=EOF等でdot_atが文字列長を超え得る)場合まで先読みすると範囲外になる。
    let after_dot = if is_dot {
        peek_at(source, dot_at + '.'.len_utf8())
    } else {
        None
    };
    let mut is_float = is_dot && after_dot.is_some_and(|d| d.is_ascii_digit());
    if is_float {
        // `.` を消費し、続く小数部(整数部と同じく数字と `_`)を読む。
        chars.next();
        let &(frac_start, frac_first) = chars.peek().expect("先読みで小数部の数字を確認済み");
        let (_, frac_end) = scan_while(source, chars, frac_start, frac_first, |d| {
            d.is_ascii_digit() || d == '_'
        });
        end = frac_end;
    } else if is_dot && after_dot != Some('.') {
        // 小数部が無い `0.`、識別子が続く `1.abs` 等(仕様1章L-10)。`.` の直後が
        // 数字でないのにfloatとして続けるのは書きかけの小数点リテラルとみなしE0106。
        // `..`(範囲演算子DotDot)だけは除外する: `0..10` は上のfloat判定で
        // 弾かれた後もここで拾わず、Int確定のまま演算子側の2文字表(DotDot)に委ねる
        // (既存の消極規則を維持。`dotdot_after_integer_splits_range` が退行の網)。
        // spanは数字列+`.`の全体(`start..(dotの直後)`)。
        return Err(LexError {
            code: ErrorCode::E0106,
            span: Span {
                start,
                end: dot_at + '.'.len_utf8(),
            },
        });
    }
    // 指数部(仕様1章1.6の `exponent = ("e"|"E") ["+"|"-"] decInt`)。
    // 小数点の有無に関わらず判定する(`decInt exponent` の形も認めるため、
    // `1e6` は小数点が無くてもFloat)。
    // 取り込むのは**`e`/`E`(と省略可能な符号)の直後がASCII数字または桁区切り `_`
    // のときだけ**という消極規則。`_` を含めるのは `1e_6` を「Int("1")+Ident("e_6")」に
    // 割らず、下の桁区切り検査でE0105として診断するため(仕様1章L-9注1)。
    // `1e` や `1e+` のように数字も `_` も続かない場合は `e` を1文字も消費せず
    // 数値をInt/Floatで確定し、`e...` は識別子側の分岐(`1end` → Int+Ident)や
    // 後続の診断(E0113系)に委ねる。字句段階で先に `e` を食べてしまうと、
    // それらの処理が本来の字面を見られなくなる。
    // 先読みは `e`・符号・数字すべてASCIIなので、`peek_at` に渡すオフセットは常に文字境界。
    if let Some(marker) = peek_at(source, end).filter(|d| matches!(d, 'e' | 'E')) {
        let after_marker = end + marker.len_utf8();
        let sign = peek_at(source, after_marker).filter(|d| matches!(d, '+' | '-'));
        let digits_at = after_marker + sign.map_or(0, char::len_utf8);
        if peek_at(source, digits_at).is_some_and(|d| d.is_ascii_digit() || d == '_') {
            // ここで初めて消費する: `e`/`E`、あれば符号、続けて数字列。
            chars.next();
            if sign.is_some() {
                chars.next();
            }
            let &(exp_start, exp_first) = chars.peek().expect("先読みで指数部1文字目を確認済み");
            let (_, exp_end) = scan_while(source, chars, exp_start, exp_first, |d| {
                d.is_ascii_digit() || d == '_'
            });
            end = exp_end;
            is_float = true;
        }
    }
    // 桁区切り `_` の検査(仕様1章L-9)は `.` を跨いだ字面全体にかける
    // (`1_000.5` の整数部・小数部を1回で見る)。`.` は数字でないため
    // 「`_` の隣が数字」の判定はそのまま働く(`1_.5` は `_` の直後が `.` でE0105)。
    // 数字述語は10進固定(`is_ascii_digit`)。指数部の `1e_6` も同じ検査で捕まる
    // (`_` の直前が `e` で非数字のため)ので、指数部専用の検査は要らない。
    let text = &source[start..end];
    check_digit_separators(text, start, |b: u8| b.is_ascii_digit())?;
    // 不正な数値リテラルの検査2つ(仕様1章L-12)。順序は
    // **末尾検査 → 先頭ゼロ検査**に固定する。両方に該当する `0755abc` は
    // 末尾検査が先に発火し、spanが字面全体(`0..7`)になる方を採る
    // (どちらもE0113なのでコードは変わらず、spanが広い側=読み手が直す範囲を
    // すべて指す側を選んだ)。
    //
    // 桁区切り検査(E0105)より**後**に置くのは仕様1章L-9注2の精神による:
    // `1_x` は「`_` の位置違反」とだけ言えばよく、E0113を重ねて出さない。
    check_trailing_alnum(source, chars, start)?;
    // 先頭ゼロ検査: 10進の全形(int・小数・指数)で整数部が `0` で始まり2文字以上なら不正
    // (仕様1章L-12注2)。`00`・`0755`・`00.5`・`01e5` が該当。`0` 単独・`0.5`・`0e5` は正当。
    // 基数リテラル(`0b1010`)は上のブロックで早期returnするためここには来ない。
    // 判定対象は整数部(小数点・`e`/`E` より前の数字列)で、spanはリテラル全体。
    let int_part_end = text.find(['.', 'e', 'E']).unwrap_or(text.len());
    let int_part = &text[..int_part_end];
    if int_part.len() > 1 && int_part.starts_with('0') {
        return Err(LexError {
            code: ErrorCode::E0113,
            span: Span { start, end },
        });
    }
    // 値の範囲検査(仕様1章L-11・E0107)。桁区切り・形式の検査(E0105/E0113)より後、
    // Int確定の直前が最後の関門。floatには適用しない(L-13/E0114は別サイクル)。
    if !is_float {
        check_int_overflow(text, Span { start, end }, 10)?;
    } else {
        check_float_overflow(text, Span { start, end })?;
    }
    // Int・Floatとも text はソースの生の字面のまま持つ(正規化しない)。
    // 値への変換は後段の担当で、字句解析器は位置と字面の対応を壊さない。
    let kind = if is_float {
        TokenKind::Float
    } else {
        TokenKind::Int
    };
    Ok(token(kind, text, Span { start, end }))
}

/// 文字列リテラル1個を読み進めてトークンにする(仕様1章1.7)。`start` は開き `"` の位置。
/// 開き `"` を消費し、閉じ `"` まで読み進める。textはクォート込みの生の字面。
/// `\` はエスケープとして扱う: 直後の1文字が許可一覧
/// (`n t r \ " $ u`、仕様1章L-14)にあれば消費して透過し、
/// その文字が `"` であっても閉じクォートと判定しない。
/// 一覧に無い場合(EOF含む)はE0111({バックスラッシュ位置}..{違反文字直後})。
/// `\u{...}` は形式と値をまとめて検証する(check_unicode_escape、E0112)。
/// `${` からは補間区分に切り替える(scan_interpolation、仕様1章L-17)。
/// `depth` はこの文字列を囲んでいる文字列の段数で、この文字列自身は `depth + 1` 段目にあたる。
/// `depth + 1` がMAX_NEST_DEPTHを超えるときは1文字も走査せずE0118を返す(仕様1章L-31)。
/// spanは開き `"` の1バイト(未終端文字列E0108と同じ流儀で、深すぎる文字列の頭を指す)。
fn scan_string(
    source: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    start: usize,
    depth: usize,
) -> Result<Token, LexError> {
    let depth = depth + 1;
    if depth > MAX_NEST_DEPTH {
        return Err(LexError {
            code: ErrorCode::E0118,
            span: Span {
                start,
                end: start + '"'.len_utf8(),
            },
        });
    }
    chars.next();
    let mut end = None;
    let mut segments = Vec::new();
    // 現在蓄積中のテキスト区分の開始位置。区切り(`${`・閉じ `"`)ごとに切り出して進める。
    let mut text_start = start + '"'.len_utf8();
    // `text_start..cut` をテキスト区分として閉じる。空(長さ0)なら区分を作らない。
    let cut_text = |segments: &mut Vec<StrSegment>, text_start: usize, cut: usize| {
        if text_start != cut {
            segments.push(StrSegment::Text {
                text: source[text_start..cut].to_string(),
                span: Span {
                    start: text_start,
                    end: cut,
                },
            });
        }
    };
    while let Some(&(i, d)) = chars.peek() {
        if d == '"' {
            chars.next();
            cut_text(&mut segments, text_start, i);
            end = Some(i + '"'.len_utf8());
            break;
        } else if d == '$' {
            // `$` の直後が `{` のときだけ補間の開始(仕様1章L-17)。
            // それ以外の `$` は普通の文字としてテキスト区分に含める(仕様1章L-28)。
            chars.next();
            if chars.peek().map(|&(_, e)| e) != Some('{') {
                continue;
            }
            chars.next();
            let (tokens, interp_end) = scan_interpolation(source, chars, i, depth)?;
            cut_text(&mut segments, text_start, i);
            segments.push(StrSegment::Interp {
                tokens,
                span: Span {
                    start: i,
                    end: interp_end,
                },
            });
            text_start = interp_end;
        } else if d == '\n' || d == '\r' {
            // 文字列リテラルの内側では生の改行(LF・CR単独とも)はE0108
            // (仕様1章L-16。文字列内ではE0108がL-29=E0117より優先=ADR-0041)。
            return Err(LexError {
                code: ErrorCode::E0108,
                span: Span {
                    start: i,
                    end: i + d.len_utf8(),
                },
            });
        } else if d == '\\' {
            let backslash = i;
            chars.next();
            match chars.peek().copied() {
                Some((j, nl @ ('\n' | '\r'))) => {
                    // `\` の直後が生の改行(LF・CR単独とも)のときは、一覧に無い
                    // エスケープ文字(E0111)ではなくL-16の未終端文字列として扱う
                    // (仕様1章L-16)。spanは改行1バイトのみ(行をまたがない)。
                    return Err(LexError {
                        code: ErrorCode::E0108,
                        span: Span {
                            start: j,
                            end: j + nl.len_utf8(),
                        },
                    });
                }
                Some((u_index, 'u')) => {
                    chars.next();
                    check_unicode_escape(chars, backslash, u_index + 'u'.len_utf8())?;
                }
                Some((_, 'n' | 't' | 'r' | '\\' | '"' | '$')) => {
                    chars.next();
                }
                Some((j, e)) => {
                    return Err(LexError {
                        code: ErrorCode::E0111,
                        span: Span {
                            start: backslash,
                            end: j + e.len_utf8(),
                        },
                    });
                }
                None => {
                    // `\` の直後にEOFに達したときも一覧に無いエスケープ文字
                    // (E0111)ではなくL-16の未終端文字列として扱う(仕様1章L-16)。
                    // spanは未終端文字列の流儀どおり開き `"` の1バイト。
                    return Err(LexError {
                        code: ErrorCode::E0108,
                        span: Span {
                            start,
                            end: start + '"'.len_utf8(),
                        },
                    });
                }
            }
        } else {
            chars.next();
        }
    }
    let end = end.ok_or(LexError {
        code: ErrorCode::E0108,
        span: Span {
            start,
            end: start + '"'.len_utf8(),
        },
    })?;
    Ok(token(
        TokenKind::Str(segments),
        &source[start..end],
        Span { start, end },
    ))
}

/// 補間 `${...}` の内側を通常の字句モードでトークン化する(仕様1章L-17)。
/// `${` を消費した直後から呼び、対応する閉じ `}` まで読み進めて
/// (トークン列, `}` の直後のバイトオフセット)を返す。
/// 対応関係は**トークン列上**の括弧の対応で決める。`( [ {` は対応する閉じ括弧を
/// スタックに積み、閉じ括弧が現れたら:
/// - スタックが空で `}` なら、それが補間の対応する閉じ(この `}` はトークン列に含めない)。
/// - スタック頂上と種類が一致すれば取り出して続行。
/// - スタックが空(`}` 以外)、または種類が一致しなければ不均衡としてE0109
///   (spanは違反した閉じ括弧そのもの。仕様1章L-18)。
///
/// 閉じ `}` を見ないままEOFに達した場合、または内側に生の改行(Newlineトークン、
/// および通常字句モードならE0117になる単独CR)が現れた場合もE0109
/// (未終端。仕様1章L-17(a)の物理1行制約、L-18)で、こちらのspanは `${` の2バイト
/// (`dollar` は呼び出し側が渡す `$` の開始バイト位置)。ネストした文字列リテラルが
/// 閉じ `"` 前にEOFへ達した場合のE0108も、対応する `}` を得られない未終端として
/// 同じくE0109へ読み替える(仕様1章L-18改訂: 終端系エラーの統一)。
/// E0108・E0117以外の内側のエラーはそのまま伝播する(内側エラー優先。仕様1章L-18注)。
/// `depth` は囲んでいる文字列の段数で、内側の文字列リテラルへそのまま引き継ぐ。
fn scan_interpolation(
    source: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    dollar: usize,
    depth: usize,
) -> Result<(Vec<Token>, usize), LexError> {
    let unterminated = || LexError {
        code: ErrorCode::E0109,
        span: Span {
            start: dollar,
            end: dollar + "${".len(),
        },
    };
    let mut tokens = Vec::new();
    // 未対応の開き括弧に対して「期待される閉じ括弧」を積むスタック。
    // 深度カウンタでなく種類を持つことで `(` に対する `}` `]` を不均衡として検出できる。
    let mut expected_closers: Vec<TokenKind> = Vec::new();
    while chars.peek().is_some() {
        let t = match next_token(source, chars, true, depth) {
            Ok(Some(t)) => t,
            // 空白・行コメントの読み飛ばし。位置は進んでいるのでループを続ける。
            Ok(None) => continue,
            // 単独CRは通常の字句モードではE0117(L-29)だが、補間の内側では
            // LF・CRLFと同じ「1物理行に収まらない」未終端として扱う(仕様1章L-17(a))。
            // ネストした文字列リテラルが閉じ`"`前にEOFへ達した場合のE0108も、
            // 補間側から見れば対応する`}`を得られない未終端なので同様にE0109へ読み替える
            // (仕様1章L-18改訂: 終端系エラーの統一)。
            Err(e) if e.code == ErrorCode::E0108 || e.code == ErrorCode::E0117 => {
                return Err(unterminated());
            }
            Err(e) => return Err(e),
        };
        if t.kind == TokenKind::Newline {
            // 補間は1物理行に収まらなければならない(仕様1章L-17(a))ので、
            // 内側に生の改行が現れた時点で対応する `}` を得られない未終端として扱う。
            return Err(unterminated());
        }
        match t.kind {
            TokenKind::LParen => expected_closers.push(TokenKind::RParen),
            TokenKind::LBracket => expected_closers.push(TokenKind::RBracket),
            TokenKind::LBrace => expected_closers.push(TokenKind::RBrace),
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                if expected_closers.is_empty() && t.kind == TokenKind::RBrace {
                    return Ok((tokens, t.span.end));
                }
                if expected_closers.last() != Some(&t.kind) {
                    // 対応する開きが無い、または種類が違う閉じ括弧(仕様1章L-18)。
                    return Err(LexError {
                        code: ErrorCode::E0109,
                        span: t.span,
                    });
                }
                expected_closers.pop();
            }
            _ => {}
        }
        tokens.push(t);
    }
    Err(unterminated())
}

/// `\u{H}` エスケープの形式と値を検証する(仕様1章L-15)。`\` の位置 `backslash` と
/// `u` の直後のバイトオフセット `after_u` を受け取り、`u` の次の文字から読み進める。
/// 正しい形(`{`+16進1〜6桁+`}`、値がU+10FFFF以下かつサロゲート域U+D800〜U+DFFF外)なら
/// `}` まで消費して `Ok`。違反はすべてE0112で、spanは `\` から次の位置まで:
/// - `u` の直後が `{` でない → `u` の直後(=`\u` の2バイト)
/// - `{` 以降が16進1〜6桁+`}` でなく、違反確定位置が `}` → その `}` の直後
/// - 上記以外(16進以外の文字・文字列終端・EOF) → 違反文字の手前(=消費済みの末尾)
fn check_unicode_escape(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    backslash: usize,
    after_u: usize,
) -> Result<(), LexError> {
    let error = |end| LexError {
        code: ErrorCode::E0112,
        span: Span {
            start: backslash,
            end,
        },
    };
    let Some(&(brace_open, '{')) = chars.peek() else {
        return Err(error(after_u));
    };
    chars.next();
    // 違反が「閉じ `}` 以外で確定した」ときのspan終端。消費した最後の文字の直後を指す。
    let mut consumed_end = brace_open + '{'.len_utf8();
    let mut digits = String::new();
    while let Some(&(i, h)) = chars.peek() {
        if !h.is_ascii_hexdigit() {
            break;
        }
        digits.push(h);
        consumed_end = i + h.len_utf8();
        chars.next();
    }
    let Some(&(brace_close, '}')) = chars.peek() else {
        return Err(error(consumed_end));
    };
    chars.next();
    let end = brace_close + '}'.len_utf8();
    if digits.is_empty() || digits.len() > 6 {
        return Err(error(end));
    }
    // 6桁以下の16進数はu32に必ず収まる
    let value = u32::from_str_radix(&digits, 16).expect("6桁以下の16進数はu32に収まる");
    if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
        return Err(error(end));
    }
    Ok(())
}

/// 数値リテラルの桁区切り `_` が「数字と数字の間」にあるか検査する(仕様1章L-9)。
/// 左から走査し、直前と直後の両方が数字でない最初の `_` をE0105として報告する。
/// spanはその `_` 1バイトぶん(`_` はASCIIなので1バイト固定)。
///
/// 「数字」の判定を述語 `is_digit` で受けるのは、基数ごとに有効数字の集合が違うため
/// (仕様1章1.6のEBNF: decInt=0-9、hexInt=0-9a-fA-F、binInt=0|1、octInt=0-7)。
/// 10進固定にすると `0xF_F` を誤ってE0105にしてしまうので、L-9を各基数のEBNFに
/// 沿って適用できるようこの形に一般化した。
///
/// `text` は検査対象の字面、`start` はその字面のソース内先頭バイト位置
/// (spanを実位置に直すオフセット)。基数リテラルでは接頭辞 `0x` を除いた本体を渡す
/// (`0` を「直前の数字」と誤認させないため)。
fn check_digit_separators(
    text: &str,
    start: usize,
    is_digit: impl Fn(u8) -> bool,
) -> Result<(), LexError> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'_' {
            continue;
        }
        let prev_is_digit = i > 0 && is_digit(bytes[i - 1]);
        let next_is_digit = bytes.get(i + 1).is_some_and(|&next| is_digit(next));
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

/// 数値リテラルの直後に英数字・`_` が貼り付いていないか検査する(仕様1章L-12の末尾検査)。
/// 数値を読み終えた位置(=`chars` の現在位置)から見て次が `[0-9A-Za-z_]` なら、
/// その連なりを消費して `start..(連なりの終端)` のE0113を返す。連なりが無ければ `Ok(())`。
///
/// 「数字で始まる字面は、途中で数値リテラルとして解釈できなくなっても識別子に化けない」
/// という統一ルールで、これ1つで4つの不正形を捕まえる:
/// `1e`(指数ガードを通らず `e` が残る)・`0x`(基数ガードを通らず10進 `0` の後に `x` が残る)・
/// `0b102`(基数スキャンが `0b10` で止まり `2` が残る)・`123abc`。
/// 指数ガード・基数ガードが「消極判定=条件を満たさなければ1文字も消費しない」設計なのは
/// このためで、ガードを外れた字面が自然にここへ落ちてくる。
///
/// 消費するのは英数字と `_` だけ。`123abc.def` の `.` 以降は消費せず、spanは `123abc` で止まる
/// (エラーは「数値リテラルが壊れている」ことだけを指し、後続の式まで巻き込まない)。
fn check_trailing_alnum(
    source: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    start: usize,
) -> Result<(), LexError> {
    let Some(&(tail_start, first)) = chars.peek() else {
        return Ok(());
    };
    // 1文字目の判定に `_` は要らない: 10進・基数どちらのスキャンも `_` を貪欲に読むため、
    // 数値スキャンが止まった位置の文字が `_` になることはない(内側のscan_whileの `_` は
    // `123a_b` のような連なりの2文字目以降で到達可能なので残す)。
    if !first.is_ascii_alphanumeric() {
        return Ok(());
    }
    let (_, end) = scan_while(source, chars, tail_start, first, |d| {
        d.is_ascii_alphanumeric() || d == '_'
    });
    Err(LexError {
        code: ErrorCode::E0113,
        span: Span { start, end },
    })
}

/// 整数リテラルが安全整数域(絶対値でMAX_SAFE_INT=2^53−1)を超えていないか検査する
/// (仕様1章L-11、ADR-0015の静的検査版)。`text` は数字列本体(桁区切り `_` を含みうる、
/// 基数接頭辞 `0x` 等は含まない)、`radix` はその基数(10進=10、16進=16、2進=2、8進=8)。
///
/// まず `_` を取り除いてから `u128::from_str_radix` で解釈する。u64ではなくu128で
/// 受けるのは、極端に長い桁数のリテラル(例: 100桁の10進数)だとu64の範囲チェックの
/// 前にパース自体がオーバーフローしてしまうため。u128でも収まらない(`Err`)場合は
/// そのまま範囲超過として扱う——u128の上限(約3.4×10^38)自体がMAX_SAFE_INTよりはるかに
/// 大きいので、この`Err`は「桁数が非現実的に多い」ケースのみで発生する。
///
/// spanは呼び出し側が渡すリテラル全体(基数接頭辞を含む)。
fn check_int_overflow(text: &str, span: Span, radix: u32) -> Result<(), LexError> {
    let digits: String = text.chars().filter(|&c| c != '_').collect();
    let out_of_range = match u128::from_str_radix(&digits, radix) {
        Ok(value) => value > MAX_SAFE_INT,
        Err(_) => true,
    };
    if out_of_range {
        return Err(LexError {
            code: ErrorCode::E0107,
            span,
        });
    }
    Ok(())
}

/// floatリテラルがIEEE754倍精度で表現できる大きさに収まっているか検査する
/// (仕様1章L-13、E0114)。`text` は数字列本体(桁区切り `_` を含みうる)。
///
/// `_` を取り除いてから `str::parse::<f64>()` する。floatの字面
/// (`decInt.decInt[exponent] | decInt exponent`)はRustのf64リテラル構文の
/// サブセットなので、字句解析器がここまでFloatと判定した`text`のパースは
/// 必ず成功する。`Err`になるとしたら字句解析器側のバグなので `expect` で落とす。
///
/// 大きさが表現域を超えるとf64は静かに`f64::INFINITY`(または`-INFINITY`)に
/// 丸めてしまう。これを検査なしで通すと、実行時の挙動が字面から想像できない
/// 値(無限大)にすり替わってしまうため、字句解析の時点でE0114として止める
/// ——「静かにInfinityにしない」。
///
/// アンダーフロー(絶対値が小さすぎて0.0に丸まる、例: `1e-999`)はここでは
/// 検査しない。仕様1章L-13が対象とするのは「表現できない大きさ」(絶対値が
/// 大きすぎる方向)の超過のみで、0への丸めは対象外(L-13にその旨を明記済み)。
///
/// spanは呼び出し側が渡すリテラル全体。
fn check_float_overflow(text: &str, span: Span) -> Result<(), LexError> {
    let digits: String = text.chars().filter(|&c| c != '_').collect();
    let value: f64 = digits
        .parse()
        .expect("Float確定済みの字面はf64構文のサブセットなのでパースは必ず成功する");
    if value.is_infinite() {
        return Err(LexError {
            code: ErrorCode::E0114,
            span,
        });
    }
    Ok(())
}

/// 完全予約語22語(仕様1章1.5)を対応するKw*トークン種別に引く表引き関数。
/// 該当しなければ `None`(呼び出し側は誘導用表 `guidance_reserved` → Ident の順に処理する)。
/// 誘導用予約語(`while` `null` 等)は `guidance_reserved` の担当、
/// 文脈キーワード(`component` `state` `view` `as`)は常にIdentが正(L-27)のため、
/// どちらも意図的にこの表へ入れない。
fn keyword_kind(text: &str) -> Option<TokenKind> {
    match text {
        "let" => Some(TokenKind::KwLet),
        "mut" => Some(TokenKind::KwMut),
        "fn" => Some(TokenKind::KwFn),
        "struct" => Some(TokenKind::KwStruct),
        "type" => Some(TokenKind::KwType),
        "if" => Some(TokenKind::KwIf),
        "else" => Some(TokenKind::KwElse),
        "match" => Some(TokenKind::KwMatch),
        "for" => Some(TokenKind::KwFor),
        "in" => Some(TokenKind::KwIn),
        "return" => Some(TokenKind::KwReturn),
        "import" => Some(TokenKind::KwImport),
        "export" => Some(TokenKind::KwExport),
        "or" => Some(TokenKind::KwOr),
        "is" => Some(TokenKind::KwIs),
        "none" => Some(TokenKind::KwNone),
        "error" => Some(TokenKind::KwError),
        "extern" => Some(TokenKind::KwExtern),
        "true" => Some(TokenKind::KwTrue),
        "false" => Some(TokenKind::KwFalse),
        "break" => Some(TokenKind::KwBreak),
        "continue" => Some(TokenKind::KwContinue),
        _ => None,
    }
}

/// 誘導用予約語22語(仕様1章1.5・L-7)かどうかを判定する表引き関数。
/// Meshに無い機能(他言語由来の構文)を指す語で、文法上の正当な出現位置が
/// 存在しないため、識別子として認めず字句段階でE0104として報告する
/// (呼び出し側で `keyword_kind` に該当しなかった語にのみ適用する)。
fn guidance_reserved(text: &str) -> bool {
    matches!(
        text,
        "while"
            | "class"
            | "null"
            | "undefined"
            | "enum"
            | "async"
            | "await"
            | "try"
            | "catch"
            | "throw"
            | "var"
            | "const"
            | "function"
            | "switch"
            | "case"
            | "do"
            | "interface"
            | "new"
            | "this"
            | "typeof"
            | "instanceof"
            | "defer"
    )
}

/// 1文字の区切り記号7種(仕様1章1.9: `( ) [ ] { } ,`)を対応する`TokenKind`に写す。
/// 該当しない文字は`None`(呼び出し側の他分岐に処理を委ねる)。
fn punctuation_kind(c: char) -> Option<TokenKind> {
    match c {
        '(' => Some(TokenKind::LParen),
        ')' => Some(TokenKind::RParen),
        '[' => Some(TokenKind::LBracket),
        ']' => Some(TokenKind::RBracket),
        '{' => Some(TokenKind::LBrace),
        '}' => Some(TokenKind::RBrace),
        ',' => Some(TokenKind::Comma),
        _ => None,
    }
}

/// 1文字の演算子13種(仕様1章1.9: `+ - * / % < > ! ? . | : =`)を対応する`TokenKind`に写す。
/// 該当しない文字は`None`(呼び出し側の他分岐に処理を委ねる)。
/// `/` を含むため、コメント(`//`・`/*`)の分岐を通過した後にだけ引くこと。
/// この表は1文字ぶんのみを見る「切り落とし」側で、2文字演算子の優先は
/// 呼び出し側が `two_char_operator_kind` を先に引くことで担保する(仕様1章L-2の最長一致)。
fn operator_kind(c: char) -> Option<TokenKind> {
    match c {
        '+' => Some(TokenKind::Plus),
        '-' => Some(TokenKind::Minus),
        '*' => Some(TokenKind::Star),
        '/' => Some(TokenKind::Slash),
        '%' => Some(TokenKind::Percent),
        '<' => Some(TokenKind::Lt),
        '>' => Some(TokenKind::Gt),
        '!' => Some(TokenKind::Bang),
        '?' => Some(TokenKind::Question),
        '.' => Some(TokenKind::Dot),
        '|' => Some(TokenKind::Pipe),
        ':' => Some(TokenKind::Colon),
        '=' => Some(TokenKind::Eq),
        _ => None,
    }
}

/// 2文字の演算子(仕様1章1.9)を対応する`TokenKind`に写す表引き関数。
/// 1文字目 `first` と、その直後の文字 `second` を受け取り、2文字で1トークンになる
/// 組み合わせだけを`Some`で返す。該当しない組は`None`で、呼び出し側は1文字表へ落ちる
/// (仕様1章L-2の最長一致を「2文字を先に引き、外れたら1文字」の順序で実現する)。
/// `..` は数値リテラル側では読まない(L-3)ため、`0..10` の分割もこの表だけで成立する。
/// 全13種: `<=` `==` `..` `!=` `>=` `&&` `||` `+=` `-=` `*=` `/=` `%=` `=>`。
fn two_char_operator_kind(first: char, second: char) -> Option<TokenKind> {
    match (first, second) {
        ('<', '=') => Some(TokenKind::LtEq),
        ('=', '=') => Some(TokenKind::EqEq),
        ('.', '.') => Some(TokenKind::DotDot),
        ('!', '=') => Some(TokenKind::BangEq),
        ('>', '=') => Some(TokenKind::GtEq),
        ('&', '&') => Some(TokenKind::AmpAmp),
        ('|', '|') => Some(TokenKind::PipePipe),
        ('+', '=') => Some(TokenKind::PlusEq),
        ('-', '=') => Some(TokenKind::MinusEq),
        ('*', '=') => Some(TokenKind::StarEq),
        ('/', '=') => Some(TokenKind::SlashEq),
        ('%', '=') => Some(TokenKind::PercentEq),
        ('=', '>') => Some(TokenKind::FatArrow),
        _ => None,
    }
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

/// バイトオフセット `offset` にある文字を**イテレータを消費せずに**返す(非消費先読みの共通部品)。
/// `Peekable` は1文字先までしか覗けないため、「1文字目を消費する前に2文字目を見る」
/// 最長一致(仕様1章L-2)の判定はソース文字列側のスライスで行う。
/// `offset` は文字境界であること。呼び出し側は「`char_indices` 由来の文字境界 `start`」+
/// 「その位置の実文字のバイト長 `len_utf8()`」の和を渡すため、`c` が非ASCII
/// (日本語・絵文字等)でも常に成立する(`start + 1` のようなASCII前提の固定値は不可)。
/// EOFに達しているときは `None`。
fn peek_at(source: &str, offset: usize) -> Option<char> {
    source[offset..].chars().next()
}
