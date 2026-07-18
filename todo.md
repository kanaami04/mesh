# Mesh TODO

> プロジェクトの「正」は [docs/requirements.md](docs/requirements.md)(要件定義書)。
> このTODOの優先順位は要件定義の討議結果で見直す。

## 次の一手(おすすめ順)

- [ ] **言語カード実証実験の継続** — 全記録は [docs/card-experiments.md](docs/card-experiments.md)
  - [x] 第1〜3回実施(TODO×2・単語頻度)。6→1→1往復。手法はメモリに記録
  - [x] 空の型付き配列 `Todo[]{}` 実装、カード記述漏れ多数を追記(参照値/前置!/print/map更新等)
  - [x] **`mut best := none` 問題を解決** — 型注釈つき宣言 `mut best: string | none = none`
        を実装(2026-07-18)。空配列 `xs: Todo[] = []` も同時に解決(any[]の互換を緩和)
  - [x] 第4回実験(2026-07-18): 型注釈導入後、第3回と同一題材で再測定。往復1のまま、
        不在アキュムレータをフラグ回避→本物のabsence handlingで記述。map反復順(挿入順)をカード明記
  - [ ] 第5回は別題材(簡易パーサ・電卓等)で新しい穴探し。または struct メソッド/stdlib の必要性を再確認
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
  - ~~`spawn` 式 / `wait` ブロック~~ ✅ 実装済み(2026-07-18)/ `select`(残)
  - ~~map型(`m[k]` は `V | none`)/ for range(完全形のみ)~~ ✅ 実装済み(2026-07-18)
  - 決定記録は [docs/syntax-proposals.md](docs/syntax-proposals.md) と [docs/design-agenda.md](docs/design-agenda.md)
- [ ] **Rust移植の開始**
  - 今の37テスト(tests/)を「合格基準」にする
  - lexer → parser → checker → codegen の順に移植
  - Rust学習を兼ねる(所有権とASTの付き合い方が最初の山)

## 言語機能(中期)

- [ ] **struct のメソッド** — 言語カード実験(2026-07-18)で必要性が浮上。
      現状 struct は「データの形」だけで振る舞いを持てず、`fn complete(todos, id)` のように
      第1引数で受け渡す関数スタイルしか書けない。メソッド構文(例: `fn (t: Todo) render() string`)
      を入れるか、関数スタイルのまま貫くか要設計。P1(書き方は一つ)との兼ね合いも論点
- [ ] **標準ライブラリ** — 言語カード実験(2026-07-18)で必要性が浮上。第一弾・第二弾実装済み
      - [x] 配列/map操作: contains / indexOf(`int | none`)/ keys / values / sort(非破壊)— 2026-07-18
      - [x] 文字列操作: split / join / trim / upper / lower / toInt(`int | error`)— 2026-07-18
      - [ ] filter/map/reduce(高階関数。実タスクでの関数値の検証が先)
      - [ ] 層分け設計(core共通 / 環境別: http・json・file・DOM)は requirements C-6 / Q3 と統合して検討
- [ ] チャネルの容量指定 `chan<int>(0)` と同期(ブロックする)送信
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
