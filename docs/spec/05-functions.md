# 仕様 5章 — 関数(Functions)

関数宣言・レシーバ付きfn(メソッド)・無名関数とクロージャ・戻り値の規則を定める。根拠ADR: [0007](../adr/0007-local-type-inference.md)(シグネチャ注釈必須)/ [0011](../adr/0011-return-type-go-style.md)(戻り値型の記法)/ [0012](../adr/0012-struct-receiver-methods.md)(レシーバfn)/ [0029](../adr/0029-anonymous-fn-contextual-typing.md)(無名関数の文脈的型付け)/ [0035](../adr/0035-statement-details.md)(パラメータ不変)/ [0036](../adr/0036-closures-and-implicit-return.md)(参照捕捉・暗黙return)。

エラーコードは `E05xx`。負例のテストIDは `tests/05-functions/` 配下。規則番号(F-n)は安定ID。

本章は3章の申し送り(関数本体の値位置=F-5)と2章の申し送り(レシーバの型パラメータ束縛=F-8)を消化する。未定義の名前の解決規則(E07xx)は7章が定める。

## 5.1 関数宣言

```ebnf
fnDecl   = [ "export" ] "fn" ( receiver identifier | identifier [ "<" typeParams ">" ] )
           "(" [ params ] ")" [ type ] block
receiver = "(" identifier ":" identifier [ "<" typeParams ">" ] ")"
params   = param { "," param }
param    = ( identifier | "_" ) ":" type
```

- **F-1**: 名前付き関数の宣言は**トップレベルのみ**であること。トップレベル関数は宣言順に関係なく相互参照できる(前方参照・相互再帰は合法)。関数の中の名前付きfn宣言はエラー E0501 とし、「無名関数を `let` に束縛するか、トップレベルへ移してください(再帰が必要ならトップレベルへ)」と案内すること。**再帰的なローカル関数は書けない**(letの初期化式の中では宣言中の名前は不可視)。〔負例: `nested-named-fn`〕
- **F-2**: 名前付き関数の引数の型注釈は文法上必須(ADR-0007。欠落 `fn f(x)` はE0509のメッセージ変種で注釈を案内)。戻り値型の省略は `none` の糖衣(T-26)。〔負例: `named-fn-no-annotation`〕
- **F-3**: パラメータ名の重複はエラー E0502。パラメータ名・レシーバ名・**無名関数のパラメータ名(exprParam)**にも**シャドーイング禁止(S-4)が適用される**(外側の可視名・自関数名との衝突はE0402。とくにクロージャのパラメータが捕捉候補のローカル変数を隠す形を防ぐ)。パラメータは不変(ADR-0035。代入はS-7のE0404変種)。`_` をパラメータ名にでき、束縛を作らない。**未使用のパラメータ・レシーバはエラーにしない**(S-6の対象外。`_` やリネームはlintの領分)。〔負例: `duplicate-param`、`param-shadowing`(名前付きfnの形と、クロージャのパラメータが外側ローカルを隠す形)〕
- **F-4**: 同名の関数の重複宣言はエラー E0503。**オーバーロード・デフォルト引数・可変長引数は存在しない**。呼び出しの実引数の個数が仮引数と一致しないときはエラー E0511(個数と宣言位置を表示)。〔負例: `duplicate-fn`、`arg-count`〕

## 5.2 戻り値と暗黙のreturn(ADR-0036)

- **F-5**(申し送りの決着): 戻り値型が `none` 以外の関数の本体は**値位置**(X-12)であること。本体ブロックの最終式が戻り値になる(暗黙のreturn)。**戻り値型は、本体の最終式と各 `return` の式へ期待型として伝播する**(T-13。`fn f() list<int> { [] }` や混在リテラルのunion戻りが成立し、ジェネリック関数の型引数推論=T-18もこの伝播に乗る)。戻り値型が `none`(省略含む)の関数の本体は値位置ではない(最終文は自由。S-2は通常どおり適用)。

```
fn greet(u: User) string {
  "こんにちは、" + u.name        // 最終式=戻り値
}

fn abs(x: int) int {
  if x < 0 { return -x } else { return x }   // 全腕発散(S-23)も合法
}
```

- **F-6**: 暗黙return値・`return expr` の値は戻り値型に代入可能であること(T-22〜T-27)。戻り値型が `none` 以外の関数で、本体の最終文が「値を持つ式」でも「発散する文(S-23)」でもないとき、エラー E0504「値を返していません。最終式または `return` を」。**関数本体ではこのE0504がS-1のE0408に優先する**。〔正例: `implicit-return`、`return-everywhere`(全腕returnのif/match・無限for)/ 負例: `missing-return-value`〕
- **F-7**: 値なし `return` は戻り値型が `none` の関数でのみ書ける**制限つき短縮形**であり、意味は `return none` と同じ(S-18)。それ以外の関数ではE0504のメッセージ変種。〔負例: `bare-return-in-valued-fn`〕

## 5.3 レシーバ付きfn(メソッド。ADR-0012)

- **F-8**: レシーバの型は**同一パッケージで宣言されたstruct**であること(プリミティブ・union・list/map・他パッケージのstructへのメソッドはエラー E0505)。レシーバの型引数位置に書けるのは**相異なる裸の新規識別子のみ**で、個数は宣言の型パラメータ数と一致すること(`fn (b: Box<T>) get() T` — `T` はここで束縛されシグネチャ・本体で使える。`Box<int>` への特殊化や重複名はエラー E0508)。**メソッドは自身の型パラメータを持てない**(fnDecl文法上、レシーバと `<T>` は併用不可。したがってメソッド呼び出しに型引数は無く、`.` の後の `<` は常に比較=X-29の拡張は不要)。〔負例: `receiver-non-struct`、`receiver-type-args`(具体型への特殊化と、メソッド自身の型パラメータ `fn (u: User) f<T>()` の2形)/ 正例: `generic-receiver`〕
- **F-9**: メソッド名は同一レシーバ型の中で一意であり、**フィールド名とも衝突できない**こと(エラー E0506)。メソッドと自由関数は名前空間が分離しており、同名で共存できる。〔負例: `method-field-collision`〕
- **F-10**: レシーバはパラメータと同様に不変(代入はE0404変種)。メソッドが「更新後の値」を提供したい場合は新しい値をreturnする(`fn (t: Todo) complete() Todo`)。〔負例: `receiver-assign`〕
- **F-11**: メソッドの呼び出しはドット記法のみ(`u.greet()`)。メソッドを自由関数形で呼ぶ(`greet(u)`)、およびメソッドを値として取り出す(`u.greet`)はエラー E0507(後者はクロージャ `fn() string { u.greet() }` を案内)。名前付き自由関数を値として参照する(`let f = greet`)のは合法で、型はそのfn型。**ジェネリック関数の参照**は、期待型が型引数を確定するときのみ合法(`let f: fn(list<int>) int | none = first`)。確定しなければエラー E0210(T-18)。〔負例: `method-as-value` / 正例: `fn-as-value`、`generic-fn-reference`〕

## 5.4 無名関数とクロージャ

```ebnf
fnExpr     = "fn" "(" [ exprParams ] ")" [ type ] block
exprParams = exprParam { "," exprParam }
exprParam  = ( identifier | "_" ) [ ":" type ]
```

- **F-12**(文脈的型付け。ADR-0029の一覧): 無名関数が引数・戻り値の型注釈を省略できるのは、**T-13の期待型伝播がfn型を届ける位置**にあるとき。具体的には: (a) fn型が宣言された引数位置(`http.serve(3000, fn(req) { ... })`)、(b) fn型の注釈付きlet/mut初期化、(c) fn型フィールドへのstructリテラル初期化、(d) 戻り値型がfn型の関数の暗黙return・return式(makeCounter形)、(e) 期待型が伝播しているif/match腕・or束縛ブロックの値。期待型が無い位置では注釈必須であり、欠けていたらエラー E0509「この位置では型が推論できません。`fn(x: int) int { ... }` の形で」。〔正例: `contextual-fn`(a〜dの形)/ 負例: `fn-no-context`〕
- **F-13**(捕捉。ADR-0036): クロージャは外側のローカル変数・パラメータを捕捉できる。**mut変数の捕捉は参照(共有)**であり、クロージャ経由の変更は外側に見え、外側の変更もクロージャに見える。同じ変数を捕捉した複数のクロージャは同じ変数を共有し、**fn値のコピー・structへの格納を経てもこの共有は維持される**(値意味論X-1の唯一の意図的な例外。ADR-0036)。let変数・パラメータの捕捉は不変のため共有の概念を生じない。**スコープ内のどこかで参照捕捉されるmut変数は、フローの位置によらず全域でnarrowingの対象外**(捕捉より前の行でも絞り込めない。X-20。`let` にコピーして絞り込む)。〔挙動検証: `closure-counter`、`closure-shared-view`(2クロージャの相互可視・struct経由コピー後の共有維持を含む)〕

```
fn makeCounter() fn() int {
  mut count = 0
  fn() int {
    count += 1        // 参照捕捉: 外側のcountを共有
    count
  }
}
```

- **F-14**: ループ変数の捕捉は**各反復の束縛**を捕まえる(S-13。反復をまたいで共有されない)。〔挙動検証: `closure-loop-capture`(クロージャのlistが0,1,2を返す)〕
- **F-15**: 無名関数の本体の中の `return` はその無名関数からのreturn(S-18)。`break`/`continue` は無名関数の境界を越えられない(S-16)。〔挙動検証: `return-in-anonymous-fn`〕
- 関数の非同期性(どの関数がasync出力になるか)は推論であり、構文・型には現れない(ADR-0021。8章)。

## 5.5 エントリポイント

- **F-16**: プログラムのエントリはトップレベルの `fn main()`(引数なし・戻り値 `none`。明示の `fn main() none` も等価で合法)であること。引数付き・`none` 以外の戻り値型・型パラメータ付き・レシーバ付きのmainはエラー E0510。`export` は不要(エントリとしての認識に関係しない)。mainは通常の関数としても呼び出せる。mainの配置(どのパッケージか)は7章が定める。〔負例: `invalid-main`〕

## conformance対応表

| テストID | 種別 | 規則 |
|---|---|---|
| nested-named-fn | 負例 | F-1 |
| named-fn-no-annotation | 負例 | F-2 |
| duplicate-param / param-shadowing | 負例 | F-3 |
| duplicate-fn / arg-count | 負例 | F-4 |
| implicit-return / return-everywhere | 正例 | F-5/F-6 |
| missing-return-value | 負例 | F-6 |
| bare-return-in-valued-fn | 負例 | F-7 |
| receiver-non-struct / receiver-type-args | 負例 | F-8 |
| generic-receiver | 正例 | F-8 |
| method-field-collision | 負例 | F-9 |
| receiver-assign | 負例 | F-10 |
| method-as-value | 負例 | F-11 |
| fn-as-value / generic-fn-reference | 正例 | F-11 |
| contextual-fn | 正例 | F-12 |
| fn-no-context | 負例 | F-12 |
| closure-counter / closure-shared-view | 挙動検証 | F-13 |
| closure-loop-capture | 挙動検証 | F-14 |
| return-in-anonymous-fn | 挙動検証 | F-15 |
| invalid-main | 負例 | F-16 |
