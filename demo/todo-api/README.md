# Todo風REST APIデモ

`mesh/http`(サーバー)+ `mesh/json`(検証つきJSONデコード)を使った、Meshの実地デモです。
コンパイラの新機能を追加するものではなく、既存機能(struct/メソッド・`T | error`・`T | none`・
クロージャによる`mut`状態捕捉・`json struct`自動デコード)を組み合わせて実用的なものが
書けることを示すためのサンプルです。TS版コンパイラ・Rust移植版コンパイラの両方でコンパイル・
実行できることを確認済みです([../../todo.md](../../todo.md)参照)。

## 実行方法

```sh
# TS版コンパイラ(本番)で実行
bun run mesh run demo/todo-api/main.mesh

# Rust移植版コンパイラで実行(JSを生成してnodeで動かす)
cd rust && cargo run -- ../demo/todo-api/main.mesh --emit-js > /tmp/todo-api.mjs && cd ..
node /tmp/todo-api.mjs
```

既定では `:8080` でリッスンします。ポートが埋まっている場合は `main.mesh` 末尾の
`http.listen(":8080", handler)` を書き換えてください(Meshにはまだ環境変数/CLI引数を
読む標準機能が無いため、アドレスはソースに直書きしています)。

## エンドポイント

| メソッド | パス | 内容 |
|---|---|---|
| `GET` | `/todos` | 一覧を取得 |
| `POST` | `/todos` | 作成(body: `{"title": string}`) |
| `GET` | `/todos/{id}` | 1件取得 |
| `PATCH` | `/todos/{id}` | 完了状態を更新(body: `{"done": bool}`) |
| `DELETE` | `/todos/{id}` | 削除 |

`mesh/http` v1にはルーターが無いため(design-agenda.md C-6参照)、`req.path`/`req.method`を
見て自分で分岐しています。データはプロセス内メモリのみ(`main()`内の`mut`変数をクロージャで
捕捉)で、再起動すると消えます。

## 試す

```sh
curl http://127.0.0.1:8080/todos
curl -X POST http://127.0.0.1:8080/todos -d '{"title":"buy milk"}'
curl http://127.0.0.1:8080/todos/1
curl -X PATCH http://127.0.0.1:8080/todos/1 -d '{"done":true}'
curl -X DELETE http://127.0.0.1:8080/todos/1
```
