# 仕様 4章 — 文と制御構造(Statements)

ブロック・宣言・代入・if文・for・break/continue・returnを定める。根拠ADR: [0008](../adr/0008-bindings-let-mut.md)(let/mut・シャドーイング禁止)/ [0009](../adr/0009-if-as-expression.md)(if)/ [0019](../adr/0019-for-three-forms.md)(forの3形)/ [0025](../adr/0025-match-arm-body-always-block.md)(ブロック)/ [0033](../adr/0033-value-semantics.md)(値意味論)/ [0035](../adr/0035-statement-details.md)(パラメータ不変・効果なし式文・未使用変数・deferなし)。

エラーコードは `E04xx`。負例のテストIDは `tests/04-statements/` 配下。規則番号(S-n)は安定ID。

本章は3章からの申し送り2件(continue/breakへのnarrowing拡張=S-17、ヘッダ位置structリテラルの括弧必須=S-15)を消化する。

## 4.1 ブロックと文

```ebnf
block        = "{" { stmt } "}"
stmt         = letDecl | assignStmt | exprStmt | ifForm | forStmt
             | breakStmt | continueStmt | returnStmt
exprStmt     = expr
ifForm       = "if" expr block [ "else" ( block | ifForm ) ]
breakStmt    = "break"
continueStmt = "continue"
returnStmt   = "return" [ expr ]
```

if/matchの構文木は文と式で共通であり(`ifForm` は3章 `ifExpr` と同一。elseの必須性は値位置の検査=E0311で課す)、「値として扱われるか」はX-12(値位置)だけで決まる。

- **S-1**: ブロックが値位置(X-12)にあるとき、ブロックの値は**最終文の式**の値であること。最終文が**発散文**(`return`・`continue`・`break`)の場合、そのブロックは値の要求から除外される(if/matchの他の腕が型を決める。X-13)。最終文が式でも発散文でもない(let・代入で終わる)場合、および空ブロックの場合はエラー E0408 とし、値を返す式で終えるよう案内すること。〔負例: `block-no-tail-expr`(let終わり・空ブロックの2形)〕
- **S-2**(効果のない式文): 式文の式は「呼び出しを含む後置式・match式・if式・`?` または or を含む式」のいずれかであること。それ以外(`x + 1` 単独、リテラル単独など)はエラー E0401「値が使われていません。代入・return・`let _ =` のいずれかの書き忘れでは?」。**値位置ブロックの最終文はこの規則の対象外**(それは値そのもの)。〔負例: `useless-expression` / 正例: `expression-statement-ok`(`f()` 単独文)〕

## 4.2 let / mut 宣言

```ebnf
letDecl = ( "let" | "mut" ) ( identifier | "_" ) [ ":" type ] "=" expr
```

- **S-3**: 宣言は初期化と同時であること(未初期化宣言は文法上存在しない)。型注釈があれば期待型として式へ伝播する(T-13)。
- **S-4**(シャドーイング禁止。ADR-0008): 可視な既存の値の名前(外側のブロックのlet/mut・パラメータ・トップレベル宣言・ループ変数)と同名の宣言はエラー E0402 とし、リネームを提案すること。ネストしたブロックでも禁止。matchパターン束縛の同違反はE0317(3章X-18。章別コード空間のため番号が異なるが同一規則)。**値と型は別の名前空間**であり、`struct User` と `let user` は共存できる。〔負例: `shadowing`(ネストブロック・パラメータの2形)〕
- **S-5**: `let _ = f()` は「値の明示的な破棄」であり、束縛を作らない。`mut _` はエラー E0403 のメッセージ変種(意味が無い)。〔正例: `discard` / 負例: `mut-underscore`〕
- **S-20**(`_` の位置): `_` を書けるのは**束縛位置**(let/mutの左辺・forのループ変数・matchパターン・orの束縛)のみであること。式・代入対象の位置の `_`(`let x = _`、`_ = f()`)はエラー E0412「`_` は値を持ちません。破棄は `let _ =` で」。ただし `x or _`(`=>` 忘れ)はX-28の専用メッセージ(E0325)が優先する。〔負例: `underscore-as-value`〕
- **S-6**(未使用変数): 宣言した変数(ループ変数を含む)が一度も**読まれない**とき、エラー E0403「未使用です。不要なら削除、意図的な破棄なら `_`」を報告すること(ADR-0035)。書き込み(再代入)のみで読まれない変数も未使用である(Goと同じ)。〔負例: `unused-variable`(再代入のみの変数を含む)〕

## 4.3 代入と複合代入

```ebnf
assignStmt = target ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" ) expr
target     = identifier { "." identifier | "[" expr "]" }
```

- **S-7**(代入対象): 代入の対象は `mut` で宣言した変数(またはそのフィールド・添字のパス)のみであること。`let` 変数への代入はエラー E0404 とし `mut` 化を提案、**パラメータへの代入・ループ変数への代入はE0404のメッセージ変種**とし `mut local = param` 等を案内すること(ADR-0035)。代入は文であり式ではない。式の中に現れた `=` は**パーサ**がエラー E0413「代入は式ではありません。比較は `==` です」として報告すること(`=` は正規トークンのため字句層では検出できない)。〔負例: `assign-to-let`、`assign-to-param`、`loop-var-assign`、`assign-as-expression`(`if x = 1 { }`)〕
- **S-8**(複合代入。設計監査・軽微項目の消化): `t op= e` は `t = t op e` と同じ意味であること。ただし `t` の部分式(添字のインデックス等)は**1回だけ評価**される(`xs[next()] += 1` の `next()` は1回)。演算子の型規則はX-4/X-6に従う(`s += "x"` の文字列連結も可)。〔正例: `compound-assign`(添字1回評価の挙動検証を含む)〕
- **S-9**(値意味論との関係): フィールド・添字への代入は「そのmut変数が保持する値の該当部分の更新」であり、他の変数へ影響しない(X-1)。**list添字代入**の範囲外はpanic(T-16)。**string添字への代入は不可**(stringは不変。E0404のメッセージ変種で連結・スライスを案内)。〔挙動検証: `field-assign-isolation` / 負例: `string-index-assign`〕
- **S-21**(map添字代入): `m[k] = v` は、キーが**無ければ挿入・あれば上書き**であること(書き込みは常に成功する。読み取りが `V | none` なのと非対称=T-16の「不在はデータ」)。この帰結として、**mapへの複合代入 `m[k] += 1` はコンパイルエラー**(S-8の展開で読み側が `V | none` となりE0320)。エラーメッセージでイディオム `m[k] = (m[k] or 0) + 1` を案内すること。〔正例: `map-insert-assign` / 負例: `map-compound-assign`〕

## 4.4 if文

- **S-10**: 文の位置のifは `else` を省略できる(値位置での省略はE0311=X-12)。`else if` の連鎖を許す。`else` は前の腕の `}` と**同一行**に書くこと(1章L-25の帰結。改行するとエラー E0411「`else` は `}` と同じ行に書いてください」)。〔負例: `else-on-new-line`(設計監査#13の消化)〕

## 4.5 for文

```ebnf
forStmt    = "for" ( block | forIn | expr block )
forIn      = ( identifier | "_" ) "in" expr [ ".." expr ] block
```

- **S-19**(3形の判別): `for` の直後が `{` なら無限ループ形。先頭2トークンが「識別子(または `_`)+ `in`」ならfor-in形。それ以外は条件形として式をパースする(2トークン先読みで決定的)。for-inの対象は式を1つパースし、直後が `..` ならもう1つパースして範囲形とする(`..` は式の演算子ではない=X-27ため、後読みで一意)。〔正例テスト: `for-forms`(3形すべて+`for _ in 0..n` のn回繰り返し)〕
- **S-11**(for-in): `for x in e` の `e` は `list<T>`(要素 `T`)または `string`(要素は1コードポイントのstring=T-15)であること。union型の対象はX-23(E0320)が優先する(絞り込みを案内)。その他の型はエラー E0406(mapは `keys(m)` 等の標準関数を案内=11章)。反復順はlistの並び順・stringのコードポイント順。〔負例: `for-in-non-iterable`〕
- **S-12**(範囲): `for i in a..b` の `a`・`b` は `int` であること(それ以外はエラー E0407)。半開区間 `a <= i < b` を昇順に反復し、`a >= b` なら0回(両端が定数で `a >= b` の場合はlint警告の対象=12章。逆順は将来の `range` 標準関数=ADR-0019)。〔正例: `for-range`(0回反復含む)/ 負例: `range-non-int`〕
- **S-22**(ヘッダの評価タイミング): for-inの対象式・範囲の両端は**ループ開始時に1回だけ**評価され、その値のスナップショットを反復すること(値意味論X-1。ループ内で元の変数を変更しても反復には影響しない)。条件形の条件式は**毎反復**評価される。〔挙動検証: `for-eval-once`〕
- **S-13**: ループ変数は**各反復ごとの不変束縛**であること(再代入はE0404変種=S-7、シャドーイング禁止はS-4、未使用はS-6の対象で `_` により回避)。クロージャによる捕捉の意味論は5章。
- **S-14**: 条件形 `for cond { }` の条件は `bool`(X-10。truthyなし)。`for { }` は無限ループ(脱出はbreak/return)。〔負例: `for-condition-non-bool` / 正例: `infinite-loop-break`〕
- **S-15**(ヘッダ位置のstructリテラル。設計監査#11の消化): if・forの条件、matchのscrutinee、for-inの対象の各**ヘッダ位置**では、structリテラルを括弧で囲まずに書けないこと(`{` は本体の開始と解釈する)。検出条件: ヘッダ式が「型名(±型引数)への参照」単独に解決し、直後にブロックが続くとき、エラー E0405「ヘッダ位置のstructリテラルは括弧で囲んでください: `(Ready{})`」を**他の診断に優先して**報告すること。〔負例: `header-struct-literal`(if/for/matchの3形)〕

## 4.6 break / continue

- **S-16**: `break`(最内ループを脱出)と `continue`(最内ループの次の反復へ)は、**同一関数内の**ループ本体の中でのみ書けること。無名関数の本体境界を越えられない(`for ... { each(ys, fn(y) { break }) }` の `break` はエラー E0409)。ループ外もE0409。ラベル付きbreakは無い(多重脱出は関数化で書く)。〔負例: `break-outside-loop`、`break-across-fn`〕
- **S-17**(narrowingの引き継ぎ。3章X-21の拡張): ifの片側の腕の**最終文**が `continue` または `break` のとき、X-21のreturnの規則と同様に、if文以降(continueなら同一反復内の以降、breakならループ後は対象外)の変数の型は**通過する側の腕で成立している型**であること。

```
for item in items {          // items: list<int | none>
  if item is none { continue }
  use(item)                  // ここで item: int
}
```

〔正例テスト: `narrowing-continue`〕

## 4.7 return

- **S-18**: `return [expr]` は関数本体の中でのみ書けること(トップレベルはエラー E0410)。値なし `return` は `return none` の糖衣(T-26と整合)。**無名関数の中のreturnはその無名関数からのreturn**(外側の関数ではない。型検査・関数本体末尾の扱いとあわせて5章が定める)。〔負例: `return-top-level`〕

## conformance対応表

| テストID | 種別 | 規則 |
|---|---|---|
| block-no-tail-expr | 負例 | S-1 |
| useless-expression | 負例 | S-2 |
| expression-statement-ok | 正例 | S-2 |
| shadowing | 負例 | S-4 |
| discard | 正例 | S-5 |
| mut-underscore | 負例 | S-5 |
| underscore-as-value | 負例 | S-20 |
| unused-variable | 負例 | S-6 |
| assign-to-let / assign-to-param / loop-var-assign / assign-as-expression | 負例 | S-7 |
| compound-assign | 正例 | S-8 |
| field-assign-isolation | 挙動検証 | S-9 |
| string-index-assign | 負例 | S-9 |
| map-insert-assign | 正例 | S-21 |
| map-compound-assign | 負例 | S-21 |
| else-on-new-line | 負例 | S-10 |
| for-forms | 正例 | S-19 |
| for-in-non-iterable | 負例 | S-11 |
| for-range | 正例 | S-12 |
| range-non-int | 負例 | S-12 |
| for-eval-once | 挙動検証 | S-22 |
| for-condition-non-bool | 負例 | S-14 |
| infinite-loop-break | 正例 | S-14 |
| header-struct-literal | 負例 | S-15 |
| break-outside-loop / break-across-fn | 負例 | S-16 |
| narrowing-continue | 正例 | S-17 |
| return-top-level | 負例 | S-18 |
