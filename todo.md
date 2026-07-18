# Mesh TODO

> プロジェクトの「正」は [docs/requirements.md](docs/requirements.md)(要件定義書)。
> このTODOの優先順位は要件定義の討議結果で見直す。

## 次の一手(おすすめ順)

- [ ] **union路線への移行** — ほぼ完了(2026-07-17)
  - [x] union型(`T | none` / `T | error`)・narrowing・`!`/`or`・nil/多値戻りの撤去
  - [x] match式(網羅性検査・アーム内narrowing・`_`・複数パターン・リテラルパターン)
  - [x] 文字列リテラル型(widening込み)と `type Status = ...` 宣言(循環検出込み)
  - [ ] `is` の対象拡大(現状は none/error のみ。型名・リテラルへ)
- [ ] **struct と型定義** — コア実装済み(2026-07-17)
  - [x] struct 宣言・リテラル生成・フィールドアクセスの型検査・再帰struct・`type =` の `{...}` ガード
  - [ ] インライン `{...}` 型式(union内)と判別可能union(`{ kind: "ok", ... } | ...`)
  - [ ] 同一性を名前ベース→構造的比較へ拡張(インライン型式とセット)
- [ ] **その他の採用決定済み構文の実装**(union移行と並行または後続)
  - ~~文字列補間 `"${式}"`~~ ✅ 実装済み(2026-07-17)
  - ~~デフォルト不変 + `mut`~~ ✅ 実装済み(2026-07-17)
  - E1 `!` / E2 `or`(union移行とセットで)
  - `spawn` 式(旧 `go` の改名+受取口を返す拡張)/ `wait` ブロック / `select`(最後)
  - map型(`m[k]` は `V | none`)/ for range(完全形のみ)
  - 決定記録は [docs/syntax-proposals.md](docs/syntax-proposals.md) と [docs/design-agenda.md](docs/design-agenda.md)
- [ ] **Rust移植の開始**
  - 今の37テスト(tests/)を「合格基準」にする
  - lexer → parser → checker → codegen の順に移植
  - Rust学習を兼ねる(所有権とASTの付き合い方が最初の山)

## 言語機能(中期)

- [ ] チャネルの容量指定 `chan<int>(0)` と同期(ブロックする)送信
- [ ] map型 `map<string, int>`
- [ ] `for x := range arr` / `for k, v := range m`
- [ ] 標準ライブラリ: http / json / file(JS APIの薄いラッパー)
- [ ] `var x: int = 0` 形式の宣言(ゼロ値の設計もセットで)
- [ ] defer 文

## ツール・品質(中期)

- [ ] エラーメッセージにソース行の表示(`^~~~` 付き)
- [ ] エラーからの復帰(1つの構文エラーで止まらず複数報告する)
- [ ] フォーマッタ `mesh fmt`(gofmt 相当。設定オプションなし)
- [ ] VS Code拡張(シンタックスハイライトだけでも)
- [ ] ソースマップ出力(生成JSのエラーを .mesh の行に対応させる)

## 完了

- [x] ランタイム検査=層1・パニック方針の実装(2026-07-17)— 範囲外の読み書き・整数ゼロ除算/剰余が
      `panic: file:line:col: 原因` で即停止。リテラル `1 / 0` はコンパイル時検出
- [x] デフォルト不変+`mut`・文字列補間 `"${式}"`(2026-07-17)
- [x] ブラウザプレイグラウンド(2026-07-17)— `mise run playground` で http://localhost:8765。
      左にMeshエディタ、右に生成JSと実行結果。実行はWeb Worker隔離+10秒タイムアウト
- [x] v0: lexer / parser / 型検査 / JS codegen(2026-07-17)
- [x] goroutine(go文)・channel・多値戻り・明示的エラーハンドリング
- [x] CLI(mesh run / build / check)
- [x] テスト37件(lexer / parser / checker / e2e)
- [x] mise でツールバージョン管理
