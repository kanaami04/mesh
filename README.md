# Mesh

[![CI](https://github.com/ryota-kanayama/mesh/actions/workflows/ci.yml/badge.svg)](https://github.com/ryota-kanayama/mesh/actions/workflows/ci.yml)

**TypeScript の型 × Go のシンプルさ・並行処理** を目指した、JavaScript にコンパイルされる言語。

ブラウザでもサーバー(Node/Bun/Deno)でも動く JavaScript を出力するので、
フロントエンドとバックエンドの両方を1つの言語で書けます。

> 名前の由来: このリポジトリの親ディレクトリ `kanaami`(金網)の英訳。
> channel で処理を編み込む言語、というイメージも重ねています。

> **開発を引き継ぐ/再開する場合**: まず [docs/handoff.md](docs/handoff.md) を読んでください。
> 現在の実装状況・進行中のタスク・開発の進め方の合意事項がまとまっています。

```go
// examples/channels.mesh
fn worker(id: int, ch: chan<string>) {
	sleep(10 * id)
	ch <- "worker ${id} done"   // 文字列補間
}

fn main() {
	ch := chan<string>(none)

	for i := 1; i <= 3; i++ {
		spawn worker(i, ch)   // goroutine 風の並行実行
	}

	for i := 0; i < 3; i++ {
		print(<-ch)
	}
}
```

## 使い方

ツールは [mise](https://mise.jdx.dev/) で管理しています。初回は `mise install` を実行してください。

```sh
mise run playground     # ブラウザプレイグラウンド (http://localhost:8765)
mise run test           # コンパイラのテスト
mise run check          # TypeScript の型チェック
mise run run-examples   # サンプルを全部実行

bun run mesh run   examples/hello.mesh   # コンパイルして即実行
bun run mesh build examples/hello.mesh   # hello.mjs を書き出す
bun run mesh check examples/hello.mesh   # 型検査のみ
bun run mesh check prog.mesh --json      # AIエージェント向けの構造化診断(JSON)
bun run mesh card                        # 言語カード(AIのコンテキストに貼る圧縮仕様書)
```

### AIにMeshを書かせるには

`mesh card` の出力を CLAUDE.md やシステムプロンプトに貼ってください。カードは
「全構文+存在しない機能のリスト+頻出エラーの直し方」を1枚に圧縮した仕様書で、
エージェントは `mesh check --json` で検証しながら自己修正ループを回せます。
カードの主張はテストスイートで実装と突き合わせており、乖離するとCIが落ちます。

## 言語ツアー

### 変数と型

```go
name := "mesh"        // := で宣言。型は右辺から推論される
count := 0            // int
ratio := 1.5          // float
ok := true            // bool
nums := [1, 2, 3]     // int[]
```

型: `int` `float` `string` `bool` `error` `none` `any` / 配列 `T[]` / map `map<K, V>` /
チャネル `chan<T>` / union `A | B` / 文字列リテラル型 `"active"` / 関数

### エラーと不在は union 型で(言語の背骨)

例外も null もありません。失敗し得る関数は `T | error`、無いかもしれない値は `T | none` を
返し、`is` で絞り込んでから使います。**絞り込む前に使うとコンパイルエラー**です。

```go
fn divide(a: int, b: int) int | error {
	if b == 0 {
		return error("division by zero")
	}
	return a / b
}

fn main() {
	result := divide(10, 3)
	if result is error {
		print("error:", result)
		return
	}
	print(result)                 // ここでは int に絞り込み済み

	safe := divide(1, 0) or _ => 0   // or: 失敗なら既定値。errorを捨てる痕跡 _ が字面に残る
	print(safe)

	logged := divide(1, 0) or e => report(e)  // 失敗値を受け取って使う形
	print(logged)
}

// ? は「自分も失敗し得る関数」の中で使う: 失敗なら呼び出し元へ即伝播(Rustの ? と同じ)
fn half(n: int) int | error {
	return divide(n, 2)?
}

// ? "文脈" — 失敗時に文脈を前置した error として伝播(noneも error に昇格)
fn readPort(s: string) int | error {
	return toInt(s) ? "config port"
}
```

### struct とメソッド

データの形は `struct`、振る舞いは関数(Goスタイルのレシーバ構文)で書きます。

```go
struct Todo {
	title: string
	done: bool
}

fn (t: Todo) complete() Todo {
	return Todo{title: t.title, done: true}
}
fn (t: Todo) render() string {
	if t.done { return "[x] " + t.title }
	return "[ ] " + t.title
}

todos[0] = todos[0].complete()
print(todos[0].render())
print(Todo{title: "x", done: false}.complete().render())  // 連鎖(左から右へ読める)
```

内部的には第1引数にレシーバを取る普通の関数で、vtable・継承・インターフェースはありません。
**名前空間は自由関数と完全に分離**していて、`render(t)`という裸の呼び出しはできず`t.render()`のみです
——同じ操作の呼び方が2通り存在することはありません。この分離のおかげで、`User`にも`Order`にも
`describe()`という同名メソッドを両方持たせられます(自由関数だと名前が衝突します)。

struct の同一性は**名前的**です。形が同じでも `Meters` と `Dollars` は別の型として扱われ、
取り違えはコンパイルエラーになります(単位型・ID型のラッパーがちゃんと守ってくれる、
Go/Rustと同じ方式)。名前を持たない判別可能unionの `{ ... }` メンバーが絡む比較だけ、
形で判定します。

### 判別可能union(discriminated union)

タグ付きのstructをunionにまとめた形です。無名の `{ ... }` 型式は `type` 宣言のunionの中でだけ
書けます(裸で `type X = { ... }` と書くのは今まで通りエラーで、単一の形なら `struct` を使えと
誘導されます)。

```go
struct User {
	name: string
}

type GetUserResponse = { kind: "ok", user: User } | { kind: "notFound" } | { kind: "unauthorized" }

fn getUser(id: string) GetUserResponse {
	u := findUser(id)
	if u is none {
		return GetUserResponse{ kind: "notFound" }
	}
	return GetUserResponse{ kind: "ok", user: u }
}

fn describe(res: GetUserResponse) string {
	return match res {
		{ kind: "ok" } => "found: ${res.user.name}"
		{ kind: "notFound" } => "not found"
		{ kind: "unauthorized" } => "unauthorized"
	}
}
```

値を作るときは **union自身の名前**をstructリテラル名としてそのまま使います
(`GetUserResponse{kind: "ok", user: u}`)——書いたフィールドの組み合わせから該当メンバーを
特定するので、メンバーごとに別名のstructを用意する必要はありません。`match`は部分構造パターン
`{ kind: "ok" }` でメンバーを絞り込みます(パターンに書いたフィールドだけで判定し、それ以外は
問いません)。フィールドをその場で束縛する構文は無く、絞り込んだ後は`res.user`のように
普通のフィールドアクセスを使います。

自己参照する判別可能union(木構造・ASTなど)も書けます——再帰参照がstructフィールドの中に
あればOKです(再帰structの `next: Node | none` と同じ仕組み):

```go
// examples/tree.mesh
type Tree = { kind: "leaf", value: int } | { kind: "node", left: Tree, right: Tree }

fn sumTree(t: Tree) int {
	return match t {
		{ kind: "leaf" } => t.value
		{ kind: "node" } => sumTree(t.left) + sumTree(t.right)
	}
}
```

唯一の例外は「union同士がstructを挟まず裸で直接参照し合う」形
(`type A = B | none` かつ `type B = A | error`)で、これだけは `type alias cycle` エラーに
なります(必要ならstructフィールドに包んでください)。

### モジュール(import / export)

パッケージ = ディレクトリです。ディレクトリ名がそのままパッケージ名になり(`package`宣言は
ありません)、`export`を付けたトップレベル宣言だけが外から見えます。

```go
// mathutil/ops.mesh
export fn add(a: int, b: int) int { return a + b }  // exportで公開
fn helper(n: int) int { return n * 2 }               // 無印はパッケージ内限定

// app.mesh(エントリファイル)
import "mathutil"

fn main() {
	print(mathutil.add(1, 2))          // 常に パッケージ名.シンボル で修飾
	p := mathutil.Point{x: 3, y: 4}    // exportされたstructの生成も同様
}
```

同じパッケージ内の複数`.mesh`ファイルはimport不要で互いに見えます(フラットな1名前空間)。
メソッドに個別のexportは無く、structをexportすればメソッドも使えます。
未exportへのアクセス・未知のパッケージ・import循環はコンパイルエラーです。
サンプル: [examples/modules_demo.mesh](examples/modules_demo.mesh)

### 並行処理: spawn / wait / channel

```go
task := spawn f(1)     // 並行起動して「結果の受取口」を得る
v := <-task            // 必要になった時点で待つ

detach sendEmail(u)    // プログラム所有のバックグラウンドタスク(関数は待たずに戻れる)

wait {                 // 関数の出口より早い時点でまとめて待ちたいとき
	spawn g(1)
	spawn g(2)
}

ch := chan<string>(none)   // チャネル: 複数タスクの結果を集めるなど(容量は常に明示。F-11)
ch <- "hello"          // 送信
msg := <-ch            // 受信(値が来るまで待つ)
```

Meshの並行処理は**構造化**されています——すべてのタスクに所有者がいます。
`spawn`したタスクは**囲む関数が所有**し、関数を抜けるとき(早期returnでも)暗黙に待たれるので、
Goで有名な「発射しっぱなしのgoroutineリーク」は構文的に書けません。関数より長生きすべき仕事
(メール送信・ログなど)だけを`detach`で明示的にプログラム所有へ切り替えます。

### channel仕様: 容量・close・select

```go
ch := chan<int>(none)  // 明示的に無制限バッファを選ぶ(送信は常に即完了。容量省略はコンパイルエラー)
ch := chan<int>(0)     // 容量0 = 真の同期(受信者が現れるまで送信ブロック)
ch := chan<int>(3)     // 容量3 = バッファが3個埋まったら送信ブロック(Go互換の本物のブロッキング)

close(ch)               // これ以上送信しないことを宣言(二重close・close後の送信はpanic)
v := <-ch                // 受信は常に int | closed(mapの V | none と同じ理由)
if v is closed { ... }   // 絞り込んでから使う

msg := select {          // 複数チャネルのうち先に準備できた方を選ぶ
	v := <-ch1 => "from ch1: ${v}"
	v := <-ch2 => "from ch2: ${v}"
	_ => "nothing ready"   // あれば非ブロッキング
}
```

`<-ch`は**常に`T | closed`**を返します——mapの読み取りが常に`V | none`を返すのと同じ理由で、
「もう値が来ない」を無視できないようにするためです。`select`はmatchの見た目を踏襲していますが、
パターンが「型」ではなく「どのチャネル操作が先に終わったか」なので独立した構文です。

### 制御構文

```go
if x > 10 { ... } else if x > 5 { ... } else { ... }   // 条件に丸括弧は不要

for i := 0; i < 10; i++ { ... }   // C スタイル
for x < 100 { ... }               // while 相当
for { ... }                       // 無限ループ(break で脱出)
```

### 組み込み関数

| 関数 | 意味 |
|---|---|
| `print(...)` | 標準出力(none は `none`、error はメッセージを表示) |
| `str(x)` | 文字列化(`"id: " + str(42)`) |
| `len(x)` | 文字列・配列の長さ |
| `push(arr, v)` | 配列に追加(破壊的) |
| `error(msg)` | エラー値を作る |
| `sleep(ms)` | ミリ秒待つ |
| `delete(m, k)` | mapからキーを削除 |
| `contains(arr, v)` | 配列に含まれるか(`bool`) |
| `indexOf(arr, v)` | 配列内の位置(`int \| none`) |
| `keys(m)` / `values(m)` | mapのキー/値の配列(挿入順) |
| `sort(arr)` | 昇順に並べた**新しい**配列を返す(非破壊。`int[]`/`float[]`/`string[]`のみ) |
| `split(s, sep)` / `join(arr, sep)` | 文字列分割(常に`string[]`)/ 結合 |
| `trim(s)` / `upper(s)` / `lower(s)` | 前後空白除去 / 大文字化 / 小文字化 |
| `toInt(s)` | 文字列→整数(`int \| error`。パース失敗時は`error`) |
| `filter(arr, pred)` | 条件に合う要素だけの新しい配列 |
| `map(arr, f)` | 各要素を変換した新しい配列(要素の型が変わってもよい) |
| `get(arr, i)` | 範囲外でもpanicしない安全な読み(`T \| none`) |
| `reduce(arr, f, init)` | 畳み込み(`f: fn(Acc, T) Acc`) |
| `close(ch)` | チャネルをclose(以後の受信は`closed`) |

### 意図的にオミットしているもの(Go 流のシンプルさ)

- セミコロン(行末に自動挿入)
- `while` / `do-while`(`for` に統一)
- 例外・try/catch(エラーは `T | error` の union で返す)
- `null` / `undefined`(不在は `T | none` の union で型に現す)
- 多値戻り `(T, error)`(union に置換)
- クラス・継承・インターフェース(structメソッドはあるが、vtable・継承は無い)

## コンパイラの仕組み

```
ソースコード (.mesh)
   │
   ▼  src/lexer.ts    ── 文字列をトークン列に分解(Go式セミコロン自動挿入もここ)
トークン列
   │
   ▼  src/parser.ts   ── 再帰下降構文解析で AST(構文木)を構築
AST
   │
   ▼  src/checker.ts  ── 型推論・型検査。式に型を書き込み codegen へ引き継ぐ
検査済み AST
   │
   ▼  src/codegen.ts  ── JavaScript を出力(+ src/runtime.ts のランタイムを同梱)
JavaScript (.mjs)
```

### goroutine → Promise 変換の仕掛け

Mesh の関数はすべて `async function` として出力され、呼び出しは常に `await` されます。
これにより:

- `<-ch`(受信)は `await ch.recv()` になる — Go の「ブロックして待つ」が
  JS の「イベントループに譲って待つ」に対応する
- `spawn f(x)` は **await しない** 呼び出しになる — 裏で走り続ける Promise = goroutine。
  Go と違い「結果の受取口」を返すので、`<-task` で後から値を受け取れる
- 容量つきチャネル(`chan<T>(n)`)の送信ブロックも、`await` 可能な Promise として
  本物のブロッキングを実現している(panic による近似ではない)

### Go との意味論の違い(割り切り)

- 並行処理は**シングルスレッド**(JS のイベントループ上)。並列(マルチコア)ではない
- `int` は JS の number(53bit 整数)。`int` 同士の除算は切り捨て。演算結果が safe integer の
  範囲(`Number.isSafeInteger`)を超えたら panic で即停止する(無音の桁あふれを許さない)
- チャネルの容量は常に明示必須(`chan<T>()` はコンパイルエラー)。無制限バッファが欲しいときは
  `chan<T>(none)`、Go の既定(容量0の同期)が欲しいときは `chan<T>(0)` と書く

## ロードマップ

- [x] v0: lexer / parser / 型検査 / JS codegen / go・channel
- [x] ブラウザで動くプレイグラウンド(`mise run playground`)
- [x] union路線コア: `T | none` / `T | error` / `is` narrowing / `?` / `or` / 文字列補間 /
      デフォルト不変+`mut` / ランタイム検査(範囲外・ゼロ除算は位置つき panic)
- [x] match式(網羅性検査)/ 文字列リテラル型 / `type` 宣言 / struct
- [x] structメソッド(Goスタイルのレシーバ構文)/ 標準ライブラリ第一〜三弾(配列・map・文字列・高階関数)
- [x] 2段スコープの構造化並行(spawn/detach)/ channel仕様の完成(容量・close・select)
- [x] 判別可能union(インライン `{...}` 型式・自己参照対応)と構造的型付け
- [x] モジュールシステム(import / export、パッケージ=ディレクトリ)
- [ ] 標準ライブラリの拡充と層分け(core共通 / 環境別: http・json・DOM など)
- [ ] コンパイラを Rust に移植(現行テストスイートが通ることをゴールにする)
