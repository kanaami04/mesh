# CLAUDE.md

Mesh(TypeScriptの型 × Goのシンプルさ・並行処理を持つ、JSにトランスパイルされる言語)の開発リポジトリ。
ここは「知らないと事故る/遠回りする」ことだけを書く。詳細は各docsが一次情報源なので重複させない。

## 最初に読むもの

新しいセッションはまず **docs/handoff.md** を読む。他のdocs(README/requirements/features/design-agenda/todo)
のどこに何が書いてあるかの案内役になっている。

## 開発の進め方(協働スタイル)

- ユーザー(kanayamaさん)は自分でコードを書かない。Claudeが実装しながら日本語で解説し、一緒に学ぶスタイル。
- 大きな機能を一気に実装しない。「説明 → 小さく実装 → 動かして確認 → 次へ」を1ステップずつ刻む。
- 実装前に「今回はここまでやる」と範囲を宣言してから着手する。

## ドキュメントのdriftに注意

`docs/requirements.md` は「正」の文書のはずだが、後続の設計決定(design-agenda.mdでの討議)に
追随できず古い記述が残ることがある。現在地を確認したいときは以下を優先する:

1. `docs/features.md` — 「できる・できない表」。現在地の一次情報源
2. ソースコード(`src/types.ts` の `typeEquals` 等)
3. `todo.md` — 次にやること

## 開発コマンド

```sh
mise run test           # bun test(TS実装)
mise run check          # bunx tsc --noEmit
mise run playground     # ブラウザプレイグラウンド
mise run run-examples   # examples/*.mesh を全部実行
mise run rust-test      # cd rust && cargo test(Rust移植版)
mise run rust-check     # cd rust && cargo clippy --all-targets
```

環境構築(mise・system パッケージ・gh認証・`/code-review`プラグイン)は **docs/setup.md** が一次情報源。

## PRワークフロー

feature branch → PR → CI green + `/code-review --comment` → **squash mergeのみ**。
`.claude/hooks/enforce-code-review.sh` が `### Code review` 見出しのコメント無しでの
`gh pr merge` を機械的に拒否する(確認できないときは常にdenyする設計)。

**PR番号の注意**: 2026-07-21のリポジトリ移管より前のコミットメッセージには、旧リポジトリの
squash mergeで付いたPR番号(`(#41)`等)が文字列として残っている。**これらは現リポジトリの
PRとは無関係**で、現リポジトリのPR番号は移管後に1から振り直されている(このPR自体が#36)。
移管前の作業を指すときはPR番号ではなくコミットSHAを使う。

## Rust移植について

`rust/` はTS実装(`src/`)の書き換えではなく、並行してゼロから育てている移植版。
TS実装が引き続き本番として動き続けている。進捗はコミット単位のマイルストーンで、
詳細はtodo.mdの各マイルストーン項目・docs/handoff.mdの「Rust移植の現状」節が一次情報源。

## メモリとdocsの使い分け

チーム/マシン横断で共有したい内容(進め方の合意・設計決定など)はClaudeのメモリではなく
このリポジトリのdocsに書く。メモリはマシンごとに独立していて同期されないため、
別マシンのセッションからは読めない。
