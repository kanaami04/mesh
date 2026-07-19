# 引き継ぎ文書(2026-07-19時点)

> 別セッションに切り替える際の入口ドキュメント。ここを読めば、他のdocsのどこに何が
> 書いてあるかが分かる状態を目指す。詳細を重複させず、一次情報源への案内に徹する。

## このプロジェクトは何か

**Mesh** — 「TypeScriptの型 × Goのシンプルさ・並行処理」を持つ、JavaScriptにトランスパイルされる
新しいプログラミング言語。単なる「良い言語」ではなく、**「AIが書き、人間が読む」時代を前提に設計する**
のが核心コンセプト(要件定義 P1〜P6)。言語カード(`src/card.ts`)を渡せば、この会話を知らない
AIエージェントでもMeshのコードを書ける、という実証実験(`docs/card-experiments.md`)まで行った。

- GitHub: https://github.com/ryota-kanayama/mesh(公開。featureブランチ→PR→CI green確認→
  squash mergeの運用。2026-07-19からこの形。それ以前はmain直push)
- ローカル: `/Users/kanayama/kanaami/language`
- 実装言語: TypeScript(v0)。将来Rust移植構想あり(未着手)
- ユーザー(kanayamaさん)はコードを書かない。Claudeが実装しながら日本語で解説する学習スタイル
  ([[user-collaboration-style]] メモリ参照)

## 読む順番(新しいセッションはここから)

1. **README.md** — 言語仕様の外向けまとめ・チュートリアル・組み込み関数表
2. **docs/requirements.md** — 要件定義書。P1〜P6の設計原則、なぜこの言語を作るのか
3. **docs/features.md** — 「できる・できない表」。**現在地の一次情報源**。迷ったらまずここ
4. **docs/design-agenda.md** — 討議中/決着済みの設計論点(B-1〜B-5, C-1〜C-9, E-1〜E-2)
5. **todo.md** — 次にやることリスト。これも一次情報源
6. **src/card.ts** — 言語カード本体。`bun run mesh card` で出力。AIにMeshを書かせる際に渡す
7. **docs/syntax-proposals.md** — 構文採用/不採用会の決定記録(経緯。凍結済み)
8. **docs/card-experiments.md** — 言語カード実証実験のログ(白紙AIに実タスクを書かせて穴を探す手法)
9. 永続メモリ([[language-project-goal]] [[user-collaboration-style]] [[mesh-card-experiment]]) —
   `/Users/kanayama/.claude/projects/-Users-kanayama-kanaami-language/memory/` にあるセッション横断の記憶

## 現在の実装状況(要約。詳細は必ず features.md を見る — ここは古くなりうる)

- コンパイラパイプライン: lexer → parser → checker → codegen(TS製、ランタイム依存ゼロ)
- **背骨(union路線)**: `T | none` / `T | error` + `is` narrowing + `match`式(網羅性検査つき)。
  null無し・多値戻り無し・例外無し。文字列リテラル型 + `type` 宣言
- **struct**: `struct User { name: string }`。Goスタイルのレシーバ構文でメソッド定義可
  (`fn (u: User) describe() string {...}`)。メソッドの名前空間は自由関数と完全分離
- **標準ライブラリ3弾**: 配列/map操作(contains/indexOf/keys/values/sort)、文字列操作
  (split/join/trim/upper/lower/toInt)、高階関数(filter/transform/reduce。`map`は型キーワードと
  衝突するため使えず`transform`)
- **構造化並行(2段スコープ)**: `spawn`=囲む関数が所有(関数を抜けるとき暗黙wait、リーク不可能)、
  `detach`=プログラムが所有(バックグラウンドタスク用エスケープハッチ)、`wait`ブロック(早期待機)
- **channel仕様完成**: 容量指定(`chan<T>(n)`、Go互換の本物のブロッキング送信)、
  `close(ch)` + `<-ch`は常に`T | closed`、`select`式(matchの見た目を踏襲した独立構文)
- **ツール**: CLI(`mesh run/build/check/card`)、`mesh check --json`(AIエージェント向け構造化診断)、
  ブラウザプレイグラウンド(`mise run playground`)、GitHub Actions CI(push毎にtsc+test+examples)
- **判別可能union(discriminated union)完成**(2026-07-19実装、design-agenda **C-1**): `type X =
  { kind: "ok", user: User } | { kind: "notFound" }`。union内だけで無名`{...}`型式が書ける。
  構築は union自身の名前をstructリテラル名に流用(`X{kind: "ok", ...}`)、matchは部分構造
  パターン`{kind: "ok"}`で絞り込み。この実装のため struct の同一性を名前ベース→全面的な
  構造的比較に変更(構造的型付けの前倒し実装)。**自己参照(木構造・AST等)も同日中に追加実装**
  ——structフィールド越しの参照(`{kind:"node", left: Tree, right: Tree}`)なら知恵の輪
  (knot-tying)で解決できる。structを挟まない「union同士が裸で直接参照し合う」形だけは
  意図的に`type alias cycle`のまま(下記参照)
- **言語カード実証実験 第5〜10回**(2026-07-19)実施。実装バグ6件発見・解消
  (chan配列`chan<int>[]`・複数行配列リテラル・`eval`/`arguments`予約語漏れ等)。
  詳細は docs/card-experiments.md
- **モジュールシステム(import / export)**(2026-07-19実装、C-6の土台): パッケージ=ディレクトリ
  (Go風・package宣言なし)、`export`可視性、`pkg.symbol`修飾アクセス(型位置・structリテラルも)。
  コンパイラはfs非依存の`compileModules`(ソース列を受ける)で、ファイル読み込みはCLIの仕事。
  環境判別は「importしたモジュールから自動推定」方式に決定済み(実装は環境別stdlibとセットで次段階)。
  v1制限: エントリは1ファイル・パスは単一セグメント・`mesh/...`予約。詳細は features.md
- **テスト277件**(2026-07-19時点。F-6のnarrowing拡張・F-1後半のジェネリクステスト込み。
  `bun test`で最新件数を確認)、CI green(直近コミットは`git log origin/main -1`で確認)

- **言語批評ターン**(2026-07-19)実施: 言語設計者レンズ+実務レンズの2独立サブエージェント
  (実機検証つき)+内部批評の3視点。全記録は docs/critique-2026-07.md。
  再討議項目は design-agenda.md の **F節**(F-1〜F-15)に整理済み

## 次にやるとしたら(未着手のトピック)

- **批評起点のF節**(design-agenda参照)が最優先。推奨順:
  ~~F-1 関数型注釈+ジェネリクス(前半・後半とも)~~・~~F-2前半 `? "文脈"`~~・~~F-3 名前的へ巻き戻し~~・
  ~~F-4 `?`改名~~・~~F-5 or束縛形~~・~~F-6 narrowing拡張(フィールドパス・`&&`・`!`/`||`)~~
  (すべて✅ 2026-07-19)
  → F-2後半 構造化エラー → F-12 対TS/Go比較ベンチ
- **C-6の続き**: 環境別標準ライブラリ(`mesh/http`・`mesh/dom`等)の中身の設計(=F-14)。
  Q3(フロントエンドの形)の討議とセット。先行依存だったF-1は完了、着手可能
- その他: defer文、エラー表示改善(ソース行表示・複数エラー報告)、mesh fmt、Rust移植、
  npm相互運用(Q2)

## 開発の進め方(重要な合意事項 — 必ず守る)

- **段階的に進める**。大きな機能を一気に実装せず「説明→小さく実装→動かして確認→次へ」。
  過去に一度「一気に実装しすぎた」とフィードバックを受けている
- **設計判断は先に討議・決定してから実装する**。特に既存構文と衝突する可能性がある場合は、
  必ず複数の選択肢とトレードオフを具体的なMeshコード例つきで提示し、`AskUserQuestion`で確認する
  (このセッションでは `map`名の衝突、channel容量、structメソッド構文などをこの形で決めてきた)
- **実装したら必ず一通り検証してからコミットする**(2026-07-19からPRフロー):
  1. `bun test` → 全パス確認
  2. `bunx tsc --noEmit` → 型エラーなし確認
  3. プレイグラウンド(`mise run playground`)で実際に動かして目視確認
  4. ドキュメント更新: `src/card.ts`(言語カード)/ `docs/features.md` / `todo.md` / `README.md`
  5. featureブランチに `git add -A && git commit`(決定の経緯・却下した代替案もメッセージに書く)
     → `git push` → `gh pr create`
  6. `gh pr checks <番号> --watch` でCI green確認 → `gh pr merge <番号> --squash`
     → ローカルブランチを `git rebase origin/main` で追従(squash mergeで履歴が分岐するため)
- **無関係な変更は別コミットに分ける**(例: MoonBit調査ドキュメントと機能実装を分けてコミットした)
- 大きな機能追加後は既存の`<-ch`等の使用箇所が壊れていないか`bun test`で確認し、
  壊れていたら**個別に narrowing を足して直す**(型を緩めて回避しない)

## 実行コマンド

```sh
mise run playground     # プレイグラウンド http://localhost:8765(main.tsをその場でバンドル)
mise run test           # = bun test
mise run check          # = bunx tsc --noEmit
mise run run-examples   # examples/*.mesh を全部実行

bun run mesh run   <file.mesh>          # コンパイルして即実行
bun run mesh build <file.mesh> -o out   # JSを書き出す
bun run mesh check <file.mesh> [--json] # 型検査のみ
bun run mesh card                       # 言語カードを出力
```

## 用語集(初見だと分かりにくい決定)

- **2段スコープ**: `spawn`=関数所有(関数を抜けるとき暗黙にwait)、`detach`=プログラム所有
  (呼び出し元は待たずに戻れる)。goroutineリークが構文的に存在できない設計
- **P1〜P6**: requirements.mdの設計原則。P1書き方は一つ、P2暗黙より明示、P3ローカルで読める、
  P4コンパイラはAIの相棒(機械可読エラー)、P5新規性予算、P6フルスタック一体
- **union路線**: 「不在(`none`)・失敗(`error`)・close(`closed`)は全部union型+narrowingで表現する」
  という言語の背骨の決定。null無し、多値戻り無し
- **言語カード**: `src/card.ts`。AIのコンテキストに貼る前提で設計された圧縮仕様書。
  「存在しない機能」リストと「よくあるエラー→直し方」が主役。カードの主張はテストで実装と
  突き合わせている(`tests/e2e.test.ts`の「カードの新項目」テスト群)ので、乖離するとCIが落ちる
