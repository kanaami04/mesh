# 引き継ぎ文書(2026-07-22時点)

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
  `lexer.rs`・`ast.rs`・`parser.rs`・`types.rs`・`checker.rs`・`codegen.rs`。
  lib+binハイブリッドのCargoプロジェクト(`cargo run -- file.mesh`でASTを表示、
  `cargo run -- file.mesh --emit-js`で生成JSを標準出力へ書き出す)
- **`parser.ts`(1217行)を全面移植完了(2026-07-22)**。詳細は todo.md の各マイルストーン
  項目が一次情報源(ここは要約のみ): lexer全体(`fffd0d9`)→ parser核サブセット→
  struct/type宣言+判別可能union+match/is式・文字列補間(+スタックオーバーフロー対策の
  `interp_depth`カウンタ等、code review指摘3件。PR #5〜#7)→ 並行処理(PR #8)→
  error/json構造化エラー`?`/`or`(PR #9)→ import/export(PR #10)→ ジェネリクス+レシーバ
  (PR #12)→ 配列/mapリテラル+添字アクセス+範囲for(実装中に**本物のスタックオーバーフロー
  回帰を自己検証で発見・修正**——詳細は下記「教訓」。PR #13)→ defer文+error/jsonマーカー
  (PR #14)→ 関数型注釈`fn(int,string) bool`+無名関数式`fn(x: int) int {...}`(PR #15。
  milestone 10のスコープ調査で発覚した最後の1件。milestone 9の教訓を踏まえ実装直後に
  スタックオーバーフロー回帰テストを5回実行して安全マージンを確認済み)。
  いずれもTS版(`parser.ts`)をほぼ1:1移植するだけで新しい設計判断は不要だった。
  **対象外の構文は無い状態**。parser自体のテストは92件
- **examples/\*.meshでの進捗確認(パース)**: **全13本(examples/*.mesh 11本 + mathutil系2本)が
  完全にパース成功**(2026-07-22時点)
- **checker(最小リゾルバ)+codegen milestone 1 完了(2026-07-22、PR #16)**。フルchecker
  (約2900行)ではなく、codegenが必要とする最小限の型情報だけを解決する「最小リゾルバ」
  (`checker.rs`。診断は出さない)を先に作り、`types.rs`(型システム)+`codegen.rs`
  (struct/map/channel/エラー伝播/パッケージ抜きの「スカラーのMesh」)とセットで移植した。
  **`examples/hello.mesh`/`examples/fizzbuzz.mesh`をRust版で実行し、生成JSを`bun`で走らせて
  TS版と標準出力が完全一致することを確認済み**——パーサのみだった今までと違い、
  初めて「本当に動く」ところまで検証できた。設計判断の詳細はtodo.mdの当該項目が一次情報源
  (ここは要約): (1) resolverとcodegenを1回のトラバーサルに融合(TS版の
  ASTミュータブル書き込み方式はRustの不変`Expr`に向かないため)。(2) `src/runtime.ts`は
  TSファイル(`export const PRELUDE = \`...\`;`というテンプレートリテラルでランタイムJSを
  包んでいる)なので、`include_str!`でファイル全体を素朴に埋め込むとTSの宣言構文まで
  生成JSに混入する実バグを発見・修正(バッククォート2箇所の間だけを切り出す方式)。
  現在テスト133件(PR #16のcode reviewで見つかった実バグ2件——組み込み関数の引数不足パニック・
  `round`/`floor`/`ceil`/`toInt`の戻り値型未解決による誤った浮動小数点演算——を同PR内で
  修正した際に+3件)、`cargo clippy --all-targets -- -D warnings` クリーン。
  **スコープ外(milestone 2以降)**: struct/メソッド・配列/map・並行処理・`?`/`or`・
  import/export・ジェネリクス——パーサは既にパースできるが、codegenは明確な
  「まだ対応していません」エラーを返す
- **checker+codegen milestone 2(struct宣言+レシーバメソッド)完了(2026-07-22)**。
  TS版のknot-tying(structを「空fieldsの殻を先に作りあとから埋める」ことで自己参照型を
  表現する手法)はRustの所有権ベースの木に向かないため、**固定点反復**(`N=types.len()`回、
  現時点のレジストリを使って全struct宣言を再解決)で置き換えた——非循環なら宣言順に
  関係なく収束する。ただし循環(自己参照含む)は固定点反復では「クラッシュしないが
  中途半端な入れ子になる」だけなので、生のTypeNode参照関係を見た軽量なDFSサイクル検出を
  別途挟み、循環があれば明確な`Err`にしている(`types.rs`が謳う「自己参照は未対応」という
  前提を実装のズレで裏切らないため)。`checker.rs`に`struct_types`/`method_table`(フィールド
  vs メソッドの判別は「フィールドが勝つ」——TS版`calls.ts`と同じ順序)を追加、`codegen.rs`に
  struct literal・フィールド読み書き(新設`gen_lvalue`。`Stmt::Assign`/`IncDec`を
  `Expr::Member`ターゲットにも対応)・メソッド呼び出し(`__m_Struct_method(recv, args)`)を
  追加。**`__proto__`ガード**: TS版が過去に踏んだprototype汚染バグの再発防止として、
  struct literalのフィールド名・代入先のフィールド名の両方で明確な`Err`にした(後者は
  milestone 2で新設したフィールド書き込み機能に伴う、TS版には無かった新しい攻撃面)。
  **`examples/struct_methods.mesh`を新規作成し実行確認**(README記載の`Todo`例の
  生成直後リテラルへの直接メソッドチェーン込み)——生成JSを`bun`で走らせてTS版と
  標準出力が完全一致。現在テスト146件、`cargo clippy --all-targets -- -D warnings`クリーン。
  詳細はtodo.mdの当該項目が一次情報源
- **既知の限界(PR #17のcode reviewで発覚、未修正のまま記録・2026-07-22)**: struct
  literalのフィールド名/値は宣言済みfieldsと照合されない(タイポ・欠落・型不一致が
  無診断でコンパイルされ、実行時に`undefined`や紛らわしいpanicになりうる)/ 代入先
  (`gen_lvalue`)はフィールド名を検証しない(read/callパスは検証するのに書き込み側だけ
  非対称)/ `__proto__`ガードはstruct宣言時点には無い(literal/代入先の2箇所のみ——
  `struct Evil { __proto__: string }`自体は宣言できてしまう)。いずれも独立検証で
  80点未満(75・75・25)と判定され、「診断は出さない」という本リゾルバの既定方針と
  整合的と判断してブロックせず記録のみに留めた(kanayama確認済み)。error/json以降で
  診断機構を入れる際にまとめて対応する候補
- **checker+codegen milestone 3(`?`/`or`/`error struct`)完了(2026-07-22)**。
  途中でmilestone 2の実バグ(`resolve_struct_decls`が`error struct`宣言を丸ごと無視
  していた——フィルタの`!t.is_error`条件を削るだけで修正、struct構築コード自体は
  既に`is_error_type`を正しく渡していた)を発見・修正。`checker.rs`に`is_failure_type`/
  `or_binding_type`(TS版の「unionでない被演算子は無条件でANY」という実際の挙動も
  忠実に踏襲)/`has_structured_failure`(**Rust版だけの追加ガード**——ランタイムの
  `__propCtx`が構造化errorを処理できないため、TS版の診断より意図的に広く取る)を追加。
  `gen_fn_decl`を「本体を生成してから`?`使用有無でtry/catch包みを事後に決める」形に
  書き換えた(TS版`genFnBody`の`propStack`と同じ設計、`Expr::FnExpr`未対応のため
  スタックではなく単一フラグで足りる)。**`examples/error_propagation.mesh`を新規作成し
  実行確認**——生成JSを`bun`で走らせてTS版と標準出力が完全一致。現在テスト162件、
  `cargo clippy --all-targets -- -D warnings`クリーン。**検証で踏んだ新しい罠**: TS版は
  `or`のfallback式の型を成功側の残り型と照合する(`or-fallback-type-mismatch`)ため、
  診断を出さないRust版なら通る組み合わせを書くとTS版側でコンパイルエラーになり
  比較できない——example作成時は必ずTS版でも成立する組み合わせにすること。
  **既知の限界(PR #18のcode reviewで発覚)**: PR #17の「struct literalの名前/フィールドが
  宣言と照合されない」という既知の限界が、`?`が入ったことでより深刻な形で現れる——
  struct名をタイポすると`is_error_type`が静かにfalseへフォールバックし、
  `__errTag`が付かないため`?`が失敗値を「成功」として素通りさせてしまう
  (独立検証で75点、80点未満でブロック対象外——PR #17の限界の新しい現れ方であり
  このPR自体の新しいバグではない、という判断)。詳細はtodo.mdの当該項目が一次情報源
- **checker+codegen milestone 4(配列/map)完了(2026-07-22)**。配列/mapリテラル・
  添字読み書き・範囲for(3形態)・配列/map対応の組み込み(`push`/`get`/`contains`/
  `indexOf`/`sort`/`len`/`delete`/`keys`/`values`)を実装。`filter`/`map`/`reduce`は
  無名関数(`Expr::FnExpr`)のcodegenがまだ無く対象外のまま。**Rust版だけの安全ガード
  3件**(milestone 2/3と同じ考え方——TS版では診断のおかげで到達不能な組み合わせを、
  診断を出さないこの設計では明確なErrで守る): mapへの複合代入・mapへのIncDec・
  明確な形のsubjectに対するrange-forのアリティ不一致。いずれもTS版のcodegen自体は
  無条件分岐でこれらを素通しするが、TS本体の別の診断(`isNumeric`/`range-arity`)の
  おかげで実際には到達しないコード——診断を出さないRust版では実際に到達しうるため
  明確なErrにした。意図的なスコープ縮小として、`gen_lvalue`自体にはIndexアームを
  追加せず(forヘッダ内の添字代入は明確なErrのまま——TS版の`genLValue`はここで
  mapに`.set`を呼ばない壊れた形を素通しするが、これは意図的に移植しなかった)。
  **`examples/collections.mesh`を新規作成し実行確認**(既存の`examples/maps.mesh`は
  `is none`を使うため変更せず、そのままだと明確な「未対応」エラーになることを確認)——
  生成JSを`bun`で走らせてTS版と標準出力が完全一致。詳細はtodo.mdの当該項目が
  一次情報源
- **PR #19の5エージェントcode reviewで2件のバグを発見・PR内で修正済み(2026-07-23)**:
  (1) `delete()`をmap以外に呼ぶと存在しないメソッドを無条件生成しクラッシュするバグ、
  (2) ネストしたmap(`map<K, map<K2,V2>>`)への二重添字が、milestone 4で追加した
  安全ガードやMap/Array判定そのものをすり抜けるバグ(`m["a"]`の型が厳密な`Type::Map`
  ではなく`V | none`の`Union`になるため)——TS版のcheckerが実はUnion型への添字自体を
  `not-indexable`診断で拒否していると判明したため、Rust版でも添字の読み・代入・
  複合代入・IncDecの4箇所でUnion containerを明確なErrにする形で修正。他に3件
  (配列/mapリテラルの相互検証なし・`gen_lvalue`のMember/Index非対称・range-forの
  アリティガードがstring/bool/struct等をカバーしない)は独立検証で75点(80点未満)
  につき既知の限界として明記するに留めた。現在テスト182件、
  `cargo clippy --all-targets -- -D warnings`クリーン。詳細はtodo.mdの当該項目が
  一次情報源
- **checker+codegen milestone 5(並行処理)完了(2026-07-23)**。`chan`/`spawn`/`detach`/
  `wait`/`select`/`<-`(recv)/`ch <- v`(send)を実装。パーサー・型システム
  (`Type::Chan`/`Type::Closed`)・ランタイム(`__Channel`/`__recv`/`__select`/`__spawn`/
  `__detach`/`__waitStack`)は既存の仕組み(TS版`runtime.ts`を`include_str!`で丸ごと
  埋め込み)で既に揃っていたため、`checker.rs`の式推論と`codegen.rs`のみが対象。
  `gen_fn_decl`を`prop_used`/`spawn_used`の2フラグ合成に書き換え(neither/propのみ/
  spawnのみ/両方の4通りでtry/catch/finallyを正しく組み立てる——TS版`genFnBody`の
  prop/spawn/defer 3フラグ合成と同じ設計。`defer`は`Stmt::DeferStmt`が常にErrを返す
  ため対象外のまま)。`gen_call`のメソッド呼び出し判定を`resolve_method_target`
  ヘルパへ切り出し`gen_spawn`と共有(TS版はこの判定を2箇所に重複して持つ)。**Rust版
  だけの安全ガード**(TS版の`not-a-channel`診断に相当): send/recv/select各アームの
  channelが確実に非chan/非anyだと分かる場合は明確なErr——milestone 4のIndexの前例
  (実装コストが高く見送った)とは違い今回は低コストなので新規構文から最初に付けた。
  新規`examples/concurrency.mesh`を作成し実行確認、既存`examples/channels.mesh`も
  今回からフルに動くことを確認、`examples/channel_spec.mesh`(`is closed`使用)は
  引き続き明確な「未対応」エラーになることを確認。詳細はtodo.mdの当該項目が
  一次情報源
- **PR #20の5エージェントcode reviewで1件のバグを発見・PR内で修正済み(2026-07-23)**:
  過去PRコメントレビュー・git履歴レビュー・コードコメント準拠レビューの3エージェントが
  独立に(別々の切り口・再現コードで)同じ根本原因に到達したバグ——`checker.rs`の
  `Expr::Select`アームが束縛名をスコープに宣言せずbodyを推論していたため、
  (1) `v := <-ch => v`のような典型的なイディオムでselect式全体の型が誤ってANYに
  潰れ、既存/このPR自身の安全ガードをすり抜ける、(2) 束縛名が外側の型が違う変数を
  shadowすると外側の型が誤って漏れ、紛らわしい実行時パニックになる、という2つの
  実害を確認。`CheckerCtx`に`Clone`を追加し、アームごとに使い捨てスクラッチctxで
  束縛名を正しく宣言してから推論する形で修正(codegen側の`gen_select`は元々
  `&mut self.ctx`で正しく束縛していたため無修正)。他4件(range-forでのchan・
  `T|closed`への算術・chanへの添字・spawnでの組み込み関数呼び出し)は独立検証で
  80点未満(既存のPR #19限界の新しい現れ方、またはmilestone 1由来の既存の穴)
  につき既知の限界として明記するに留めた。現在テスト204件、
  `cargo clippy --all-targets -- -D warnings`クリーン。詳細はtodo.mdの当該項目が
  一次情報源
- **checker+codegen milestone 6(モジュール)完了(2026-07-23)**。複数ファイル
  コンパイル・`import`・パッケージ修飾参照(`mathutil.Point`・`mathutil.add(...)`等)を
  実装——これまでの5マイルストーンと違い、初めて構造そのもの(`main.rs`の単一ファイル
  前提・`CheckerCtx`の単一名前空間)を拡張した、この移植で最大の構造変更。新規
  `rust/src/modules.rs`(TS版`cli.ts`のloadModules/loadDependencies相当、ファイル発見)。
  `CheckerCtx`にパッケージレジストリ(`PackageSymbols`のtypes/fns/consts、パッケージ名で
  引く)を追加、struct名は`qualify_struct_name`で`pkg.Name`に修飾(TS版
  `types-resolve.ts`と同じ)。パッケージ間のstruct循環は構造的に起こり得ないため
  milestone 2の固定点反復はパッケージ内のみで済む。`codegen.rs`は`generate_all`を
  `generate_all_modules`(パッケージを依存順にトポロジカルソート、循環は明確なErr)+
  `generate_package`(1パッケージぶんの処理、ファイルごとに`self.file`を切り替えて
  パニック位置情報を正しく保つ)に分割。`generate(program, file)`は1パッケージのみの
  薄いラッパーになり既存220件近いテストは無変更で通る。新設`fn_js_name`
  (mainは無修飾、それ以外は`{pkg}$name`——TS版`fnJsName`と同じ)を自由関数/メソッド以外の
  命名に使用。**意図的なスコープ縮小**: 未export診断・パッケージ誤用診断・パッケージ
  修飾された値参照(呼び出しを伴わない)・パッケージ修飾レシーバ・exportedなconstの
  レジストリ登録は対象外。新設`modules.rs`5件+`checker.rs`4件+`codegen.rs`7件の
  テストを追加、`cargo clippy --all-targets -- -D warnings`クリーン。
  新設のマルチファイルエントリポイント経由で`examples/modules_demo.mesh`+
  `examples/mathutil/{ops,point}.mesh`を実行し`bun run mesh run`(TS版)とbyte-for-byte
  一致を確認、既存の全exampleも回帰無しを再確認。詳細はtodo.mdの当該項目が一次情報源
- **PR #21の5エージェントcode reviewで4件のバグを発見・PR内で修正済み(2026-07-23、
  全て実際のビルド+実行で再現確認済み)**: (1) `spawn`/`detach`でパッケージ修飾された
  自由関数呼び出しが解決できなかったバグ、(2) pkg修飾された型注釈の循環検出が素の
  名前だけを見ていたため同一パッケージ内の無関係なstructと誤って循環認定してしまう
  バグ、(3) 複数パッケージ(または同一パッケージの複数ファイル)で同名のトップレベル
  constを宣言すると生成JSがパースできない構文エラーになるバグ(新規`declared_consts`
  で重複検出し明確なErrに変更)、(4)「パッケージ間でのstruct循環は構造的に起こり
  得ない」という設計上の前提が、型注釈/struct literalの解決で`is_package_alias`
  (実際にimportされているか)を確認していなかったため成り立っていなかったバグ
  (import文を経由しないパッケージ間の型参照を許すと依存グラフの循環検出をすり抜け、
  処理順に依存して非決定的に振る舞っていた)。回帰テスト6件追加、220→226件、
  `cargo clippy --all-targets -- -D warnings`クリーン。詳細はtodo.mdの当該項目が
  一次情報源
- **checker+codegen milestone 7(match/is式・判別可能union)完了(2026-07-23)**。
  パーサー・型システムは既に完全実装済み。**最重要の発見**(TS版codegen.tsを深掘りして
  確認): narrowing(絞り込み)はcheckerのスコープだけの概念で、生成JSには一切
  影響しない——`match`のアーム本体は`__m`という合成パラメータを一切参照せず、元の
  Mesh変数名をそのまま参照する生JSになる(JSは動的型付けのため)。つまりnarrowingは
  codegen側の型依存判断(`__iarith`等)を正しくするためだけに必要で、生成JSの「形」
  自体は変えない(milestone 5のselect/orElseの束縛パターンの再利用)。`checker.rs`は
  `resolve_struct_decls`を`resolve_type_decls`へ汎化(struct/union型aliasを同じ依存
  グラフで扱いサイクル検出も拡張)、`pattern_matches_member`/`narrow_for_match_patterns`/
  `narrow_for_is`/`match_is_exhaustive`を新設。`codegen.rs`は新設`gen_type_test`
  (TS版genTypeTestの移植、discriminant_tagは一切参照せずASTから直接構造テストを
  組み立てる)、`match`はexhaustiveならTS版と同じ形でbyte-for-byte一致、**exhaustive
  でない場合だけ**(Rust版だけの安全ガード)明確なpanicを追加。`if x is T {...}`は
  then/else/フォールスルー(then節が必ず終端する場合、絞り込んだ「残り」の型を後続の
  同ブロックへ引き継ぐ)いずれでも正しくnarrowingする——`examples/channel_spec.mesh`の
  `if v is closed { break } total = total + v`で実際に検証済み。自己参照する判別可能
  union(`examples/tree.mesh`)はmilestone 2の自己参照structと同じ理由で対象外、
  明確なErrになることを確認。新規テスト19件(checker9+codegen10)、226→244件。
  **想定外の副産物**: `examples/maps.mesh`も今回`is`実装により初めてフルに動くことを
  確認(milestone 4時点では`is none`未対応で止まっていた)。詳細はtodo.mdの当該項目が
  一次情報源
- **PR #22の5エージェントコードレビューで発見し即修正した4件のバグ**(いずれも
  実行して再現確認済み、milestone 4/5/6と同じ「再現確認済みなら即修正」の前例に
  従った): (1) `match_is_exhaustive`が0アーム・非union subjectを常に「網羅的」
  扱いし安全ガードが完全に無効化(空の`match x {}`は構文的に壊れたJSにすらなって
  いた)、(2) `pattern_matches_member`の非リテラルフィールドが型を見ない緩い判定
  だったため同名・型違いフィールドで判別するunionのexhaustivenessが過大評価され
  値が誤ったアームへ静かに振り分けられる(3エージェント中3件が独立指摘)、
  (3) 裸型名`"error"`パターンがchecker側ではnamed error structも拾うのに
  codegen側の`instanceof Error`テストは拾わないという認識の食い違い(TS版はこの
  組み合わせを`impossible-pattern`診断で弾く、このリゾルバはプリミティブERROR型
  のみに一致させ食い違いを解消)、(4) `gen_if`のnarrowing伝播がelse節ありthen節
  必ず終端のケースを見落としていた(if/elseの後で絞り込み前の型のまま扱われ
  `__idiv`ではなく浮動小数点除算になる)。回帰テスト2件追加、244→246件、
  `cargo clippy`クリーン、既存の全exampleがbyte-for-byte一致のまま回帰なしを
  再確認。詳細と、修正せず記録に留めた3件(struct literalのfield未検証・
  union型struct literalの算術ギャップ・裸struct名パターンの判別不能、いずれも
  既存の別スコープ決定の帰結)はtodo.mdの当該項目が一次情報源
- **checker+codegen milestone 8(error type・union形式の名前付きエラー型)完了
  (2026-07-23)**。milestone 7完了後、kanayamaから「error typeとjson structは
  一緒にできるか」と聞かれ、TS版`tagErrorMembers`(約20行)と`json-decode.ts`
  (313行、AST合成による`decode<X>`自動生成+`mesh/json`スタブが未実装)を直接
  読んで調査した結果、分量・複雑さが1桁近く違い技術的にも無関係と判明したため
  分けて進めることに決定。`checker.rs`の`resolve_type_decls`に新設
  `tag_error_union`を追加——union宣言のソースmembersがすべて無名`{...}`由来
  (今まさに作られたfresh struct)であることを検証し(既存の名前付き型への参照や
  非struct形はTS版の2診断をまとめた明確なErrに)、通れば全メンバーに
  `is_error_type: true`を立てる(単体の`error struct`と違い、union形は全メンバーが
  等しく失敗を表す設計)。既存の`is_failure_type`/`has_structured_failure`/
  `or_binding_type`(milestone 3)・`pattern_matches_member`/`match_is_exhaustive`
  (milestone 7)は無変更でそのまま効く。`codegen.rs`は`generate_package`の
  「union形error typeは未対応」ゲートを撤去し、`Expr::StructLit`の`__errTag`
  ラップ判定を`lookup_union`にも対応させた。新規`examples/db_error.mesh`
  (`error type DbError = {...}|{...}`+`?`伝播+`or e => match e {...}`)で
  TS版とbyte-for-byte一致を確認、既存の全exampleも回帰無し。
  **PR #23コードレビューで発見・即修正した2件**: (1) `__errTag`ラップ判定が
  "any"判定だったため、通常structとerror type unionを混ぜたさらに外側の
  unionで成功値まで誤ってerrTagラップされる回帰(このPR自身の新規コードの
  バグ)、(2) パッケージのexportedシンボル登録がunion型alias宣言
  (milestone 7)を一切見ておらず、milestone 8がこのmilestone 6/7由来の
  既存ギャップを実害のある形で顕在化させていた——exportされた`error type`が
  パッケージ越しに構築できない、かつpkg修飾された戻り値型注釈がis_error_type
  無しの殻structへ静かにフォールバックしmilestone 3の安全ガードを素通りして
  文脈付き`?`が構造化errorを「成功扱い」してしまう(2エージェント独立指摘・
  実行確認済み)。いずれも修正しexport登録をunion型aliasにも対応、
  新設`type_is_error_instance`ヘルパへ集約。テスト246→253件(+7)。
  詳細はtodo.mdの当該項目が一次情報源
- **checker+codegen milestone 9(json struct)完了(2026-07-23)**。これまでの
  8マイルストーンと質的に違う点: checker/codegenの「解析」だけでなく、
  `json struct X {...}`宣言から`decode<X>(v: json.Value) X | error`という新しい
  Mesh関数を**構文レベルのAST(Stmt/Expr)として合成しprogram.fnsへ追加する新しい
  パイプライン段階**が必要(TS版`compiler.ts`が`parse`直後・`check`前に挟むのと
  同じ)。新設`rust/src/json_decode.rs`(TS版`json-decode.ts`313行の忠実な移植)+
  新設`json_stdlib_symbols()`(`mesh/json`という`.mesh`ソースを持たない組み込み
  パッケージ、TS版`stdlib.ts`相当)。**ランタイムJS側は既に完全に揃っていた**
  (H-2実装時にruntime.ts全体が移植済みで`json$parse`等が既に実装済み)ため、
  codegen自体への変更は「registryへの1回の登録」+`modules.rs`への`mesh/json`
  早期continueだけで済んだ。`json.Value`(TS版では真に自己参照する判別可能union)は
  milestone 2以来の自己参照型の壁(`tree.mesh`と同じ)にぶつかる——真に自己参照する
  再帰位置(`arr.items`/`obj.entries`の要素/値型)だけを名前だけの不透明な殻に留め、
  それ以外(kind判別フィールド+実フィールド)は本物のunionにする設計を選んだ
  (最初は完全に不透明な殻structにしていたが、PR #24のcode reviewで
  `tests/e2e.test.ts:1146-1160`という既存のmesh/json手書きdestructureが壊れる
  ことが発覚し修正——下記参照)。**副産物として既存バグを発見・修正**: `is_json`
  宣言がstruct自体の型解決から丸ごと除外されていた(json struct未実装時の
  プレースホルダ)ため、手書きの`X{...}`構築でフィールドが空の殻へ静かに
  フォールバックする潜在バグがあった——TS版がisJsonをstruct型解決に一切使わない
  ことを確認し、除外を撤去。新規`examples/json_decode.mesh`
  (`tests/e2e.test.ts:2738-2859`のシナリオ一式)+`examples/json_models_demo.mesh`
  (cross-package export)でTS版とbyte-for-byte一致を確認、既存の全example・
  `tree.mesh`の明確なErrも回帰無し。**PR #24コードレビューで発見・即修正した2件**:
  (1) `json.Value`を完全に不透明な殻にする当初設計は、json struct機能より前からある
  既存のmesh/json手書きdestructure(`if v is {kind:"obj"} { len(v.entries) }`)を
  壊す見積もり漏れだった——`mesh/json`のimport自体がこのPRで初めて可能になるため
  顕在化。再帰位置だけ殻に留める設計に修正し、1階層の絞り込み+フィールドアクセスが
  正しい型(`len`が`.size`を選ぶ等)で動くようにした。(2) 合成する`decode<Name>`が
  手書きの同名関数と衝突しても検出されず、二重宣言の無効なJS(SyntaxError)を
  静かに出力していた——このシンセシス自体が初めて「隠れた予約名」を生む処理だった
  ため踏みやすい間違いになっていた。合成前に衝突を確認し明確なErrにする修正を追加。
  副産物としてTS版自体のフォーマッタのバグ(`json struct`の`json`キーワードが
  再整形で消える)も発見・修正。テスト253→266件(+13)、TS版テストスイート484→485件。
  詳細はtodo.mdの当該項目が一次情報源
- **checker+codegen milestone 10(filter/map/reduce)完了(2026-07-23)**。
  `filter`/`map`/`reduce`自体のcodegen(`(await __filter(...))`等、ランタイム
  ヘルパーはH-2実装時に移植済みで既に揃っていた)は3行で済んだが、その引数となる
  **無名関数式(`Expr::FnExpr`)のcodegenがmilestone 4以来ずっと明確なErrスタブの
  ままだった**ため、これを実装するのが今回の本題だった。無名関数は他の関数の
  中にネストしうる(`g := fn() int { return f()? }`)ため、`prop_used`/
  `spawn_used`を単一フラグからスタック(`Vec<bool>`)へ変更(TS版の
  `propStack`/`spawnStack`と同じ設計)、FnDecl/Expr::FnExpr共通の「本体を
  いったん別バッファに生成し`?`/`spawn`の使用有無で事後にtry/catch/finally包みを
  決める」ロジックを`gen_fn_body`という共有ヘルパーへ切り出した。checker.rs側は
  `Expr::FnExpr`の型推論(`Type::Fn{params, ret}`、本体は検査しない)を追加した
  ことで`infer_expr`の`match`が全`Expr`variantを尽くす形になり、既存の
  `_ => ANY`最終フォールバックが到達不能になったため削除(意図せぬ副産物——
  これでinfer_exprは全構文を明示的に扱うようになった)。新規
  `examples/filter_map_reduce.mesh`(名前付き関数を値として渡す・インライン
  クロージャで外側のmut変数を捕捉・mapで要素の型を変える・reduceの2用途・
  filter→map→reduceのパイプライン合成)でTS版とbyte-for-byte一致を確認、
  既存の全exampleも回帰無し。**PR #25コードレビューで発見・即修正した4件**
  (うち3件は複数エージェント独立指摘): (1)(2) 裸の識別子がトップレベル関数名を
  値として参照する場合・ローカル変数に代入した無名関数を直接呼び出す場合の
  いずれも、`CheckerCtx`のローカル変数用`scopes`とトップレベル関数用`fn_decls`
  という別テーブル構成のせいで型がANYへ落ち、`__iarith`等の整数オーバーフロー
  安全ガードが選ばれなくなっていた(milestone 10で初めて到達可能になった経路、
  実際にオーバーフローする入力で実害を再現確認済み)——両方とも「ローカル→
  fn_decls→組み込み」の優先順位でフォールバックするよう修正。(3) 入れ子になった
  `Expr::FnExpr`の再インデントが崩れる(実行結果は正しいがbyte-for-byte一致の
  検証基準から外れる)——TS版と同じく全体を結合してから改行分割し直す形に修正。
  (4) **milestone 10自体とは無関係な既存バグ`toInt`が常に失敗する問題も発見**——
  `prelude()`が`runtime.ts`のテンプレートリテラルを単純な部分文字列抽出のみで
  取り出し、JS自身のエスケープ解決(`\\`→`\`)を評価していなかったため、正規表現が
  `\\d`(実質何にもマッチしない)になっていた(`toInt`を使うexampleが今まで無く
  発覚しなかった)。`prelude()`の戻り値を所有型にしエスケープを評価する形に修正。
  テスト266→276件(+10)。`defer`は独立した別機能なので今回のスコープ外
  (次のmilestone候補)。詳細はtodo.mdの当該項目が一次情報源
- **checker+codegen milestone 11(defer)完了(2026-07-23)**。todo.mdに残っていた
  既知の未対応機能が`defer`のみになり実装。TS版`genDeferStmt`の「影武者call式」
  トリック(引数・レシーバをdefer時点の値で一時変数〈`__d0`,`__d1`,...、コンパイル
  全体で1つのカウンタ〉へ捕捉し、一時変数への参照に差し替えた影武者のcall式を
  既存の`gen_call`にそのまま渡すことで呼び出し形の判定を重複させない)をそのまま
  踏襲。**TS版との違い**: checker/codegenが融合しているため、影武者の一時変数の
  型を`self.ctx.declare`でも宣言しないと`gen_call`自身のメソッド判定がANY扱いに
  なってしまう(実装中に発見・対応)。milestone 10で切り出し済みの`gen_fn_body`
  (FnDecl/Expr::FnExpr共通)へ`defer_used`スタックを追加、`finally`節を
  「spawnした子タスクを待ってからdeferを実行」の順序に拡張。無名関数式の中の
  `defer`もmilestone 10のprop/spawnスタック分離のおかげで独立して働く。**副産物
  としてTS版自体のフォーマッタのバグを発見・修正**(milestone 9の`json struct`
  キーワード欠落と同じ構図)——`printStmt`のswitchに`deferStmt`のcase自体が無く、
  `defer`文を再整形すると丸ごと消えてしまっていた。新規`examples/defer.mesh`
  (複数defer LIFO・引数固定・メソッドdefer・組み込み/パッケージ修飾defer・早期
  return・ループ内累積・spawn併用・無名関数内defer)+`examples/defer_panic.mesh`
  (panicでの巻き戻り、終了コード/stdout/stderrの3点確認)+
  `examples/defer_pkg_demo.mesh`(cross-package)でTS版とbyte-for-byte一致を確認、
  既存の全exampleも回帰無し。**PR #26コードレビューで発見・即修正した1件**:
  影武者call式にdefer文自体の`pos`を使ってしまっており(TS版は元のcall式自身の
  `pos`をそのまま引き継ぐ)、deferした組み込み呼び出しの型エラー・パニック位置
  情報がdefer文の位置を指してしまっていた(値・フロー自体は正しかった)。元のcall式
  の`pos`を捕捉し引き継ぐよう修正。あわせて`ast.rs`/`parser.rs`の古いコメント
  (「checkerが検証する」→実際は今回実装した`codegen.rs`が検証)も修正。1件、
  TS版自体にも同じ理由で存在する既知の限界(構造体フィールドが保持する関数値
  経由の呼び出しではレシーバが固定されない)を確認したが、Rust版だけの新しい
  退行ではないため記録に留めた。テスト276→284件(+8)、TS版テストスイート
  486→490件。詳細はtodo.mdの当該項目が一次情報源
- **checker+codegen milestone 12(struct literalのフィールド検証)完了(2026-07-24)**。
  11マイルストーン完了後、kanayamaと既知の限界を整理し、最も古く(PR #17以来)・
  影響範囲が広い「struct literalのフィールドが宣言済みの形と一切照合されない」穴を
  選んで着手(`is_numeric`のUnion/ANY対応と2択で提示し、こちらを先に選択)。TS版
  `structLit`ケースを読むと、単純なフィールド名照合だけでなく**判別可能unionの
  構築時disambiguation**(タグ値でメンバーを特定)も含む、想定より大きい機能だった。
  F-7判別可能unionのタグ計算(`find_discriminant_tag`、milestone 7では「codegenが
  参照しないため計算しない」という意図的な先送りだった)を今回初めて実装し
  `resolve_type_decls`から呼ぶ形にした——struct literalの正しいdisambiguationには
  タグ名そのものが要るため。新設`resolve_struct_lit_member`(タグdisambiguation/
  単一候補/名前付きstruct同士のフィールド集合解決の3分岐)・
  `validate_struct_lit_fields`(重複/未知/型不一致/欠落、型互換性は既存の
  `types::assignable`を再利用)を`codegen.rs`の`Expr::StructLit`から呼ぶ。
  **検証で発覚**: 既存のmilestone 8回帰テストが`Result{value: 42}`(union自身の
  名前で名前付きメンバーを構築)という、実際にTS版でも`discriminated-union-tag-missing`
  で拒否されるコードを使っていた(無名メンバー2個がunion自身にタグを要求するため、
  名前付きメンバーはタグ経由のdisambiguationの対象外になる)——具体的な struct 名
  (`Success{value: 42}`)を使う形にテストを修正(退行ではなく、milestone 12の検証が
  TS非互換な既存テストを正しく検出した形)。あわせて過去に「Rust側だけの穴」と
  記録していた「union経由で構築した直後のフィールドアクセスがANYになる」という
  項目も、実際はTS版自身の意図的な設計(式全体の型は絞り込んだメンバーではなく
  常にunion自身)だったと判明し、todo.mdの記載を訂正した。テスト284→304件(+20)。
  既存の全example(21本、自己参照型で対象外の`tree.mesh`を除く)がbyte-for-byte
  一致のまま回帰無し。詳細はtodo.mdの当該項目が一次情報源
- **checker+codegen milestone 13(算術演算子の妥当性検査・is_numericのUnion/ANY
  問題)完了(2026-07-24)**。12マイルストーン完了後、残る既知の限界のうち
  「`is_numeric`のUnion/ANY対応」に着手。当初は「union型への算術がおかしくなる」
  という狭い問題だと見積もっていたが、TS版`checkArithOp`を読み実際にTS版へ複数
  パターンを通して検証した結果、**Rust版には算術演算子(`+ - * / %`)の妥当性検査が
  一切無い**という、より根本的な穴だと判明した——両辺が「両方int/float」でも
  「両方stringで`+`」でもない組み合わせは、TS版なら`invalid-operation`で拒否する
  ところを、Rust版は無条件にANYへフォールバックし生のJS演算子を出力していた
  (`x := <-ch; y := x / 2`が浮動小数点`/`になる、`true - false`が意味不明な
  JSになる、等)。「union型への算術」(PR #19以来の限界)はこの一般的な穴の一種類に
  すぎなかった。milestone 2以来一貫している「TS本体は診断、Rust版は明確なErr」
  パターンをそのまま適用し、`check_arith_op`/`infer_binary`の戻り値を
  `Result<BinaryInfo, String>`へ変更。TS版と同じ「ANY安全弁がis_numeric分岐の
  中と外の2箇所にある」構造まで含めて移植(`infer_expr`自体はErrを飲み込み
  ANYへフォールバックし、診断を出さない設計を維持——codegen側だけがErrを外へ
  伝える)。スコープは算術演算子のみ(比較演算子`< <= > >=`の`incomparable-types`・
  `&&`/`||`の`not-bool`・`==`/`!=`絡みの検査は別カテゴリの診断として対象外)。
  **code reviewで見つかった最重要指摘**: unary`-`と`++`/`--`がTS版で算術演算子と
  全く同じ`invalid-operation`診断を共有しているのに一切検査されておらず
  (`x := <-ch; x++`・`bools[0]++`〈bool配列〉等が無診断で素通りしJSの暗黙型変換で
  壊れた値になっていた)、「is_numericのUnion/ANY問題を閉じる」という今回の目的
  そのものに直結する漏れだったため、`check_unary_minus`/`check_inc_dec`を追加し
  スコープに含めた。ほかTS版の唯一の"hint"メッセージ(`+`かつ片側がstringのとき
  「str()で変換を」)の移植漏れも修正。テスト304→322件(+18)、既存の全example
  (22本)がbyte-for-byte一致のまま回帰無し。`<-ch`/map読み取り/`true - false`/
  `bools[0]++`の各パターンをRust版・TS版両方でコンパイルし、同じ理由・同じ
  位置情報で拒否されることまで確認済み。詳細はtodo.mdの当該項目が一次情報源
- **次にやるなら**: 確認済みの13マイルストーン(struct/メソッド → error/json →
  配列/map → 並行処理 → モジュール → match/is式・判別可能union → error type
  〈union形式〉→ json struct → filter/map/reduce → defer → struct literalの
  フィールド検証 → 算術演算子の妥当性検査)が全て完了——TS版リファレンス実装の
  主要機能をRust版がひととおり移植し終えた。細かな既知の限界・意図的なスコープ
  縮小(自己参照型・`json.Value`の2階層以上のdestructure・ジェネリック関数・
  `mesh/io`/`mesh/http`・cross-file/cross-packageのjson struct参照・
  `gen_lvalue`の代入先フィールド名検証・struct宣言時点の`__proto__`ガード・
  比較演算子/`&&`/`||`/`==`/`!=`の妥当性検査 等)は引き続きtodo.mdに記録済みの
  通り残る。次の対象はkanayamaと相談して決める
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
  `cargo test`(特に`文字列補間_上限を超えるネストは...`)を必ず確認すること。
  **milestone 11で実践**: 関数型注釈+無名関数式の追加時にこの教訓通り実装直後に同テストを
  5回連続実行し、クラッシュしないことを確認してから次に進んだ(今回は既存ヘルパーへの
  委譲のみで局所変数が少なく、別関数への切り出しは不要と判断)
- **教訓(checker/codegen milestone 1で発覚)**: 他言語で書かれたファイルを`include_str!`で
  「そのまま埋め込めば済む」と考えるのは危険——`src/runtime.ts`はTSファイルであり、
  中身のランタイムJSはテンプレートリテラル文字列として包まれている。ファイル全体を
  素朴に埋め込むとTSの宣言構文(`export const PRELUDE = \`...\`;`)まで生成JSに混ざり、
  実行時に構文エラーになる。実装直後に実際に`bun`で生成JSを走らせて確認したことで
  この場で発覚した——「コンパイルが通った」だけでは検知できないクラスの不具合なので、
  今後もcodegen関連の変更は必ず生成JSを実行して確認すること(このmilestoneから
  「本当に動く」ことの確認が可能になったので、以後のmilestoneでも同様に徹底する)
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
(cd rust && cargo run -- ../examples/hello.mesh)              # AST疎通確認CLI
(cd rust && cargo run -- ../examples/hello.mesh --emit-js)    # 生成JSを標準出力へ(milestone 1の
                                                               # スカラーサブセットのみ。struct/map/
                                                               # channel等は「未対応」エラーになる)
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
