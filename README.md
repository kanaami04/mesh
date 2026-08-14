# Musubi (結び)

フロントエンドとバックエンドを1つの言語で「結ぶ」、人間に読みやすくAIが書きやすいプログラミング言語。

- **実装言語**: Rust
- **コンパイル方式**: JavaScriptへのトランスパイル(=JSソースコードへの変換。将来WASMバックエンド追加の余地を残す)
- **拡張子**: `.msb` / **CLI**: `musubi`

## 絶対に外せない軸

1. **FE + BE 両対応** — 1言語でブラウザUIとサーバを書ける
2. **人間に読みやすい** — Goのようなシンプルさ、一貫した文法
3. **AIに書きやすい** — 曖昧さのない文法、機械可読なエラー、局所で完結するコンテキスト

## リポジトリ構成

```
language/
├── README.md            # このファイル
├── CLAUDE.md            # AI協働のための作業ルール・正文書ルール
├── AGENTS.md            # → CLAUDE.md へのsymlink(AIツール向け業界標準の置き場所)
├── .claude/skills/      # リポジトリ手順のスキル化(adr, doc-review, drift-check, spec-review, spec-write, tdd)
├── docs/
│   ├── vision.md            # 要件定義・理想像・非目標
│   ├── roadmap.md           # フェーズ別ロードマップ
│   ├── testing-strategy.md  # テスト戦略
│   ├── style-guide.md       # 文章スタイルガイド(文書・出力・エラーメッセージ設計)
│   ├── learning/            # 言語の作り方 学習教材
│   ├── research/            # 他言語調査(MoonBit, Topcoat, Gleam...)
│   ├── spec/                # 言語仕様書
│   └── adr/                 # 意思決定記録(ADR)+ 前提台帳
├── crates/              # Rustワークスペース(実装開始時に作成)
├── examples/            # Musubiのサンプルコード(仕様確定後)
└── tests/               # conformanceテスト(仕様と1:1対応)
```

## いま何のフェーズか

**Phase 0: 資料整備**(実装前)。進捗と次の一手は [docs/roadmap.md](docs/roadmap.md) を参照。

## 読み始める順番

1. [docs/vision.md](docs/vision.md) — 何を作るのか
2. [docs/learning/how-languages-work.md](docs/learning/how-languages-work.md) — 言語処理系の基礎知識
3. [docs/research/landscape.md](docs/research/landscape.md) — 競合・先行言語の調査
4. [docs/adr/](docs/adr/) — これまでの決定と、その前提
