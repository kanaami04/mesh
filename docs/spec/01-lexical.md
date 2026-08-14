# 仕様 1章 — 字句(Lexical Structure)

ソースコードの文字列をトークン(意味の最小単位)に分割する規則を定める。根拠ADR: [0010](../adr/0010-newline-statement-separator.md)(改行区切り)/ [0015](../adr/0015-numbers-int53-float.md)(数値)/ [0018](../adr/0018-string-interpolation.md)(補間)/ [0024](../adr/0024-comments-and-naming.md)(コメント・命名)/ [0026](../adr/0026-line-continuation-binary-only.md)(行継続)/ [0030](../adr/0030-lexical-details.md)(識別子・数値記法・単一行文字列)/ [0031](../adr/0031-continuation-list-bracket-depth-contextual-keywords.md)(継続一覧・括弧深度・文脈キーワード)。

エラーコードは `E01xx` を字句エラーに割り当てる。負例にはconformanceテストID(`tests/01-lexical/` 配下)を併記する。

## 1.1 ソースファイルと適用範囲

- **L-1**: ソースファイルがUTF-8として不正なとき、コンパイラは位置つきでエラー E0101 を報告すること。〔負例: `invalid-utf8`〕
- 拡張子は `.msb`。
- この章の規則は、`component` の `view` ブロック内部(タグ記法の領域)を**除く**全ソースに適用する。viewブロック内部は9章が定める別の字句モードに切り替わる。

## 1.2 トークン分割

```ebnf
token = keyword | identifier | number | string | operator | punctuation
```

- **L-2**: 字句解析は**最長一致**でトークンを切り出すこと。〔正例テスト: `longest-match`〕
- **L-3**(`..` の消極規則): 数値リテラル読み取り中の `.` は、直後がもう1つの `.` のとき数値に**含めない**こと。したがって `0..10` は `0` `..` `10` に分割される。
- 空白(スペース・タブ)はトークンの区切りで、それ自体に意味は無い。改行の扱いは1.8。

## 1.3 コメント

- **L-4**: `//` から行末まではコメントであり、トークンを生成しないこと。行末トークンの判定(1.8)は**コメントを除去した後**の行末に対して行うこと。
- **L-5**: `/*` が現れたとき、エラー E0102「ブロックコメントはありません。`//` を使ってください」を報告すること。〔負例: `block-comment`〕
- `///` はドキュメンテーションコメントとして予約する(v1の扱いは `//` と同じ)。

## 1.4 識別子

```ebnf
identifier = ( letter | "_" ) { letter | digit | "_" }
letter     = "a"…"z" | "A"…"Z"
```

- **L-6**: 識別子はASCIIのみ。ASCII外の文字が識別子位置に現れたとき、エラー E0103 を報告し、英数字名への変更を促すこと。文字列リテラルとコメントの日本語は制限しない。〔負例: `non-ascii-ident`(`let 合計 = 0`)〕
- `_1` や `_tmp` は正規の識別子である(先頭 `_` は許される)。`_` **単体**のみ「値を捨てる」専用で、束縛を作らない(4章)。
- 命名ケース(変数・関数=camelCase、型=PascalCase)は**lintの担当**であり、コンパイルエラーにはしない(ADR-0024)。

## 1.5 予約語

**完全予約語**(識別子に使えない):

```
let mut fn struct type if else match for in return
import export or is none error extern true false break continue
```

**文脈キーワード**(ADR-0031。`component` 宣言の文法内でのみ予約。通常コードでは識別子として使える):

```
component state view
```

**誘導用予約語**(Musubiに無い機能。他言語ユーザー・AIの誤用を明快なエラーにするため予約):

```
while class null undefined enum async await try catch throw
var const function switch case do interface new this typeof instanceof
```

- **L-7**: 完全予約語・誘導用予約語が識別子位置に現れたとき、エラー E0104 を報告すること。誘導用は代替を案内すること(`while` →「条件ループは `for 条件 { }`」、`null`/`undefined` →「不在は `T | none`」、`new` →「structは `User{...}` で生成します」、`this` →「レシーバ名(`fn (u: User)` の `u`)を使います」)。〔負例: `reserved-while`、`reserved-null`、`reserved-new`〕
- **L-8**: `or` の束縛位置に `error` が書かれたとき(`or error => { ... }`)、専用の案内「`error` は予約語です。`e` など別名で束縛してください」を出すこと(エラーコードは E0104 のメッセージ変種)。〔負例: `or-bind-error-name`〕
- **L-27**: 文脈キーワードは字句解析では常に**識別子トークン**として出力されること。予約の判定はパーサが `component` 文法の内部でのみ行う。したがって `let state = loadState()` は通常コードで合法。〔正例テスト: `contextual-keyword-ident`〕
- 誘導用予約語の採録基準: JS/TS/Python由来の高頻度誤用語。白紙AI実験(Phase 9)の実測で増減を見直す。

## 1.6 数値リテラル

```ebnf
number    = int | float
int       = decInt | "0x" hexInt | "0b" binInt | "0o" octInt
decInt    = digit { digit | "_" digit }
hexInt    = hexDigit { hexDigit | "_" hexDigit }
binInt    = binDigit { binDigit | "_" binDigit }
octInt    = octDigit { octDigit | "_" octDigit }
float     = decInt "." decInt [ exponent ] | decInt exponent
exponent  = ( "e" | "E" ) [ "+" | "-" ] decInt
digit     = "0"…"9"        hexDigit = digit | "a"…"f" | "A"…"F"
binDigit  = "0" | "1"      octDigit = "0"…"7"
```

※ このEBNFに一致しても診断規則が優先する形がある(先頭ゼロの10進 `0755` はL-12のエラー)。

正例: `42` / `1_000_000` / `0xFF` / `0b1010` / `0o755` / `3.14` / `0.5` / `1e6` / `1E6` / `2.5e-3` / `1e+6`

- **L-9**: 桁区切り `_` は数字と数字の間にのみ書けること。先頭(基数接頭辞直後を含む)・末尾・連続はエラー E0105。指数部にも同じ規則を適用する。〔負例: `underscore-edge`(`1__0`、`1_`、`0x_FF`)〕
- **L-10**: floatの小数点の**両側には数字が必須**であること。`0.` と `.5` はエラー E0106 とし、`0.0` / `0.5` への修正候補を提示すること(設計監査#12)。〔負例: `float-dot-edge`〕
- **L-11**: 整数リテラルが安全整数域(±2^53−1)を超えるとき、エラー E0107 を報告すること(ADR-0015の静的検査版)。〔負例: `int-literal-overflow`〕
- **L-12**: 次の形はエラー E0113 とし、それぞれ修正候補を提示すること: 指数部が空(`1e`)/ 基数接頭辞の後に数字が無い(`0x`)/ 基数外の数字(`0b102`。最長一致で `0b10`+`2` に割らず、リテラル全体の不正として診断する)/ 先頭ゼロの10進(`0755`。C系8進の誤解防止として `0o755` か `755` へ誘導)/ 数字の直後に識別子文字(`123abc`)。〔負例: `number-malformed`〕
- **L-13**: floatリテラルがIEEE754倍精度で表現できない大きさ(`1e999` 等)のとき、エラー E0114 を報告すること(静かにInfinityにしない)。〔負例: `float-literal-overflow`〕
- 16進の小数(`0x1.8`)は存在しない(`0x` 系はintのみ)。

## 1.7 文字列リテラル

- `"..."` の**単一行のみ**。
- エスケープ: `\n` `\t` `\r` `\\` `\"` `\$` `\u{H}`(Hは1〜6桁の16進)。
- **L-14**: 一覧に無いエスケープ(`\q` 等)はエラー E0111 とし、近い正解があれば案内すること。〔負例: `invalid-escape`〕
- **L-15**: `\u{H}` の H が16進1〜6桁でない・U+10FFFFを超える・サロゲート域(U+D800〜U+DFFF)のとき、エラー E0112 を報告すること。〔負例: `unicode-escape-range`〕
- **L-16**: リテラル中に生の改行が現れた、または閉じ `"` の前にファイルが終わったとき、エラー E0108 を報告し `\n` または閉じ `"` を案内すること。〔負例: `string-raw-newline`、`string-unterminated-eof`〕
- **L-17**(補間): `${` から対応する `}` までは**通常の字句モードで再帰的に**トークン化すること。「対応する `}`」は、再帰トークン化の結果の**トークン列上で**括弧類(`( ) [ ] { }`)の対応を数えて決める(ネストした文字列リテラルやエスケープの中の括弧「文字」は数えない)。補間内にはネストした文字列リテラルを書ける。ただし: (a) 補間を含む文字列全体は物理1行に収まること、(b) 補間内にコメントは書けない(`//` はエラー E0115)。〔正例テスト: `interpolation-nested`(`"${f("x")}"`、`"${f("(")}"`、`"${m[k] or 0}"`)/ 負例: `interpolation-comment`〕
- **L-18**: `${` が対応する `}` を得ないままリテラル・行が終わる、または補間内の括弧が不均衡なまま終わるとき、エラー E0109 を報告すること。**補間の内側ではE0109がL-16(E0108)より優先する**。〔負例: `unterminated-interpolation`〕
- **L-28**: 直後が `{` でない `$` はただの文字であること(`"$5"` は合法)。リテラルとして `${` と書きたいときだけ `\$` を使う。〔正例テスト: `dollar-literal`〕

```
let msg = "こんにちは、${u.name}さん\n"
let price = "$5"                      // 合法。補間ではない
let quote = "値は \${price} で参照"    // リテラルの ${
```

## 1.8 文の終端と行継続(ADR-0010/0026/0031)

- **L-19**: 改行は原則として文を終端すること。セミコロンは存在しない(`;` はエラー E0110「文末セミコロンは不要です」)。〔負例: `semicolon`〕
- **L-20**(継続トークン): 行末のトークン(コメント除去後)が次のいずれかのとき、文は次の行へ継続すること:
  - 二項演算子: `+ - * / % == != < <= > >= && || |`
  - キーワード演算子: `or` `is` `in`
  - 複合代入: `+= -= *= /= %=`
  - `.`(メンバーアクセス)・`..`(範囲)・カンマ・開き括弧 `( [ {`・`=`・`=>`
  
  メソッドチェーンの折り返しは `builder.` のように行末ドットで行う(整形はfmtが統一)。〔正例テスト: `continuation-operators`、`continuation-method-chain`〕
- **L-21**(括弧深度): `(` または `[` の内側では、改行は文を終端しないこと。ただし**その内側でも `{ ... }` ブロックに入ったら終端規則が復活する**こと(深度は `{` で退避し、対応する `}` で復元するスタック構造。引数中に無名関数の本体を書くケースのため)。〔正例テスト: `multiline-call` — トレーリングカンマ無しの複数行呼び出しに加え、**引数の無名fn本体に複数文を含む**形を必ず含める〕
- **L-22**(複数行structリテラル): `{` は深度の対象にしないため、複数行のstructリテラルは各フィールドの行末カンマで継続すること。**最終フィールドにもカンマが必須**(トレーリングカンマ。fmtが自動補完する)。〔正例: `multiline-struct-literal` / 負例: `struct-literal-missing-trailing-comma`〕
- **L-23**: 行頭に演算子を置いても継続は起きないこと(継続の判定は前行の行末のみ)。この規則が意味を持つのは括弧深度0の位置である(括弧内では改行が終端しないため、`f(a` の次行に `- b)` と書くとカンマ忘れが単一式 `a - b` として**静かに通る**。この形はlintの検出対象とする=12章)。〔負例: `no-leading-operator-continuation`(`1` の次の行に `+ 2`)〕
- **L-24**: 後置 `?` は文を終端**できる**こと(継続を誘発しない)。帰結として `? "文脈"` は `?` と同一行必須。〔挙動検証(正例): `postfix-question-newline` — 下記2行目が独立文としてパース・実行されることを確認〕

```
let user = findUser(id)?
"保存しました"              // ← 前行の ? "文脈" に飲み込まれてはならない
```

- **L-25**: `}` は文を終端すること(`else` は `}` と同一行に書く。詳細は4章)。〔負例テストは4章で定義〕

## 1.9 演算子・区切り記号のトークン一覧

```
+  -  *  /  %  ==  !=  <  <=  >  >=  &&  ||  !
=  +=  -=  *=  /=  %=  =>  ?  ..  .  ,  |  :  ( ) [ ] { }
```

各トークンの意味・優先順位は3章で定義する(複合代入の意味論も3章)。

- **L-26**(キャッチオール): この一覧・本章のどの規則にも該当しない文字・記号列はエラー E0116 とし、近い正解があれば案内すること: `===` →「等価比較は `==`」/ `->` →「戻り値型は空白のみ・matchの腕は `=>`」/ `` ` `` →「文字列は `"`(`"` でも `${}` 補間が使えます)」/ `'` →「文字列は `"`」。`;` はL-19が優先する。〔負例: `triple-equals`、`backtick-string`、`single-quote-string`〕

## conformance対応表

| テストID | 種別 | 規則 |
|---|---|---|
| invalid-utf8 | 負例 | L-1 |
| longest-match | 正例 | L-2/L-3 |
| block-comment | 負例 | L-5 |
| non-ascii-ident | 負例 | L-6 |
| reserved-while / reserved-null / reserved-new | 負例 | L-7 |
| or-bind-error-name | 負例 | L-8 |
| contextual-keyword-ident | 正例 | L-27 |
| underscore-edge | 負例 | L-9 |
| float-dot-edge | 負例 | L-10 |
| int-literal-overflow | 負例 | L-11 |
| number-malformed | 負例 | L-12 |
| float-literal-overflow | 負例 | L-13 |
| invalid-escape | 負例 | L-14 |
| unicode-escape-range | 負例 | L-15 |
| string-raw-newline / string-unterminated-eof | 負例 | L-16 |
| interpolation-nested | 正例 | L-17 |
| interpolation-comment | 負例 | L-17 |
| unterminated-interpolation | 負例 | L-18 |
| dollar-literal | 正例 | L-28 |
| semicolon | 負例 | L-19 |
| continuation-operators / continuation-method-chain | 正例 | L-20 |
| multiline-call(無名fn本体の複数文を含む) | 正例 | L-21 |
| multiline-struct-literal | 正例 | L-22 |
| struct-literal-missing-trailing-comma | 負例 | L-22 |
| no-leading-operator-continuation | 負例 | L-23 |
| postfix-question-newline | 挙動検証 | L-24 |
| triple-equals / backtick-string / single-quote-string | 負例 | L-26 |
