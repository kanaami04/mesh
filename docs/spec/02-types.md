# 仕様 2章 — 型(Types)

型の種類・宣言・同一性・unionの意味論・代入可能性を定める。根拠ADR: [0005](../adr/0005-absence-and-failure-as-union.md)(union)/ [0006](../adr/0006-nominal-typing.md)(名前的)/ [0012](../adr/0012-struct-receiver-methods.md)(struct)/ [0014](../adr/0014-string-codepoint-unit.md)(文字列単位)/ [0015](../adr/0015-numbers-int53-float.md)(数値)/ [0020](../adr/0020-generics-all-no-constraints.md)(ジェネリクス)/ [0023](../adr/0023-union-naming-type-keyword.md)(type宣言)/ [0027](../adr/0027-panic-boundary-recovery.md)(panic境界)/ [0029](../adr/0029-anonymous-fn-contextual-typing.md)(文脈的型付け)/ [0032](../adr/0032-type-system-details.md)(list/map記法・type透過・添字・範囲外)。

エラーコードは `E02xx` を型エラーに割り当てる。負例のテストIDは `tests/02-types/` 配下。規則番号(T-n)は安定IDであり、追記により節の順序と一致しない場合がある。

## 2.1 型の一覧と型式の文法

```ebnf
type      = unionType
unionType = primary { "|" primary }
primary   = "none" | "error" | "int" | "float" | "bool" | "string"
          | identifier [ "<" type { "," type } ">" ]     (* 名前付き型・ジェネリック適用 *)
          | fnType
          | "(" type ")"
fnType    = "fn" "(" [ type { "," type } ] ")" type    (* 戻り型は必須。T-26 *)
```

| 分類 | 型 | 実行時表現(参考。正はコード生成設計=A-7) |
|---|---|---|
| プリミティブ | `int` `float` `bool` `string` | JSの number / number / boolean / string |
| 単位型 | `none` | 専用シングルトン |
| 失敗 | `error`(組み込みエラー値の型)、`error struct`(6章) | 各自のタグ付きオブジェクト |
| コレクション | `list<T>` `map<K, V>` | 配列 / 専用表現(Phase 2で決定) |
| 複合 | struct(2.2)、union(2.4)、`fn(T...) U` | タグ付きオブジェクト / 関数 |
| FFI | `js.Value`(10章) | 任意のJS値(不透明) |

- **T-1**: 未定義の型名が型位置に現れたとき、コンパイラはエラー E0201 を報告し、近い名前があれば候補を提示すること。〔負例: `unknown-type-name`〕
- **T-2**: ジェネリック型の型引数の個数が宣言と一致しないとき(型引数なしの裸参照 `let x: Box` を含む)、エラー E0202 を報告すること。〔負例: `type-arg-count`、`bare-generic-reference`〕
- **T-19**(fn型の結合): 型式中のfn型の戻り型は**右方最長**で取ること。`fn(int) int | none` は「`int | none` を返すfn」である(関数宣言の戻り値の読みと一致)。fn型自体をunionのメンバーにするには括弧を使う: `(fn(int) int) | none`。〔正例テスト: `fn-type-precedence`(両方の書き分け)〕
- **T-26**(fn型の戻り型は必須): **型式中のfn型は戻り型を省略できない**こと。値を返さない関数の型は `fn(int) none` と書く(「戻り値なし」=noneを返す)。関数**宣言**での戻り値省略(`fn log(msg: string) { }`)は `none` の糖衣である。省略形 `fn(int)` が型位置に現れたらエラー E0215 とし、`fn(int) none` を案内すること(この規則によりT-19の「括弧なしのfn | union」は文法上存在しない)。〔負例: `fn-type-no-return`〕

## 2.2 struct宣言と名前的同一性

```ebnf
structDecl = [ "export" ] "struct" identifier [ "<" typeParams ">" ] "{" { field } "}"
field      = identifier ":" type
typeParams = identifier { "," identifier }
```

```
struct User {
  name: string
  age:  int
}

struct Box<T> { value: T }
```

- **T-3**: 同一struct内でフィールド名が重複したとき、エラー E0203 を報告すること。〔負例: `duplicate-field`〕
- **T-4**(名前的同一性): structの型の同一性は「**パッケージ+型名+型引数列**」で決まること(`Box<int>` と `Box<string>` は別の型)。フィールド構成が同じでも型名が違えば別の型であり、混用はエラー E0204「`UserId` が必要ですが `ItemId` が渡されました(フィールド構成は同じですが別の型です)」を報告すること。〔負例: `nominal-mismatch`〕
- プリミティブ・コレクション・fn型の同一性は構造的に決まる(`list<int>` と `list<int>` は同じ型)。unionの同一性は2.4。

## 2.3 type宣言(透過的な別名)

```ebnf
typeDecl = [ "export" ] "type" identifier [ "<" typeParams ">" ] "=" type
```

- **T-5**: `type` で付けた名前は**透過的な別名**であること。別名は使用箇所で右辺に展開され、展開結果が同じなら書き方(別名経由・直書き)によらず完全に同じ型として扱われる。再帰的な別名(T-6が許すもの)を含む同一性判定は、展開中に既出の別名適用へ戻ったら同一とみなす**サイクル検出つき比較**で行う(展開は停止する)。新しい名目型が欲しい場合はstructで包む。〔正例テスト: `alias-transparent`(`type MaybeUser = User | none` と `User | none` の相互代入)、`recursive-alias-identity`(`Json` とその1段展開形の相互代入)〕
- **T-6**(再帰の許容範囲): typeエイリアスの**裸の自己参照**(展開しても構成子の内側に入らない参照)はエラー E0205 とすること(`type A = A | int`、`type A = B` かつ `type B = A`)。**型構成子(list・map・fn・structのフィールド)の引数位置を経由する再帰は許される**。〔負例: `alias-self-reference` / 正例: `recursive-json`(`type Json = map<string, Json> | list<Json> | string | float | bool | none`)、`recursive-via-struct`(`struct Tree { children: list<Tree> }`)〕

## 2.4 union型

### 意味論(集合)

- **T-7**: unionは**メンバー型の集合**として扱うこと。ネストはフラット化され(`(A | B) | C` = `A | B | C`)、重複は除去され(`A | A` = `A`)、順序は同一性に影響しない(`A | B` = `B | A`)。別名の展開・ジェネリクスのインスタンス化で生じた重複も同様にフラット化される(エラーにしない。T-5の透過性と一貫させる)。〔正例テスト: `union-normalization`〕
- **T-8**: フラット化の帰結として、`(int | none) | none` は `int | none` になり「外側のnone」と「内側のnone」は区別**できない**こと。区別が必要な場合はstructで包む。〔挙動検証: `union-flatten-behavior`(`type Opt<T> = T | none` の `Opt<Opt<int>>` が `int | none` と同一に流通する)〕

### 判別可能性検査(設計監査#6)

- **T-9**: **具体型のみからなる**unionの全メンバーの対は、実行時に判別可能でなければならないこと。判別不能な対を含むunion型が書かれたとき、エラー E0206 を報告し、structで包む修正候補を提示すること。判別不能な対:
  - `int` と `float`(実行時はどちらもJSのnumber)
  - fn型同士(`fn(int) int | fn(string) string`)
  - **同じ構成子の異なる適用同士**(`list<int> | list<string>`、`map<string,int> | map<string,bool>`、およびユーザー定義ジェネリックの `Box<int> | Box<string>`。実行時タグが共通で型引数は消去されるため)
  
  判別可能な対の例: 名前の異なるstruct同士(実行時タグ=A-7)、プリミティブ異種(`int | string`、`int | bool`)、`T | none`、`T | error`、`MyErr | error`(error structは自分のタグを持つ独立の型。2.4末尾)、struct と `list<U>`。この対リストは実行時表現の設計(A-7)と連動しており、Phase 2で表現が変わる場合はADR経由で更新する。〔負例: `union-int-float`、`union-two-fn`、`union-same-constructor`(list版とBox版)〕
- **T-20**: `js.Value` は **`none` 以外の型**とのunionを組めないこと(不透明なJS値は他のあらゆる型と判別不能。E0206のメッセージ変種で「`js.decode` で先に型を確定してください」と案内)。**`js.Value | none` のみ特例で許す**(mapの読み `m[k]` に必要。ADR-0005のJS境界正規化により、Musubiのnoneシングルトンは `js.Value` としてFFIから入ってこないため判別可能)。〔負例: `union-js-value`(`string | js.Value`)/ 正例: `js-value-or-none`(`map<string, js.Value>` の読み取り)〕
- **T-10**(ジェネリクスとの2段構え。設計監査#7): 型パラメータを含むunionは、宣言時には**具体型メンバー同士の対のみ**をT-9/T-20で検査すること(`fn f<T>(x: T | int | float)` は宣言時点でE0206)。型パラメータを含む対はインスタンス化で具体化した時点でT-9/T-20を適用し、**判別不能な対**が生じたらインスタンス化の地点でエラー E0207 を報告すること(重複はT-7どおりフラット化し、エラーにしない)。〔負例: `union-instantiation-collision`(`fn f<T>(x: T | int)` に `T = float`)、`union-concrete-pair-in-generic`(宣言時検査)、`union-js-value-instantiation`(`fn f<T>(x: T | int)` に `T = js.Value`。noneとの対はT-20特例で合法のため対象外)〕
- **T-21**(errorとerror structの関係): `error` は**組み込みエラー値のみ**の型であり、`error struct` で宣言した型の上位型ではないこと(この言語に部分型階層は無い)。両者は別々の型としてunionに共存でき、タグで判別できる。「`?`/`or` の伝播対象になる」という性質の定義は6章。この帰結として、`U | error` を返す関数内で `T | MyErr` を `?` 伝播するには、MyErrを戻り値unionに明記するか `? "文脈"` でerrorへ昇格させる必要がある(メンバー単位検査。6章が定める)。〔正例テスト: `error-struct-vs-error`(`MyErr | error` のmatchで両腕を書き分け)〕

## 2.5 ジェネリクス

- 型パラメータは `fn` / `struct` / `type` の宣言で使える(ADR-0020)。制約は無く、`T` の値にできる操作は「受け取る・保持する・渡す・返す」のみ。
- **T-11**: 型パラメータの値に演算・比較・フィールドアクセスを適用したとき、エラー E0208 を報告すること(「`T` には制約が無いため操作できません」)。〔負例: `type-param-operation`〕
- **T-12**: 型パラメータを `is` / matchの型パターンの対象にしたとき、エラー E0209 を報告すること(型消去により実行時に判別できない)。〔負例: `type-param-match`〕
- 呼び出し時の型引数は明示(`first<int>(xs)`)と推論(実引数・期待型から)の両方を許す。
- **T-18**: 型引数が実引数からも期待型からも推論できないとき、エラー E0210 を報告し、明示形(`first<int>(xs)`)を案内すること。〔負例: `type-arg-uninferable`〕
- レシーバ付きfnの型パラメータ(`fn (b: Box<T>) get() T` の `T`)はレシーバ型の適用から束縛される。詳細は5章で定める。

## 2.6 期待型の伝播(設計監査・軽微項目)

- **T-13**: 注釈・引数位置などで**期待型が確定している式**には、その期待型をリテラル(list・structリテラル・無名関数=ADR-0029)の内側へ伝播すること。次は合法である:

```
type Item = Todo | User
let xs: list<Item> = [Todo{title: "milk"}, User{name: "a", age: 1}]
```

〔正例テスト: `expected-type-propagation`〕
- **T-14**: 期待型が無い位置のlistリテラルは、全要素が同じ型であることを要求すること。異なる場合はエラー E0211 とし、union型の注釈を付ける修正候補を提示すること。**空リテラル `[]` は期待型が必須**であり、無ければE0211のメッセージ変種で注釈を促すこと。〔負例: `list-literal-mixed`、`empty-list-no-context`〕

## 2.7 コレクション・文字列・数値の型規則

- **T-25**: mapのキー型 `K` は `int` / `string` / `bool` に限ること(値の等値が自明なプリミティブのみ。floatはNaNの等値問題のため除外)。それ以外はエラー E0214 とし、キーにできる型の一覧を案内すること。`K` が型パラメータの場合は宣言時に検査せず、インスタンス化で具体化した地点で検査すること(T-10と同じ2段構え)。〔負例: `map-key-type`、`map-key-type-instantiation`(`struct Cache<K> { m: map<K, string> }` に `K = float`)〕
- `len(s)` はコードポイント数を返す(ADR-0014)。
- **T-15**: `s[i]` は **string(1コードポイント)** を返すこと。char型は存在しない。〔正例テスト: `string-index-type`(`"👍あ"[0]` が `"👍"`)〕
- **T-16**: list・stringの添字が範囲外のとき、**panic**すること(位置つき。回復は境界のみ=ADR-0027。panicする操作の一覧は6章)。mapの読み取り `m[k]` は `V | none` を返す(**不在はデータ、範囲外はバグ**)。〔挙動検証: `index-out-of-range-panic` / 正例テスト: `map-read-type`(`m[k]` の型が `V | none` で、絞り込み無しの使用がエラーになる)〕
- **T-17**: 暗黙の型変換は **int→float の拡大1方向のみ**であること(ADR-0015)。それ以外(float→int、int→string等)が必要な文脈ではエラー E0212 とし、明示変換(`toInt` 等。11章)を案内すること。〔負例: `implicit-conversion`〕

## 2.8 代入可能性(assignability)

「型Sの値を、型Tが要求される場所に置けるか」の規則。代入・実引数・return・structリテラルのフィールドすべてに適用される。

- **T-22**(unionへの拡大): 型Sは、正規化(T-7)後の**Sのメンバー集合がTのメンバー集合の部分集合**であるとき、union型Tに代入可能であること。`int` → `int | none`、`int | none` → `int | string | none` は合法。逆方向(union→狭い型)は代入不可であり、narrowing(3章)を要する。〔正例テスト: `union-widening`〕
- **T-23**(不変性): ジェネリック型構成子は型引数について**不変(invariant)**であること。`list<int>` は `list<int | none>` に代入**できない**(可変なコレクションでの健全性のため。Goと同じ割り切り)。違反はE0213のメッセージ変種とし、要素ごとの変換(mapによる詰め替え)を案内すること。〔負例: `invariance`〕
- **T-24**(一般の型不一致): 上記のいずれの規則でも代入可能にならないとき、エラー E0213「`T` が必要ですが `S` が渡されました」を位置つきで報告すること。名前的不一致(T-4のE0204)はこの特殊形である。〔負例: `type-mismatch-basic`(`let x: int = "a"`)〕
- **T-27**(int→float拡大との合成): T-17の拡大は、**値の期待位置**で代入可能性判定(T-22〜T-24)と期待型伝播(T-13)に**先立って**適用すること。具体的には: ①期待の原子型がfloatの位置ではintの式をfloatとして扱う(`let x: float | none = 5` は、5がfloatに拡大されてからT-22で合法)。②期待型伝播はfloat要素をリテラル内の各要素に伝えるため `let xs: list<float> = [1, 2]` も合法(各要素が拡大)。③**すでに構成済みの値の型構成子引数には適用しない**(`list<int>` の変数を `list<float>` に渡すのはT-23どおり不可)。④**union型の値のメンバー単位でも拡大しない**(`int | none` の値を `float | none` へ渡すのは不可。narrowingで取り出してから個別に拡大する。エラーメッセージでその手順を案内)。⑤この規則群は**演算子のオペランド位置**(3章のX-4/X-7/X-8/X-13/X-24)にも同様に適用される。〔正例テスト: `widening-in-union`、`widening-list-literal` / 負例: `no-widening-through-constructor`〕

## conformance対応表

| テストID | 種別 | 規則 |
|---|---|---|
| unknown-type-name | 負例 | T-1 |
| type-arg-count / bare-generic-reference | 負例 | T-2 |
| fn-type-precedence | 正例 | T-19 |
| duplicate-field | 負例 | T-3 |
| nominal-mismatch | 負例 | T-4 |
| alias-transparent / recursive-alias-identity | 正例 | T-5 |
| alias-self-reference | 負例 | T-6 |
| recursive-json / recursive-via-struct | 正例 | T-6 |
| union-normalization | 正例 | T-7 |
| union-flatten-behavior | 挙動検証 | T-8 |
| union-int-float / union-two-fn / union-same-constructor(list版とBox版) | 負例 | T-9 |
| union-js-value | 負例 | T-20 |
| js-value-or-none | 正例 | T-20 |
| union-instantiation-collision / union-concrete-pair-in-generic / union-js-value-instantiation | 負例 | T-10 |
| error-struct-vs-error | 正例 | T-21 |
| type-param-operation | 負例 | T-11 |
| type-param-match | 負例 | T-12 |
| type-arg-uninferable | 負例 | T-18 |
| expected-type-propagation | 正例 | T-13 |
| list-literal-mixed / empty-list-no-context | 負例 | T-14 |
| map-key-type / map-key-type-instantiation | 負例 | T-25 |
| fn-type-no-return | 負例 | T-26 |
| string-index-type | 正例 | T-15 |
| index-out-of-range-panic | 挙動検証 | T-16 |
| map-read-type | 正例 | T-16 |
| implicit-conversion | 負例 | T-17 |
| union-widening | 正例 | T-22 |
| invariance | 負例 | T-23 |
| type-mismatch-basic | 負例 | T-24 |
| widening-in-union / widening-list-literal | 正例 | T-27 |
| no-widening-through-constructor | 負例 | T-27 |
