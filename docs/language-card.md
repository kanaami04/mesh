# Mesh 言語カード v0.1

MeshのコードをAI・人間が書くための1枚(仕様1〜7章準拠。正は docs/spec/)。**このカードは必要な作業のときだけ読み込む**(常駐させない)。

## 一目でわかるMesh

```mesh
import "shop/cart"                     // パッケージ=ディレクトリ。常に cart.f() で修飾

struct User {
  name: string
  age:  int
}

error struct DbError { table: string }         // 失敗を表す型(宣言マーカーつき)
type LoadResult = User | DbError | error       // unionの命名(透過的な別名)

let maxItems = 100                     // トップレベルはlet(不変)+定数式のみ

fn (u: User) greet() string {          // レシーバつきfn=メソッド。呼び出しは u.greet()
  "こんにちは、${u.name}さん"            // 補間。関数は最後の式が戻り値(暗黙return)
}

fn findUser(id: int) User | none {     // 戻り値型は閉じ括弧の後に空白のみ。不在は | none
  ...
}

fn load(id: int) LoadResult {
  let row = query(id)?                 // ? = 失敗をそのまま呼び出し元へ伝播
  parse(row) ? "user ${id} の解析に失敗" // ? "文脈" = errorに昇格して伝播
}

fn main() {
  mut count = 0                        // 再代入するならmut。共有状態はmain内+クロージャ
  let user = findUser(1) or User{name: "guest", age: 0}   // noneのフォールバック
  print(user.greet())
  match load(1) {                      // matchは型パターンのみ・全メンバー網羅・腕は => { }
    User u    => { print(u.greet()) }
    DbError e => { print("DB: ${e.table}") }
    error e   => { print(e.message) }
  }
  for _ in cart.items() {              // for x in xs / for i in 0..n / for 条件 { } の3形のみ
    count += 1
  }
  print("${count} 件を処理")            // 未使用変数はエラーになるので必ず読む
}
```

## 型

- プリミティブ: `int`(53bit。超過はpanic)・`float`・`bool`・`string`(コードポイント単位) / 単位: `none` / 失敗: `error`・`error struct`
- コレクション: `list<T>`・`map<K,V>`(Kは int/string/bool のみ)。`T[]` 記法は無い
- union: `User | none`。**名前的型付け**(フィールドが同じでも型名が違えば別物)。型引数は**不変**(`list<int>` を `list<int|none>` に渡せない)
- 暗黙変換は int→float のみ。`toInt` 等はライブラリ

## コンパイラが強制する絶対ルール

1. **書き方は1つ**: セミコロン無し / コメントは `//` のみ / 文字列は `"..."`(補間 `${式}`)/ 型注釈はコロンの後(戻り値だけ空白)
2. **unionは絞り込んでから使う**: `is` / `match` / `or` / `?` の4手段。絞り込み前のアクセスはエラー
3. **matchは網羅必須**: 型パターン+`_` のみ(リテラルパターン無し)。腕は必ず `パターン => { ... }`
4. **ブロックの最後の式が値**: if式・matchの腕・関数本体(暗黙return)すべて共通。途中脱出は `return`
5. **不変がデフォルト**: 再代入は `mut` だけ。シャドーイング禁止。**パラメータは常に不変**
6. **値意味論**: struct/list/mapは代入・引数渡しで独立した値(コピー)。共有できるのは**mut変数を捕捉したクロージャだけ**
7. **書き忘れはエラー**: 効果のない式文・未使用変数・未使用import。意図的な破棄は `let _ = f()`

## TS/Goの手癖との違い(罠トップ10)

1. 等価は `==`(`===` は無い)。**list/map/fn の `==` は禁止**(要素比較は標準関数)
2. 三項演算子なし → if式: `let l = if c { "a" } else { "b" }`
3. `while`・`class`・`null`・`try/throw`・`new`・`this` は無い(書くと修正候補つきエラー)
4. メソッドチェーンの折り返しは**行末ドット**: `users.` ↵ `filter(...)`(行頭ドット不可)
5. `m[k]` は `V | none` を返す。`m[k] += 1` は不可 → `m[k] = (m[k] or 0) + 1`
6. 複数行structリテラルは**トレーリングカンマ必須**(関数呼び出しは不要)
7. `len("👍")` は 1(コードポイント単位。UTF-16ではない)
8. トップレベルに `mut` 不可・初期化は定数式のみ(`loadConfig()` はmainで呼ぶ)
9. list/stringの範囲外添字は**panic**、mapの不在キーは**none**(不在はデータ、範囲外はバグ)
10. クロージャに捕捉されたmut変数は絞り込めない(`let` にコピーしてから `is`)

## エラー処理

```mesh
let age = parseAge(s) or 0                    // 素のor: noneのフォールバック専用
let age = parseAge(s) or e => {               // errorは束縛必須(黙殺できない)
  log(e)
  0                                           // ブロックの最後の式が値
}
let user = findUser(id)?                       // 失敗メンバーを戻り値型に明記して伝播
let cfg  = readCfg() ? "設定の読み込みに失敗"    // 全部errorに昇格(cause連鎖でログに残る)
```

- 失敗の生成: `error("メッセージ")` / `DbError{table: "users"}`
- panic(バグ由来: 範囲外・ゼロ除算・int超過)は捕まえられない。サーバ/UIの境界が回復する
- 例外・try/catch・recoverは存在しない

## モジュール

- パッケージ=ディレクトリ。`import "ui/parts"` → `parts.f()` と常に修飾(`as` で別名)
- `export` を付けたトップレベル宣言だけが公開。公開APIに非公開型は使えない
- エントリは ルートパッケージの `fn main()`(引数なし)
