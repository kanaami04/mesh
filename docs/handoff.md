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
- **次にやるなら**: milestone 6(モジュール。`examples/*.mesh`を1本ずつ動かす計画。
  todo.md参照)
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
