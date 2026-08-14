# 仕様 3章 — 式と演算子(Expressions)

式の文法・演算子の型規則・if式/match式・narrowing・or/`?` を定める。根拠ADR: [0005](../adr/0005-absence-and-failure-as-union.md)(narrowing・match・`?`・or)/ [0009](../adr/0009-if-as-expression.md)(if式)/ [0015](../adr/0015-numbers-int53-float.md)(数値演算)/ [0019](../adr/0019-for-three-forms.md)(範囲)/ [0025](../adr/0025-match-arm-body-always-block.md)(腕はブロック)/ [0028](../adr/0028-or-binding-block-form.md)(or束縛形)/ [0033](../adr/0033-value-semantics.md)(値意味論・等価)/ [0034](../adr/0034-expression-details.md)(matchパターン範囲・範囲式・narrowing対象)。

エラーコードは `E03xx`。負例のテストIDは `tests/03-expressions/` 配下。規則番号(X-n)は安定ID。

## 3.1 値の意味論(ADR-0033)

- **X-1**: 複合値(struct・list・map)は代入・引数渡し・returnで**独立した値**になること。別名経由の変更(`mut b = a` 後の変更が `a` に及ぶこと)は観測できない。実装はコピーの省略最適化を行ってよいが、意味論はコピー。〔挙動検証: `value-semantics`(struct・list・mapの3種で独立性を確認)〕

## 3.2 式の文法と優先順位

```ebnf
expr      = orExpr
orExpr    = logicOr { "or" ( logicOr | binding ) }      (* 左結合。直後の判別は下記 *)
binding   = ( identifier | "_" ) "=>" block             (* blockは4章 *)
logicOr   = logicAnd { "||" logicAnd }
logicAnd  = equality { "&&" equality }
equality  = relational [ ( "==" | "!=" ) relational ]
relational= additive [ ( "<" | "<=" | ">" | ">=" ) additive | "is" memberType ]
additive  = multiplicative { ( "+" | "-" ) multiplicative }
multiplicative = unary { ( "*" | "/" | "%" ) unary }
unary     = ( "!" | "-" ) unary | postfix
postfix   = primary { call | index | field | "?" } [ "?" stringLit ]
            (* 素の ? は連鎖の途中に書ける(f()?.name)。? の直後がstringLitなら文脈付き伝播で、これは連鎖の終端のみ *)
call      = [ typeArgs ] "(" [ expr { "," expr } ] ")"
typeArgs  = "<" type { "," type } ">"                    (* typeは2章 *)
index     = "[" expr "]"
field     = "." identifier
primary   = literal | identifier | structLit | listLit | ifExpr | matchExpr | fnExpr | "(" expr ")"
structLit = identifier [ typeArgs ] "{" [ fieldInit { "," fieldInit } [ "," ] ] "}"
fieldInit = identifier ":" expr
listLit   = "[" [ expr { "," expr } [ "," ] ] "]"
memberType= identifier [ typeArgs ] | "none" | "error"   (* isとmatchパターンの型。単一メンバーのみ *)
```

`fnExpr`(無名関数)は5章、`block` は4章、`stringLit`・数値等のリテラル字句は1章で定める。

- **X-2**: 優先順位は上のEBNFのとおり(低い順: `or` → `||` → `&&` → `==`/`!=` → 比較/`is` → `+`/`-` → `*`/`/`/`%` → 単項 → 後置)。`f() or g() + 1` は `f() or (g() + 1)`、`a or b or c` は `(a or b) or c`(左結合。連鎖の型はX-24)。〔正例テスト: `precedence`〕
- **X-3**: `==`/`!=` と比較演算子は**連鎖できない**こと(`a < b < c` はエラー E0301 とし、`a < b && b < c` を案内)。〔負例: `comparison-chain`〕
- **X-27**: 範囲式 `a..b` は**式ではない**(forヘッダ専用の構文=4章。ADR-0034)。式の位置に `..` が現れたらエラー E0302 を報告すること。〔負例: `range-outside-for`〕
- **X-28**(orの構文判別): `or` の直後は、トークン列が「識別子または `_`、続いて `=>`」なら束縛形、それ以外はフォールバック式と解釈すること(2トークン先読みで決定的)。`x or _`(`=>` なし)はエラー E0325「`_` は値ではありません。`or _ => { ... }` の `=>` 忘れでは?」。〔負例: `or-underscore-no-arrow`〕
- **X-29**(ジェネリック呼び出しの判別): `識別子 <` に続くトークン列が型引数リストとしてパース可能で、閉じ `>` の直後が `(` のとき、**常にジェネリック呼び出しと解釈する**こと(比較としての合法な読みが存在する場合でも。C#と同じ解決)。それ以外は比較演算子と解釈する。試行パースは型文法(2章)がトークン列長で有界のため停止する。比較の意図で書くときは括弧を使う(`f((a < b), (c > (d)))`)。〔正例テスト: `generic-call-disambiguation`(`f<int>(xs)` と `a < b`)/ 挙動検証: `generic-call-ambiguous-comma`(`f(a < b, c > (d))` がジェネリック呼び出しに固定されること)〕

## 3.3 演算子の型規則

- **X-4**(二項算術): `+ - * / %` のオペランドは数値であること(`+` は文字列連結にも使える=X-6)。`int × int → int`、`float` が混ざればT-27の拡大により `float`。数値・文字列以外への適用はエラー E0303。単項 `-` のオペランドも数値(E0303のメッセージ変種)。単項 `!` の規則はX-10。〔負例: `arith-type`(`true + 1`、`-true`)〕
- **X-5**(実行時の数値異常): intの `+ - *` が安全整数域を外れたとき、およびintの `/ %` の除数が0のとき、**panic**すること(ADR-0015/0027。6章のpanic一覧に登録)。intの `/` は**ゼロ方向への切り捨て**(`-7 / 2` は `-3`)、`%` の結果の符号は**被除数に従う**(`-7 % 2` は `-1`。JS/Go/Rustと同じ)。floatの演算は**IEEE 754に従う**(`1.0 / 0.0` は `Infinity`、`0.0 / 0.0` は `NaN`、`x % 0.0` は `NaN`。panicしない)。〔挙動検証: `int-overflow-panic`、`int-div-zero-panic`、`int-div-negative`、`float-ieee`〕
- **X-6**(文字列): `+` は `string + string` の連結にも使えること。`string + int` 等の混合はエラー E0304 とし、補間 `"${}"` または明示変換を案内すること。〔負例: `string-plus-int`〕
- **X-7**(比較): `< <= > >=` は `int`/`float`(混合はT-27で拡大)と `string`(コードポイントの辞書順)に適用できること。それ以外はエラー E0305。〔負例: `compare-bool`〕
- **X-8**(等価): `==`/`!=` の規則:
  - **等価可能な型**(帰納的=最小不動点として定義): プリミティブ・`none`・**全フィールドが等価可能なstruct**(フィールドの深い値比較。ADR-0033)・**全メンバーが等価可能なunion**。`list`/`map`/fn値/`js.Value`/組み込み `error` は等価可能でなく、それらをフィールドに(推移的に)含むstruct、および**フィールドを辿って再帰的に自身へ到達するstruct**(`Node { next: Node | none }` 等)も等価可能でない(帰納的定義の帰結。導出が基底に到達しない)。等価可能でない型の `==` はエラー E0306(fn値はE0307)とし、listの要素比較は標準関数を、structは比較したいフィールドの直接比較を案内すること(静かなO(n)比較を作らないため)。なお全フィールドが等価可能なerror structは(structとして)等価可能である一方、組み込み `error` は等価可能でない——この非対称は意図的(エラー値の同一性比較に意味が薄いため)。**型パラメータを含むオペランドの等価はT-11(E0208)が宣言時に禁じる**(T-10の2段構えは適用しない)。
  - 両辺の型は**正規化後に同一**であること(T-27の拡大適用後)。幅の異なるunionとの比較(`x: int` と `y: int | none` の `x == y`)はエラー E0324 とし、narrowingしてから比較するよう案内すること。
  - union値同士は実行時タグが一致し、かつ値が等しいとき等しい(`none` メンバー同士は等しい)。
  - floatはIEEE 754に従う(`NaN == NaN` は偽)。
  
  〔正例: `struct-equality`(プリミティブフィールドのみのstruct)、`union-equality` / 負例: `list-equality`、`struct-with-list-equality`(推移的検査)、`recursive-struct-equality`(再帰到達)、`fn-equality`、`union-width-equality`〕
- **X-9**: リテラル `none` との比較 `== none` / `!= none` はエラー E0308 とし、`is none` / `!(x is none)` を案内すること(ADR-0005)。〔負例: `equals-none`〕
- **X-10**(論理): `&&`/`||` のオペランドと結果、および**単項 `!` のオペランド**は `bool` であること(`&&`/`||` は短絡評価。右辺は必要時のみ評価)。truthy/falsy(数値や文字列を条件に使う)は存在せず、エラー E0309 で明示比較を案内すること(`!x` のbool以外もE0309のメッセージ変種)。〔負例: `truthy`(`if count { }`、`!count`)〕

## 3.4 structリテラル・listリテラル・呼び出し・添字

- **X-11**(structリテラル): `User{name: "a", age: 1}` は**全フィールドを過不足なく**指定すること。欠落・余剰・重複はエラー E0310(欠落フィールド名を列挙)。フィールドの順序は自由。デフォルト値・省略記法はv1には無い。〔負例: `struct-literal-fields`〕
- listリテラルの型付けはT-13/T-14。生成した値への即時のメソッド呼び出しは合法(`User{...}.greet()`)。
- 呼び出しの実引数・添字の型規則は2章(T-16・T-22〜T-27)に従う。

## 3.5 if式(ADR-0009)

```ebnf
ifExpr = "if" expr block [ "else" ( block | ifExpr ) ]
         (* 構文木は4章ifFormと共通。elseの必須性は構文でなく値位置の検査(E0311)で課す *)
```

条件位置にstructリテラルを直接書く場合は括弧で囲む(ヘッダ位置の `{` 衝突回避。規則本体は4章のヘッダ規則と共通)。

- **X-12**(値位置の定義): 式が「値として使われる」のは、let/mutの初期化・実引数・returnの値・structリテラルのフィールド・**値が要求されるブロックの最終式**の位置にあるとき。値の要求は外側から内側へ伝播する(値位置のmatch式の腕ブロックの最終式は値位置。値を捨てる文の位置なら要求されない)。**関数本体の最終式が値位置となるか(暗黙のreturn)は5章が定める**。値位置のif式は `else` が必須であり、無ければエラー E0311。〔負例: `if-value-no-else`〕
- **X-13**: if式の値は各腕ブロックの最後の式。**期待型がある位置**では各腕に期待型を伝播する(T-13)。期待型が無い位置では、T-27の拡大を適用した上で両腕の型が同一であることを要求し、異なればエラー E0312 とし、union型の注釈を案内すること(`if c { 1 } else { 1.5 }` は `float` として合法)。**発散する腕**(最終文が `return`・`continue`・`break`)は型の要求から除外する(**全腕が発散するif/match式が値位置に置かれた場合は本規則がE0408を報告する**)。matchの腕にも同じ規則を適用する。〔負例: `if-arm-type-mismatch`(int/string)/ 正例: `if-arm-union-expected`、`if-arm-widening`(int/float)、`if-arm-diverging`(片腕return)〕

## 3.6 match式(ADR-0025/0034)

```ebnf
matchExpr = "match" expr "{" { arm } "}"
arm       = pattern "=>" block
pattern   = memberType [ identifier ] | "_"
```

scrutinee(match対象の式)にstructリテラルを直接書く場合は括弧で囲む(ヘッダ位置の `{` 衝突回避。規則本体は4章のヘッダ規則と共通)。

- **X-14**: scrutineeは**union型の式**であること。union以外へのmatchはエラー E0313 とし、ifを案内すること。〔負例: `match-non-union`〕
- **X-15**(パターン): 使えるパターンは「単一メンバー型のパターン(`User u` / `none` / `error e` / `MyErr e`。束縛は省略可)」と「`_`(残り全部)」のみ。union型・fn型はパターンに書けない(memberTypeの文法上不可。1メンバーずつ書くよう案内)。**`none` パターンへの束縛(`none n =>`)はエラー**(値はシングルトンで束縛する意味が無い。E0314のメッセージ変種)。リテラルパターン(`200 =>`)はエラー E0314 とし、if/else ifを案内すること(ADR-0034)。〔負例: `literal-pattern`、`none-binding`〕
- **X-16**(網羅性): 腕のパターンと `_` が、scrutineeのunionの**全メンバーを覆う**こと。覆っていなければエラー E0315「メンバー `X` が処理されていません」を不足メンバー列挙つきで報告すること。〔負例: `non-exhaustive-match`〕
- **X-17**: scrutineeのメンバーでない型のパターン、および先行する腕(`_` を含む)に覆われて到達不能な腕は、エラー E0316。〔負例: `unreachable-arm`、`foreign-pattern`〕
- **X-18**: パターンの束縛名は新しい束縛であり、シャドーイング禁止(ADR-0008)が適用されること(外側と同名ならエラー E0317、リネームを提案)。束縛した値の型はそのメンバー型に確定する。**腕の内側でscrutineeの変数自体は絞り込まれない**(束縛を使う)。〔負例: `pattern-shadowing`〕
- match式の値の型はif式と同じ規則(X-12/X-13)に従う。

## 3.7 narrowing(`is`。ADR-0005/0034)

- **X-19**: `x is T` の `T` は `x` の静的型(union)の**単一メンバー**であること(memberTypeの文法によりunion・fn型は書けない)。メンバーでない型を書いたらエラー E0318(常に偽になる検査のため)。結果は `bool`。〔負例: `is-foreign-type`〕
- **X-20**(対象は変数のみ): `is` の左辺は**ローカル変数またはパラメータの裸の識別子**に限ること(ADR-0034)。フィールドパス(`u.addr is Home`)・呼び出し結果(`findUser(1) is User`)・複合式・括弧付きはエラー E0319 とし、「一時変数に取り出してください(`let addr = u.addr`)」を案内すること。〔負例: `is-field-path`、`is-non-variable`〕
- **X-21**(フロー効果): 次の規則で `x` の型が絞り込まれること:
  - `if x is T { ... }` のthen側で `x: T`、else側で `x: 残りのメンバー`。`!(x is T)` は効果が反転する。
  - `x is T && 式` の右側で `x: T`。`x is T || 式` の右側で `x: 残りのメンバー`(左が偽の文脈)。
  - **片側の腕が `return` で終わるとき、if文以降の `x` の型は「通過する側の腕で成立している型」**(`if x is none { return }` の後は `x: 残り`、`if !(x is int) { return }` の後は `x: int`、elseがreturnなら then側の型)。「〜で終わる」とは腕ブロックの**最終文**がその文であることを指す(4章S-17と共通の定義)。
  - **絞り込みはif/&&/||の構文を通じてのみ伝わる**。boolを変数に取ると伝わらない(`let ok = x is T` の後の `if ok` は絞り込まない)。
  - 合流(if全体を抜けた後、両腕からの到達がある場合)では宣言型に戻る。
  - ループ内の `continue`/`break` によるearly-exit(`if item is none { continue }` の後の絞り込み)への拡張は、break/continueを定義する4章がこの「通過する側の型」規則を引き継いで定める。
  
  〔正例テスト: `narrowing-flow`(5形すべて)〕
- **X-22**: mutな変数への再代入は、その変数の絞り込みを無効化すること。**forループ本体の内側では、ループ内で再代入されるmut変数のループ外絞り込みは無効**(back-edgeのため保守的に扱う)。〔負例: `narrowing-invalidation`(再代入後の絞り込み前アクセスがE0320になること)〕
- **X-23**(絞り込み前アクセス): union型の値を、絞り込み(is/match/or/`?`)を経ずにメンバー固有の操作へ使ったとき、エラー E0320「`T | none` のままでは使えません。`is` か `match` で絞り込んでください」を報告すること(ADR-0005の中核)。**期待型との不一致がunionの縮小に起因する場合はE0320がE0213(2章)に優先する**(それ以外の不一致はE0213)。〔負例: `use-before-narrowing`〕

## 3.8 or と `?`(ADR-0005/0028)

- **X-24**(素のor): `expr or fallback` は `expr` の型に失敗メンバーとして **noneのみ** を持つとき書けること。**none以外の失敗メンバー(error、またはerror struct/error type)を含むなら**エラー E0321 で束縛形を案内すること(黙殺防止)。失敗メンバーを持たない型へのorはエラー E0323「この値は失敗しません。boolの選択なら `||` です」。成功メンバーが1つも無い型(`none` のみ等)へのorもE0323のメッセージ変種(「常に失敗します」)。型規則: 成功メンバーの型を `S`、`fallback` の型を `F` とすると、`F` は `S` または `S | none` に代入可能であること(T-27適用)。結果の型は正規化した `S | Fのメンバー` — つまり `F` がnoneを持たなければ `S`、持てば `S | none`。この規則により左結合の連鎖 `a or b or c`(bが `int | none`、cが `int`)が成立する。`fallback` は失敗時のみ評価。〔正例: `or-fallback`、`or-chain` / 負例: `or-silent-error`(error structを含むケースも)、`or-on-non-failable`〕
- **X-25**(束縛形): `expr or e => { ... }` は失敗メンバー(none・error・error struct)の値を `e` に束縛してブロックを評価し、その値で置き換えること。`e` の型は**失敗メンバーのunion**(複数ならそのunion、1つならその型)。ブロック値の代入可能性(**成功メンバーのunion**へ)は、**or式全体が値位置(X-12)にあるときのみ**要求する(文の位置の `save(u) or e => { log(e) }` は合法)。ブロックの**最終文**が発散文(`return`・`continue`・`break`。X-13と同じ一般化)の場合も要求を免除する(途中の発散文だけでは免除しない)。束縛名にはX-18(シャドーイング禁止)が適用される。〔正例: `or-binding`(失敗メンバー複数の束縛型)、`or-binding-statement`(文位置でのログ処理)〕
- **X-26**(`?` の式としての型): `expr?` は `expr` の型から失敗メンバー(none・error・error struct)を除いた型を持つこと。失敗時は呼び出し元へ即return(伝播のメンバー単位検査・`? "文脈"` の昇格は6章)。失敗メンバーを持たない型への `?` はエラー E0322(不要な `?`)。**成功メンバーが1つも残らない場合**(`fn f() error` への `f()?`)もE0322のメッセージ変種とし、「常に伝播します。match か return を使ってください」と案内すること。`? "文脈"` は後置連鎖の**終端にのみ**書ける(直後に `.`/`[`/`(` を続けるのはエラー E0326。括弧で包むよう案内)。パース規則: 後置連鎖中の `?` は、直後のトークンが文字列リテラルのとき文脈付き伝播(終端形)として読むこと(1トークン先読みで決定的)。〔負例: `question-on-non-failable`、`question-all-failure`、`question-context-chain`〕

## conformance対応表

| テストID | 種別 | 規則 |
|---|---|---|
| value-semantics | 挙動検証 | X-1 |
| precedence | 正例 | X-2 |
| comparison-chain | 負例 | X-3 |
| range-outside-for | 負例 | X-27 |
| or-underscore-no-arrow | 負例 | X-28 |
| generic-call-disambiguation | 正例 | X-29 |
| generic-call-ambiguous-comma | 挙動検証 | X-29 |
| arith-type | 負例 | X-4 |
| int-overflow-panic / int-div-zero-panic | 挙動検証 | X-5 |
| int-div-negative / float-ieee | 挙動検証 | X-5 |
| string-plus-int | 負例 | X-6 |
| compare-bool | 負例 | X-7 |
| struct-equality / union-equality | 正例 | X-8 |
| list-equality / struct-with-list-equality / recursive-struct-equality / fn-equality / union-width-equality | 負例 | X-8 |
| equals-none | 負例 | X-9 |
| truthy | 負例 | X-10 |
| struct-literal-fields | 負例 | X-11 |
| if-value-no-else | 負例 | X-12 |
| if-arm-type-mismatch | 負例 | X-13 |
| if-arm-union-expected / if-arm-widening / if-arm-diverging | 正例 | X-13 |
| match-non-union | 負例 | X-14 |
| literal-pattern / none-binding | 負例 | X-15 |
| non-exhaustive-match | 負例 | X-16 |
| unreachable-arm / foreign-pattern | 負例 | X-17 |
| pattern-shadowing | 負例 | X-18 |
| is-foreign-type | 負例 | X-19 |
| is-field-path / is-non-variable | 負例 | X-20 |
| narrowing-flow | 正例 | X-21 |
| narrowing-invalidation | 負例 | X-22 |
| use-before-narrowing | 負例 | X-23 |
| or-fallback / or-chain | 正例 | X-24 |
| or-silent-error / or-on-non-failable | 負例 | X-24 |
| or-binding / or-binding-statement | 正例 | X-25 |
| question-on-non-failable / question-all-failure / question-context-chain | 負例 | X-26 |
