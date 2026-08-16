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
    /// 文字列リテラル中の生の改行・閉じ`"`前のEOF(仕様1章L-16)。
    E0108,
    /// 補間 `${` が対応する `}` を得ないまま終わった(仕様1章L-18)。
    E0109,
    /// 一覧に無いエスケープ(仕様1章L-14)。
    E0111,
    /// `\u{H}` の範囲・形式違反(仕様1章L-15)。
    E0112,
    /// 補間の内側のコメント `//`(仕様1章L-17(b))。
    E0115,
    /// どの字句規則にも該当しない文字(仕様1章L-26キャッチオール)。
    /// 注意: 固有の規則を持つが未実装の文字(`;`=E0110、非ASCII識別子=E0103)も
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

/// ソース文字列を字句解析してトークン列を返す。
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while chars.peek().is_some() {
        if let Some(t) = next_token(source, &mut chars, false, 0)? {
            tokens.push(t);
        }
    }
    Ok(tokens)
}

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
        let (text, end) = scan_while(source, chars, start, c, |d| d.is_ascii_digit() || d == '_');
        check_digit_separators(text, start)?;
        Ok(Some(token(TokenKind::Int, text, Span { start, end })))
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
