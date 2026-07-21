# 引き継ぎ文書(2026-07-21時点)

> 別セッションに切り替える際の入口ドキュメント。ここを読めば、他のdocsのどこに何が
> 書いてあるかが分かる状態を目指す。詳細を重複させず、一次情報源への案内に徹する。

## このプロジェクトは何か

**Mesh** — 「TypeScriptの型 × Goのシンプルさ・並行処理」を持つ、JavaScriptにトランスパイルされる
新しいプログラミング言語。単なる「良い言語」ではなく、**「AIが書き、人間が読む」時代を前提に設計する**
のが核心コンセプト(要件定義 P1〜P6)。言語カード(`src/card.ts`)を渡せば、この会話を知らない
AIエージェントでもMeshのコードを書ける、という実証実験(`docs/card-experiments.md`)まで行った。

- GitHub: https://github.com/kanaami04/mesh(公開。featureブランチ→PR→CI green確認と
  `/code-review --comment`(順不同・並行可)→両方揃ったらsquash mergeの運用。2026-07-19から
  PRフロー、2026-07-21から`/code-review`必須化〔`gh pr merge`実行時に2つのフックが機械チェックする:
  `.claude/hooks/enforce-code-review.sh`がレビューコメント(`### Code review`見出し)の有無を、
  `enforce-squash-merge.sh`が`--squash`の有無を確認し、欠けていればdenyする。
  2026-07-21の#2で`.claude/`をgit管理下に入れたので、設定自体はcloneすれば付いてくる。
  ただし**フックが動く条件は各マシンで揃える必要がある**(`jq`・`gh`・`/code-review`
  プラグイン本体)——詳細と、揃っていないと何が起きるかは docs/setup.md〕。
  それ以前はmain直push)
- **PR番号について**: 2026-07-21に`ryota-kanayama/mesh`から現リポジトリへ移管し、旧リポジトリは
  削除した。PR番号は1から振り直されているので、**#40以前の番号は旧リポジトリのもので現在は無効**
  (書くとGitHubが現リポジトリの別PRへ誤リンクする)。過去の作業を指すときはコミットSHAを使うこと
- 環境構築: **docs/setup.md が一次情報源**(mise・system パッケージ・認証・プラグイン)。
  作業ディレクトリは固定していない
- 実装言語: TypeScript(v0、本番)。**2026-07-21からRust移植が進行中**(`rust/`、
  lexer+parser一部まで完了。詳細は下記「Rust移植の現状」節)
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
   セッション横断の記憶。ただし**マシンごとに独立していて共有されない**ので、
   別マシンでは読めないことがある(上記3件は Mac 側の
   `~/.claude/projects/.../memory/` にある)。全員に届けたい内容はメモリではなく
   このリポジトリのドキュメントに書くこと

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
  パターン`{kind: "ok"}`で絞り込み。**訂正(2026-07-21、調査で発覚)**: この実装のため一時的に
  structの同一性を名前ベース→全面的な構造的比較に変更したが、同日中のF-3で名前的型付けへ
  巻き戻された——名前付きstruct同士は名前で判定し(`Meters`と`Dollars`は別型)、無名`{...}`型式
  (判別可能unionメンバー)が絡む比較だけ構造的、が最終的な確定仕様(`src/types.ts`の
  `typeEquals`参照)。**自己参照(木構造・AST等)も同日中に追加実装**
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
- **テスト477件**(2026-07-21時点。`bun test`で最新件数を必ず確認 — この行はすぐ古くなる)、CI green
  (直近コミットは`git log origin/main -1`で確認)

- **言語批評ターン**(2026-07-19)実施: 言語設計者レンズ+実務レンズの2独立サブエージェント
  (実機検証つき)+内部批評の3視点。全記録は docs/critique-2026-07.md。
  再討議項目は design-agenda.md の **F節**(F-1〜F-15、2026-07-20までに全項目決着・実装済み)

- **2026-07-19以降に完了した主なもの**(詳細はfeatures.md/todo.mdの一次情報源を見ること):
  F節(F-1〜F-15)全項目・ベンチマーク第1/第2ラウンド・`mesh fmt`・エラー表示改善
  (ソース行表示・複数エラー報告への復帰)・VS Code拡張・**H節**(H-1: `any`型の完全撤去、
  H-2: `mesh/json`ヘルパー+`json struct`自動デコード)・**C-6続き**(`mesh/http` v1、
  サーバー専用の生ハンドラ+障害分離)。詳細は design-agenda.md H節・I節を参照

## Rust移植の現状(2026-07-21、todo.md「Rust移植の開始」参照)

TS実装(477テスト)はそのまま本番として動き続けており、Rust版は**並行して**
`rust/`ディレクトリにゼロから育てている(TSを書き換えているわけではない)。
進め方はTS実装と同じくClaudeが実装+日本語で解説するスタイル(kanayamaと確認済み)。

- **アーキテクチャ**: `rust/src/token.rs`(Pos/TokenType/Token/CompileError)・
  `lexer.rs`・`ast.rs`・`parser.rs`。lib+binハイブリッドのCargoプロジェクト
  (`cargo run -- file.mesh`でトークン列/ASTを表示するだけの疎通確認CLI。
  checker/codegenが無いのでまだ`mesh run`相当にはなっていない)
- **進捗(古い順。番号ではなくSHAで示す — 下記「PR番号について」参照)**:
  `fffd0d9` lexer全体(TS 393行→Rust、テスト15件)・
  `3ac059a` parser核サブセット(fn宣言・if/for・変数宣言・二項演算子・関数呼び出し、
  エラー復帰の枠組みをフル移植)・`207802e` struct/type宣言+判別可能union+match/is式。
  現在テスト41件(lexer 15+parser 26)、`cargo clippy --all-targets -- -D warnings`
  クリーン
- **対象外(未着手)**: ジェネリクス・レシーバ(メソッド)・error/jsonマーカー(`?`/`or`が
  無いと構造化エラーの旨みが薄いためセット予定)・spawn/wait/chan/select・
  **文字列補間**・配列/mapリテラル・import/export。対象外の構文は誠実に構文エラーで
  失敗する(クラッシュしない)よう作ってある
- **examples/\*.meshでの進捗確認**: 全13本中mathutil系2本を除いた11本のうち4本
  (`hello.mesh`・`fizzbuzz.mesh`・`status.mesh`・`tree.mesh`)が完全にパース成功。
  `discriminated_union.mesh`/`users.mesh`はstruct/union/match/isを全部通過し、
  **文字列補間だけ**で止まることを確認済み——次に文字列補間を実装すればこの2本も
  通る見込みが高い、という具体的な足がかりがある
- **次にやるなら**: 文字列補間(再字句解析が絡むので複雑——`src/lexer.ts`の
  `StringPart`/`t.parts`と`src/parser.ts`の`parsePrimary`内で`lex(p.source, p.pos)`を
  再帰的に呼ぶ部分を参照)。その後はspawn/wait/chan/select、import/export、
  ジェネリクス、error/json構造化エラーと続く見込み(todo.mdに書いていないだけで
  まだ相当量残っている——parser.ts全体は1217行、現状のRust版はその半分強程度)
- **今回の設計判断**(詳細はtodo.mdの各マイルストーン項目に書いてある。ここは要約のみ):
  `CompileError`を`Box`で包む(clippy::result_large_err対策)/
  TS の`CompileError`↔`MultiCompileError`の型分けは`Vec<CompileError>`に統一/
  二項演算子等はlexerの`TokenType`をそのまま流用/ `allow_struct_lit`フラグは
  `with_struct_lit_flag`という「必ず戻す」ヘルパー経由でしか触らない(code reviewで
  1回踏んだ罠——早期returnで復元をすり抜けるパターンを避ける)
- **教訓(milestone 3で発覚)**: 文字列リテラル型(`TypeNode::Literal`)と値としての
  `none`(`Expr::None`)を最初のスコープ見積もりで見落としていた——「実際に典型的な
  コード片を1つ最後まで組んでみる」まで気づけなかった。次のマイルストーンでも
  スコープを決めたら早めに実例(discriminated_union.mesh相当)で組んでみること
- **開発環境**: `mise.toml`に`rust = "1.97.1"`を追加済みなので`mise install`で入る
  (セットアップ全般は docs/setup.md)。CIには`rust-test`ジョブ(build+clippy+test)を新設済み

## 次にやるとしたら(Rust移植以外で未着手のトピック)

todo.md「次の一手」に列挙された討議項目(F節・H節・C-6コア+`mesh/http` v1)は
2026-07-21時点ですべて決着・実装済み。Rust移植は上記の通り進行中。それ以外の候補
(todo.md記載順):

- **言語カード実証実験の継続**(docs/card-experiments.md): 第11回まで実施済み。
  単体機能の検証はほぼ出尽くしたため、次はより大規模な複合タスクでの再測定
- 保留中の未決事項: Q2(npm相互運用の深さ)、Q3(フロントエンドの形。`mesh/dom`の中身と
  環境自動推定の実装はこれとセット)、E-2(スナップショットテストの採否)

## 開発の進め方(重要な合意事項 — 必ず守る)

- **段階的に進める**。大きな機能を一気に実装せず「説明→小さく実装→動かして確認→次へ」。
  過去に一度「一気に実装しすぎた」とフィードバックを受けている
- **設計判断は先に討議・決定してから実装する**。特に既存構文と衝突する可能性がある場合は、
  必ず複数の選択肢とトレードオフを具体的なMeshコード例つきで提示し、`AskUserQuestion`で確認する
  (このセッションでは `map`名の衝突、channel容量、structメソッド構文などをこの形で決めてきた)
- **実装したら必ず一通り検証してからコミットする**(2026-07-19からPRフロー、2026-07-21から
  `/code-review`必須化):
  1. `bun test` → 全パス確認
  2. `bunx tsc --noEmit` → 型エラーなし確認
  3. プレイグラウンド(`mise run playground`)で実際に動かして目視確認
  4. ドキュメント更新: `src/card.ts`(言語カード)/ `docs/features.md` / `todo.md` / `docs/design-agenda.md`
  5. featureブランチに `git add -A && git commit`(決定の経緯・却下した代替案もメッセージに書く)
     → `git push` → `gh pr create`
  6. 次の2つを並行して進める(順不同): `gh pr checks <番号> --watch` でCI green確認、
     **`/code-review <番号> --comment` を実行**(PRにレビューコメントを投稿。指摘があれば
     対応してコミットを追加し、CIとレビューをやり直す)——`.claude/hooks/enforce-code-review.sh`が
     このコメントの有無を`gh pr merge`実行時に機械チェックし、無ければ拒否する
  7. 両方(CI green・レビューコメント)が揃ったら `gh pr merge <番号> --squash --delete-branch`
     → ローカルは
     `git checkout main && git fetch --prune origin && git merge --ff-only origin/main` で同期
     (featureブランチはリモート側で自動削除されるので、ローカルでrebaseして使い回す必要はない)
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

# Rust移植版(rust/) — 動かない場合はセットアップを docs/setup.md で確認
mise run rust-test      # = cd rust && cargo test
mise run rust-check     # = cd rust && cargo clippy --all-targets
(cd rust && cargo run -- ../examples/hello.mesh)   # トークン/AST疎通確認CLI
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
