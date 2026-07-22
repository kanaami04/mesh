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
  PRフロー、2026-07-21から`/code-review`必須化〔`gh pr merge`実行時に
  `.claude/hooks/enforce-code-review.sh`がレビューコメント(`### Code review`見出し)の
  有無を機械チェックし、欠けていればdenyする。squash統一はリポジトリ設定
  (merge commit・rebase mergeを無効化)でサーバ側に強制(経緯は`13405bf`参照)。
  フックが動く条件(`jq`・`gh`・`grep`・`/code-review`プラグイン本体)は各マシンで
  揃える必要がある——詳細は docs/setup.md〕。それ以前はmain直push)
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

## Rust移植の現状(2026-07-22、todo.md「Rust移植の開始」参照)

TS実装(477テスト)はそのまま本番として動き続けており、Rust版は**並行して**
`rust/`ディレクトリにゼロから育てている(TSを書き換えているわけではない)。
進め方はTS実装と同じくClaudeが実装+日本語で解説するスタイル(kanayamaと確認済み)。

- **アーキテクチャ**: `rust/src/token.rs`(Pos/TokenType/Token/CompileError)・
  `lexer.rs`・`ast.rs`・`parser.rs`。lib+binハイブリッドのCargoプロジェクト
  (`cargo run -- file.mesh`でトークン列/ASTを表示するだけの疎通確認CLI。
  checker/codegenが無いのでまだ`mesh run`相当にはなっていない)
- **進捗概要(詳細は todo.md の各マイルストーン項目が一次情報源。ここは要約のみ)**:
  lexer全体(`fffd0d9`、テスト15件)→ parser核サブセット(fn宣言・if/for・変数宣言・
  二項演算子・関数呼び出し、エラー復帰の枠組み)→ struct/type宣言+判別可能union+match/is式・
  文字列補間 → 文字列補間まわりのcode review指摘3件(スタックオーバーフロー対策の
  `interp_depth`カウンタ・エラーメッセージの`describe_token`統一・引用符エスケープ等。
  PR #5〜#7)。ここまでで**着手当初に挙がった4候補が全て完了**:
  並行処理(`chan<T>`型・`spawn`/`detach`/`wait`/`send`/`recv`/`select`式。PR #8)→
  error/json構造化エラー(`?`伝播式・`or`束縛形。PR #9)→
  import/export(`import "path"`宣言。`export`修飾自体は以前から実装済みと判明。実例を
  最後まで組んで見つかったパッケージ修飾structリテラル・型注釈つき変数宣言も同PRで追加。
  PR #10)→ ジェネリクス+レシーバ(`fn first<T>(...)`型パラメータ・
  `fn (u: User) describe() ...`Goスタイルレシーバ。`export fn (u: T) ...`は
  `method-export-redundant`エラーに誘導。レシーバとgenericsは併用不可)。
  いずれもTS版(`parser.ts`)をほぼ1:1移植するだけで新しい設計判断は不要だった。
  テスト76件・`cargo clippy --all-targets -- -D warnings` クリーン・
  配列型`T[]`+配列リテラル+型付き配列リテラル(`Todo[]{}`等)+map型`map<K,V>`+mapリテラル+
  添字アクセス`a[i]`(代入先としても可)+範囲for(`for i, v := range arr`等)を追加
  (`maps.mesh`——examples/*.mesh 11本のうち唯一未対応だった1本——を最後まで通す一括りとして
  採用)。**実装中にスタックオーバーフローの実バグ1件を自己検証で発見・修正**(下記「教訓」参照)。
  現在テスト84件、`cargo clippy --all-targets -- -D warnings` クリーン
- **対象外(未着手)**: `error struct`/`json struct`宣言マーカー(checkerが無いと
  `isError`/`isJson`フラグの使い道が無いため、checker移植まで後回し)・defer。
  対象外の構文は誠実に構文エラーで失敗する(クラッシュしない)よう作ってある
- **examples/\*.meshでの進捗確認**: **全13本(examples/*.mesh 11本 + mathutil系2本)が
  完全にパース成功**(2026-07-22時点)
- **次にやるなら**: 着手当初の4候補+配列/mapリテラル等も全て完了したので、残るのは
  defer・error/jsonマーカーという小さな2件を埋めるか、**checker/codegenの移植に進むか**の
  判断(パーサはparser.ts全体1217行の8割強まで到達)
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
- **教訓(milestone 9で発覚)**: `parse_primary`/`parse_postfix`は文字列補間の再帰パース
  (`interp_depth`で深さ制限)と同じ呼び出し経路に乗るため、これらの関数(および間接に
  呼ばれる`parse_unary`/`parse_binary`等)のスタックフレームサイズが`MAX_INTERP_DEPTH`
  (上限64)の安全マージンに直結する。新しい構文の局所変数(配列/mapリテラルの
  `elems`/`entries`等)をそのまま関数本体にインライン展開したところ、この安全マージンを
  食い潰して**回帰テストが本物のスタックオーバーフローで実際にクラッシュした**
  (code reviewではなく`cargo test`の自己検証で発見)。対応: 該当ロジックを専用の関数に
  切り出し、その分岐が実際に呼ばれたときだけ専用フレームが積まれる形にして解消。
  **今後、この2関数(および`parse_unary`)に新しい分岐を足すときは、局所変数が数個を超える
  ロジックは必ず別関数に切り出すこと**。テストで検知できることは確認済みだが、
  安全マージンそのものは今回で使い切った可能性があるため、次に何か追加する際は
  `cargo test`(特に`文字列補間_上限を超えるネストは...`)を必ず確認すること
- **開発環境**: Rustのバージョンは`mise.toml`で固定済みなので`mise install`で入る
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
- **`/code-review`実行時の注意(2026-07-22、PR #10で発覚)**: レビュー観点ごとに立てる
  調査サブエージェントは(明示的に読み取り専用のagent typeを指定しない限り)フルツール
  アクセスを持ち、実際にPR #10のレビュー中に1体が調査目的で`rust/src/parser.rs`へ
  デバッグ用テスト関数を一時的に書き込む場面があった(コミット前に気づいて削除、実害は
  無かった)。同じファイルを自分も編集中にサブエージェントを並列起動すると衝突しうるので、
  次回以降は調査系サブエージェント(Agent#1〜#5)には読み取り専用のagent type
  (例: `Explore`)を指定するか、少なくとも実装中のファイルへの並列レビュー起動を避けること

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
