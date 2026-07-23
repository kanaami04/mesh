# Mesh TODO

> プロジェクトの「正」は [docs/requirements.md](docs/requirements.md)(要件定義書)。
> このTODOの優先順位は要件定義の討議結果で見直す。

## 次の一手(おすすめ順)

- [x] **批評ターン(2026-07-19)起点の再討議・実装** — 全記録は [docs/critique-2026-07.md](docs/critique-2026-07.md)、
      討議項目は [docs/design-agenda.md](docs/design-agenda.md) の **F節**(F-1〜F-15)。**F節は全項目決着**
      (2026-07-20)。宿題だったC-6の続き(環境別モジュール・`mesh/http`)も2026-07-21実装済み(下記)

- [x] **TS不満調査からの追加討議項目(2026-07-21)** ✅ **H-1・H-2とも実装済み(2026-07-21)**。詳細は [docs/design-agenda.md](docs/design-agenda.md) H節
  - [x] **H-1: `any`型の扱い** ✅ **2026-07-21実装**(kanayamaと討議のうえ選択肢(a)完全撤去を採用)。
        `x: any = 5`からの型不整合な演算・`mut arr := []`への型混在pushを実測で確認し、
        どちらもコンパイルエラー無しで通っていた実際の穴だったことを検証してから着手。
        `any`はユーザーが書ける型として撤去(書くと`any-type-removed`)、内部のエラー回復用
        センチネルは残すが到達不能。副作用として、文脈の無い空配列/mapリテラル
        (`x := []`)も`cannot-infer-type`エラーになった(`xs: Todo[] = []`のように文脈が
        あれば今まで通り動く)。詳細は design-agenda.md H-1参照
  - [x] **H-2: API境界の検証つきデコード(P6の実質化)** ✅ **2026-07-21実装**(kanayamaと討議のうえ、
        構造体単位の自動生成を採用)。`mesh/json`にヘルパー(`json.field`/`json.optField`/
        `json.asString`/`asInt`/`asFloat`/`asBool`/`asArray`)を追加。さらに`json struct X {...}`
        (`error struct`と同じマーカー構文)と書くと、`decode<X>(v: json.Value) X | error`を
        コンパイラが自動生成する(Meshの構文レベルAST=FnDeclを合成し、以降のcheck/codegenは
        無改造で流用)。対応範囲: int/float/string/bool・同一ファイル内の他のjson struct
        (ネスト)・それらの配列・`T | none`。それ以外は合成時にフィールドを指してエラーにし、
        ヘルパー関数での手書きデコーダへ誘導する。詳細は design-agenda.md H-2参照

- [x] **C-6の続き: 環境別モジュール・`mesh/http`** ✅ **2026-07-21実装**(kanayamaと討議のうえ、
      Go `net/http`直訳〈生ハンドラ1本+`http.listen`、ルーター無し〉を採用)。
      `http.Request{method, path, query, headers, body}` / `http.Response{status, body, headers}` /
      `fn listen(addr: string, handler: fn(Request) Response) none | error`。サーバー専用
      (クライアント機能=fetch相当は無い)。障害分離を自動適用(1リクエストのハンドラが
      panicしてもそのリクエストだけ500になり、サーバーは他のリクエストへ応答し続ける —
      F-14実装メモの初適用)。メソッド別登録(`http.get`/`http.post`等、Echoスタイル)はv2として
      `mesh/http`自身に将来追加する構想を明記(フレームワークへは切り出さない — Q2未決着で
      サードパーティパッケージのエコシステムが無いため)。DOM側(未決Q3)は引き続きスコープ外。
      詳細は design-agenda.md I節参照
  - 推奨順: ~~F-1(前半・後半)~~・~~F-2(前半・後半)~~・~~F-3~~・~~F-4~~・~~F-5~~・~~F-6~~(すべて✅ 2026-07-19)・
    ~~F-12第1・第2ラウンド~~(✅ 2026-07-20。結果は [docs/benchmark-2026-07.md](docs/benchmark-2026-07.md)。
    第2R: Haiku被験体で一発成功率 Mesh 2/4・TS 3/4・Go 4/4、全セル最大1往復で成功。
    回帰確認でSonnet×Task04が往復2→0 — **発見→修正→改善のループを1周実証**。
    matchの「何もしない」アーム非対応をカード明記(同日)。第3R候補: 大型タスク・複数試行のdiff一貫性)
  - **ベンチ第1ラウンドで出た改善項目**:
    - [x] 複数行unionの行頭`|`継続をパーサ対応 ✅ 2026-07-20(行末`|`と両対応。カードの
          判別可能union例も複数行形に変更して実例化)
    - [x] 予約語一覧(`eval`/`arguments`等のRESERVED全34語)をカードに明記 ✅ 2026-07-20
  - [x] ~~F-13(前半・後半とも)~~ ✅ 2026-07-20実装。前半: 全169箇所の診断に約87種のコードを付与、
        機械適用可能なものに単一range置換の`fix`、`mesh explain <code>`で説明を引ける。
        後半: `mesh card --for <file>...`(使っている機能のセクションだけの縮小版カード)+
        カード完全性の逆方向検査(`BUILTINS`/`RESERVED`とカード記載の機械照合CI。
        `s[0]`のような未知の見落としまでは拾えない退行防止止まり、と設計時点で合意済み)
  - [x] ~~F-7〜F-11(討議項目の残り)~~ ✅ 2026-07-20実装(kanayama承認: 各項目の推奨案どおり)。
        F-7判別可能unionのタグ必須化(宣言時に検証、構築はタグ値のみで解決。名前付きstruct同士の
        unionは対象外で従来どおり)/ F-8 transform→map改名(文脈依存キーワード)/
        F-9小さな一貫性の穴4件(空配列記法統一・複合代入`+=`等・トップレベル定数・`get(arr,i)`)/
        F-10 int safe-integer検査(panic層)/ F-11 chan capacity常時明示必須(`chan<T>(none)`で
        無制限は引き続き選べる)。討議項目(F節)は全て決着 — 詳細は design-agenda.md 参照
  - [x] ~~F-14(mesh/io + mesh/json)~~ ✅ 2026-07-20実装(kanayama承認: v1は
        `io.args()`/`io.readFile(path)` + `json.parse`/`json.stringify`+`json.Value`のみ)。
        `.mesh`ソースを持たない「組み込みパッケージ」という新しい種別を追加(`src/stdlib.ts`で
        型シグネチャを直接構築しcheckerのregistryへ事前登録)。json.Valueは自己参照判別可能union
        のショーケースとして無事構築できた。副産物: `typeToString`の自己参照型無限再帰バグを発見・修正
  - [x] ~~F-15(mesh test --json)~~ ✅ 2026-07-20実装(kanayama承認: 各設計判断は推奨案どおり)。
        テストは`_test.mesh`ファイル限定・`fn test...() none | error`(none=合格・error=失敗、
        新しい合否概念を作らず既存union語彙を流用)。`mesh test file.mesh`(mainパッケージ)/
        `mesh test <dir>`(そのパッケージ自身)のどちらもmain()不要でTDD向け。依存先パッケージの
        テストは実行しない。**テスト実行中のpanicは1件の失敗として隔離**し他は続行 —
        F-14実装メモの障害分離方針を初めて実地適用した。討議項目(F節)はこれで全て決着

- [ ] **言語カード実証実験の継続** — 全記録は [docs/card-experiments.md](docs/card-experiments.md)
  - [x] 第1〜3回実施(TODO×2・単語頻度)。6→1→1往復。手法はメモリに記録
  - [x] 空の型付き配列 `Todo[]{}` 実装、カード記述漏れ多数を追記(参照値/前置!/print/map更新等)
  - [x] **`mut best := none` 問題を解決** — 型注釈つき宣言 `mut best: string | none = none`
        を実装(2026-07-18)。空配列 `xs: Todo[] = []` も同時に解決(any[]の互換を緩和)
  - [x] 第4回実験(2026-07-18): 型注釈導入後、第3回と同一題材で再測定。往復1のまま、
        不在アキュムレータをフラグ回避→本物のabsence handlingで記述。map反復順(挿入順)をカード明記
  - [x] 第5〜10回実施(2026-07-19): 電卓・再帰下降パーサ・並行処理・stdlib高階関数・
        判別可能union・sort/contains系と各層を一巡。実装バグ6件発見・全て解消
        (chan配列 / 複数行配列リテラル / eval・arguments予約語漏れ 等)
  - [x] 第11回実施(2026-07-20): モジュール(import/export)+mesh/json+mesh/ioを組み合わせた
        在庫レポートツールで再測定。完全一発成功・エラー0件。カード記述漏れ1件のみ(解消済み)。
        単体機能の検証は出尽くしたため、次は中期課題の実装 or より大規模な複合タスクで再測定
- [ ] **union路線への移行** — ほぼ完了(2026-07-17)
  - [x] union型(`T | none` / `T | error`)・narrowing・`!`/`or`・nil/多値戻りの撤去
  - [x] match式(網羅性検査・アーム内narrowing・`_`・複数パターン・リテラルパターン)
  - [x] 文字列リテラル型(widening込み)と `type Status = ...` 宣言(循環検出込み)
  - [x] `is` の対象拡大(2026-07-18: `closed`を追加。none/error/closedの3種)
  - [x] `is` のさらなる対象拡大 — 2026-07-19実装。matchと完全に同じパターン
        (型名・文字列リテラル・部分構造 `{ kind: "ok" }`)を受け付け、ガード節スタイル
        (`if res is { kind: "notFound" } { return "404" }`)が書けるようになった
- [x] **struct と型定義** — コア実装済み(2026-07-17)、判別可能unionまで完了(2026-07-19)
  - [x] struct 宣言・リテラル生成・フィールドアクセスの型検査・再帰struct・`type =` の `{...}` ガード
  - [x] インライン `{...}` 型式(union内)と判別可能union(`{ kind: "ok", ... } | ...`)— 2026-07-19実装(C-1)。
        構築は union自身の名前をstructリテラル名として流用、matchは部分構造パターンで絞り込み。
        自己参照(木構造・AST等)も同日中に追加実装 — structフィールド越しの参照はknot-tyingで解決。
        structを挟まない裸のunion同士の相互再帰だけは引き続き`type alias cycle`(意図的な制限)
  - [x] 同一性判定にインライン`{...}`型式(判別可能unionメンバー)を対応 — 2026-07-19実装。
        **訂正(2026-07-21、調査で発覚)**: この行は当日いったん「名前付きstruct同士も含め
        全面的に構造比較に変更」した直後の記述だったが、同日中にF-3([design-agenda.md](docs/design-agenda.md)参照)
        で名前的型付けへ巻き戻された。**現状(確定)**: 名前付きstruct同士は名前で判定
        (`Meters`と`Dollars`は形が同じでも別型・コンパイルエラー)。無名`{...}`型式が絡む
        比較だけ構造的(`src/types.ts`の`typeEquals`参照)。本行は巻き戻し前の記述のまま
        取り残されていたため訂正した
- [x] **モジュールシステム(import / export)** — 2026-07-19実装。パッケージ=ディレクトリ
      (Go風・package宣言なし)、export可視性、`pkg.symbol`修飾アクセス、環境判別は
      「importしたモジュールから自動推定」方式を採用(環境別stdlibの実装時にセットで実装)。
      v1制限: エントリは1ファイル・パスは単一セグメントのみ。詳細は features.md
- [x] **その他の採用決定済み構文の実装** — 全て完了
  - ~~文字列補間 `"${式}"`~~ ✅ 実装済み(2026-07-17)
  - ~~デフォルト不変 + `mut`~~ ✅ 実装済み(2026-07-17)
  - ~~E1 `!` / E2 `or`~~ ✅ 実装済み(2026-07-17、union移行とセットで)
  - ~~`spawn` 式 / `wait` ブロック / `select`~~ ✅ 実装済み(2026-07-18)
  - ~~map型(`m[k]` は `V | none`)/ for range(完全形のみ)~~ ✅ 実装済み(2026-07-18)
  - 決定記録は [docs/syntax-proposals.md](docs/syntax-proposals.md) と [docs/design-agenda.md](docs/design-agenda.md)
- [ ] **Rust移植の開始** — 2026-07-21着手(kanayamaと討議のうえ。進め方は今まで通り
      Claudeが実装+日本語で解説するスタイルを継続)
  - 現行テストスイート(477件、2026-07-21時点)を「合格基準」にする
  - lexer → parser → checker → codegen の順に移植
  - [x] **lexer移植(第一弾)** ✅ 2026-07-21実装。`rust/`にCargoプロジェクト新設
        (lib+binのハイブリッド構成)。`src/token.ts`+`src/lexer.ts`(計393行)を
        `rust/src/token.rs`+`rust/src/lexer.rs`に1:1移植、`tests/lexer.test.ts`の
        15件のテストも`#[cfg(test)]`インラインテストとして移植し全件パス。
        `cargo clippy`警告ゼロも確認。`mise.toml`に`rust = "1.97.1"`を追加、
        `rust-test`/`rust-check`タスクを新設。
        **移植で出てきたTS→Rustの主な設計判断**: (1) TSの文字列リテラルunion
        (`TokenType`)はRustのenumに1:1対応するが、フィールド名`type`は予約語なので
        `kind`に改名(Meshの判別可能unionのタグ名と同じ呼び方になり、むしろ整合)。
        (2) TSの`throw`はRustに無いので`Result<T, E>`+`?`に変換
        (Mesh自身の`T | error` + `?`が着想を得た元ネタと同じ形なので自然に対応した)。
        (3) TSのクロージャ(advance/pos/last)が外側の変数を直接書き換える形は、
        Rustの借用チェッカと相性が悪いため、状態をまとめた`Lexer`構造体+
        `&mut self`メソッドに置き換えた。(4) 文字列は`Vec<char>`でUnicodeスカラ値
        単位に扱う(JSのUTF-16コード単位ベースの`source[i]`とは厳密には異なるが、
        BMP内の文字〈日本語含む〉であれば実質差が出ない、という意図的な簡略化)
  - [x] **parser移植(第一弾・実用サブセット)** ✅ 2026-07-21実装。`src/parser.ts`
        (1217行)+`src/ast.ts`(403行)全体は一度に移さず、意味のある実用サブセットに
        絞った——`fn`宣言(ジェネリクス・レシーバは次回)・トップレベル定数・
        if/else-ifチェーン・for(3形態)・break/continue・変数宣言/代入/複合代入/
        インクリメント・二項演算子(優先順位込み)・関数呼び出し。対象外(struct/type宣言・
        ジェネリクス・match/is/or・spawn/wait/chan/select・文字列補間・配列/mapリテラル・
        import/export等)は次回以降のPRで追加していく。構文エラー復帰(パニックモード)の
        枠組みは今後ずっと使う土台なのでフルで移植した。`tests/parser.test.ts`
        (35件)のうちスコープ内の16件を移植し全件パス(lexerと合わせて計31件)。
        `examples/*.mesh`全13本のうち`hello.mesh`/`fizzbuzz.mesh`(スコープ内)は
        正しくパース、残りはスコープ外の構文(chan型・struct宣言・is式・mapリテラル等)で
        クラッシュせず明確な構文エラーになることを確認(誠実な「未対応」の失敗の仕方)。
        **移植で出てきた主な設計判断**: (1) `CompileError`にlexer/parser共通のエラー型として
        `Fix`(自動修正情報)を追加したところ136バイトまで育ち、`Result`の`Err`に素で
        置くと成功時の戻り値まで重くなる(clippy::result_large_err)ため`Box<CompileError>`
        に統一(公開APIの`parse()`/`parse_ignoring_errors()`だけは呼び出し側の使い勝手を
        優先してBoxを漏らさない)。(2) TS版の「1件ならCompileError、2件以上なら
        MultiCompileError」という互換維持の型分けは、合わせるべき既存Rust呼び出し側が
        無いので`Vec<CompileError>`に統一する簡略化をした。(3) 二項演算子は専用enumを
        作らずlexerの`TokenType`をそのまま流用(意味の重複するenumを増やさない)。
        (4) parseProgram()はTS版と同じく「回復してでも必ずProgramを返す」設計を
        戻り値の型(Resultを持たない)で表現した
  - [x] **parser移植(第二弾・struct/type宣言+判別可能union+match/is)** ✅ 2026-07-21実装。
        「Meshの型システムの全部乗せ」(examples/users.meshのコメントより)にあたる
        struct/type宣言・構造体リテラル・メンバーアクセス(`.field`)・is式・match式を
        追加。対象外(引き続き次回以降): ジェネリクス・レシーバ・error/jsonマーカー
        (`?`/`or`が無いと構造化エラーの旨みが薄いためセットで後回し)・
        パッケージ修飾structリテラル(import前提)・spawn/wait/chan/select・
        文字列補間・配列/mapリテラル・import/export。
        `tests/parser.test.ts`からスコープ内の8件相当を移植+実例相当の統合テスト2件を
        新規作成し全件パス(lexer+parser計41件)。`examples/*.mesh`は4/11本が完全に
        パース成功(前弾の2本から倍増)、うち`discriminated_union.mesh`/`users.mesh`は
        struct/union/match/isを全部通過し文字列補間だけで止まることを確認(次弾の
        対象がまさにそこだと裏付けられた)。
        **実装中に見つけた当初スコープの見落とし2件**(実例で組んでみて発覚): (1) 判別可能
        unionのタグ(`{ kind: "ok" }`の`"ok"`部分)に必須な文字列リテラル型
        (`TypeNode::Literal`)を入れ忘れていた — struct/union対応に含めていたつもりが
        漏れていたので追加。(2) 値としての`none`(`Expr::None`。`return none`等)も
        見落としていた — Meshの核心語彙なので同様に追加。段階的に切っていくスコープの
        境界は「実際に典型的なコードを組んでみるまで正確に見積もれない」という教訓
  - [x] **parser移植(第三弾・文字列補間)** ✅ 2026-07-22実装。lexer側(`StringPart::Text`/
        `StringPart::Expr`への分解)は前弾までに既に移植済みだったため、今回は
        `ast.rs`に`Expr::Interp`/`InterpSegment`を追加し、`parser.rs`の`parse_primary`で
        TS版(`parsePrimary`)と同じ「式の断片を`lex()`で再字句解析し、新しい`Parser`で
        `parse_standalone_expr()`(式1つ+EOFを要求)を呼んでASTに組み込む」処理を1:1移植する
        だけで完了。新しい設計判断は不要だった(TS版で既に決着済みの設計をそのまま運ぶ形)。
        `tests/parser.test.ts`相当のテスト5件(部品分解・メンバーアクセスを含む式・
        元ソース上の位置情報・式部分の構文エラー伝播・式が複数あるときのエラー)を新規作成、
        既存の統合テストにも実例(`"found: ${res.user.name}"`)を追加(lexer+parser計46件、
        全件パス)。`examples/*.mesh`は6/11本が完全パース成功(前弾の4本から+2、
        `discriminated_union.mesh`/`users.mesh`が前弾の見立て通り通った)。
        残る5本(`channel_spec`/`channels`はchan型、`errors`は`or`束縛形、`maps`はmapリテラル、
        `modules_demo`はimport/export)は全てスコープ外の構文で構文エラーになることを確認—
        次の対象の優先順位付けにそのまま使える。**code reviewで見つかった2点は別PRで対応**:
        (1) 深いネスト補間が本物のスタックオーバーフロー(プロセスクラッシュ)を起こす
        問題を`interp_depth`カウンタ(上限64)で修正 — TS版は同じ再帰設計だがJSのスタック
        超過は捕捉可能な例外なのに対し、Rustのそれは捕捉不能なプロセス強制終了なので、
        移植によって深刻度が格上げされていた点が要点。(2) 補間式の余ったトークンが
        「補間つき文字列自身」だと、そのトークンの`value`が空文字列であるがゆえに
        `unexpected ''`という中身の分からないエラーになっていた問題を、EOFは"end of file"・
        `value`が空なら種別名にフォールバックする`describe_token`ヘルパーに統一して修正
        (`expect`/`parse_standalone_expr`/`parse_primary`の3箇所がバラバラに同種のロジックを
        持っていたのも合わせて解消)。テストは46→48→49件。**その後のPR(#6)のcode reviewで
        見つかった残り2件も対応**(2026-07-22): (1) `describe_token`が値を引用符で囲む際に
        `'`をエスケープしておらず、値に`'`を含むトークンだと`unexpected 'it's a test'`の
        ように引用符の対応が崩れて表示が壊れる問題を修正。(2) 「補間つき文字列トークンか」の
        判定に`value.is_empty()`を代理指標として使っていたため、素の空文字列リテラル`""`
        (valueも空文字列)と衝突していた問題を`parts.is_some()`判定に変更して解消。
        (3) EOF文言がTS版の意図的な非対称性(`expect()`は"end of file"・`parsePrimary`は
        `'eof'`)から外れて両方"end of file"に統一された点も指摘されたが、動作影響なしかつ
        新しい統一挙動を前提にテストが書かれているため、意図的な簡略化として現状維持を選択
        (直さなかった)。テストは49→51件
  - [x] **parser移植(第四弾・並行処理)** ✅ 2026-07-22実装(kanayamaと討議のうえ、
        4候補〈並行処理/import・export/ジェネリクス+レシーバ/error・json構造化エラー〉の中から
        並行処理を採用——READMEの一番最初のサンプルがこの機能で、Meshの看板そのものなため)。
        `chan<T>`型(`parse_type_atom`)・`spawn`/`detach`式(`parse_unary`。2段スコープ設計は
        TS版のコメントをそのまま踏襲)・`wait`文(`parse_statement`)・`send`文`ch <- v`
        (`parse_simple_stmt`)・`recv`式`<-ch`(`parse_unary`)・`select`式(`parse_primary`。
        アーム`name := <-ch => body`+ default `_ => body`は最大1つ)・`chan<T>(capacity)`生成式
        (`parse_primary`。F-11: 容量は`none`か整数式で常に明示必須、省略はエラー)を追加。
        TS版(`parser.ts`)の該当箇所をほぼ1:1移植するだけで、新しい設計判断は不要だった。
        `tests/parser.test.ts`相当のテスト10件+実例テスト2件(chan型のfnシグネチャ利用・
        `examples/channels.mesh`簡略版)を新規作成(51→63件、全件パス)。`cargo clippy`クリーン。
        `examples/*.mesh`は8/11本が完全パース成功(前弾の6本から+2、`channel_spec.mesh`/
        `channels.mesh`が想定通り通った)。残る3本(`errors`は`or`束縛形、`maps`はmapリテラル、
        `modules_demo`はimport/export)は引き続きスコープ外の構文で構文エラーになることを確認。
        配列型サフィックス(`chan<int>[]`)はarray型自体が未実装のためスコープ外のまま
        (TS版の対応するテストも見送った)
  - [x] **parser移植(第五弾・error/json構造化エラー)** ✅ 2026-07-22実装(kanayamaと討議のうえ、
        残り3候補〈import・export/ジェネリクス+レシーバ/error・json構造化エラー〉の中から採用。
        `errors.mesh`が新たに完全パースできるようになる)。`?`伝播式(`parse_postfix`。
        直後が文字列リテラルなら文脈つき`f() ? "ctx"`)・`or`束縛形(`parse_binary`。
        優先順位表に`Or => 1`を追加——TS版の`PRECEDENCE`表と同じく全演算子中最弱結合。
        `name := ` の形が続けば束縛、なければ単純な既定値式)を追加。TS版(`parser.ts`)の
        該当箇所をほぼ1:1移植するだけで、新しい設計判断は不要だった。`tests/parser.test.ts`
        相当のテスト1件相当を4件に分けて移植+実例テスト1件(`errors.mesh`簡略版)を
        新規作成(63→67件、全件パス)。`cargo clippy`クリーン。`examples/*.mesh`は9/11本が
        完全パース成功(前弾の8本から+1、`errors.mesh`が想定通り通った)。残る2本(`maps`は
        mapリテラル、`modules_demo`はimport/export)は引き続きスコープ外の構文で構文エラーに
        なることを確認。`error struct X {...}`/`json struct X {...}`宣言マーカーは対象外の
        まま(checkerが無いと`isError`/`isJson`フラグの使い道が無いため、checker移植まで後回し)
  - [x] **parser移植(第六弾・import/export)** ✅ 2026-07-22実装(kanayamaと討議のうえ、
        残り2候補〈import・export/ジェネリクス+レシーバ〉の中からimport/exportを採用。
        `modules_demo.mesh`が新たに完全パースできるようになる)。`import "path"`宣言
        (`parse_program`。ファイル先頭にまとめる必要があり、以後に書くと`import-order`
        エラー)を追加。`export`修飾自体はfn/struct/type/トップレベル定数の`exported`
        フィールドとして以前のマイルストーンから既に実装済みだったと判明(`parse_top_level_item`
        が`self.eat(TokenType::Export)`を最初から持っていた)。パッケージ修飾型名
        (`math.User`)・パッケージ修飾呼び出し(`math.add(1, 2)`)はメンバーアクセス/呼び出しの
        通常構文で既に表現できていたため無変更。実例(`modules_demo.mesh`)を最後まで組んでみて
        判明した2つの見落としも合わせて実装:
        (1) **パッケージ修飾structリテラル**(`math.Point{x: 1, y: 2}`。`parse_postfix`に
        `Expr::Member`+`{`の組み合わせを追加、`StructLit`に`pkg: Option<String>`フィールドを追加)、
        (2) **型注釈つき変数宣言**(`x: T = v` / `mut best: string | none = none`。
        `parse_simple_stmt`の先頭に追加、新規`Stmt::TypedVarDecl`)——どちらもimport自体とは
        独立した小さな機能だが、前回milestone 3の教訓(「実際に典型的なコード片を組んでみるまで
        スコープの見落としに気づけない」)通り、実例を最後まで通そうとして発覚したため
        同じPRでまとめて追加した。`tests/parser.test.ts`相当のテスト4件+自作テスト2件
        (型注釈つき変数宣言・実例テスト)を新規作成(67→73件、全件パス)。`cargo clippy`クリーン。
        `examples/*.mesh`は10/11本が完全パース成功(前弾の9本から+1、`modules_demo.mesh`が
        想定通り通った)。残る1本(`maps.mesh`)はmapリテラルが未実装のため引き続き構文エラーになる。
        **PR #10のcode reviewで見つかった2件**: (1) 既存テスト「モジュール_修飾型名math_user
        をパースできる」のコメントが「pkg修飾structリテラルはimportが要るので次回以降」と
        書いていたが、このPR自体でその機能を実装したため矛盾していた——新設した
        「モジュール_修飾型名math_userと修飾structリテラルmath_pointをパースできる」が完全に
        上位互換なので、重複テストごと削除して解消(73→72件)。(2) `fn main() {}\nimport "x"`
        (宣言の後にimport)を投げると、正しい`import-order`エラーに加えて紛らわしい2件目の
        `invalid-top-level-declaration`エラーが連鎖する(`sync_to_top_level`が`Import`を
        停止トークン集合に含むため、復帰時に前進せず「前進保証」の1トークン強制スキップが
        `import`キーワード自体だけを飛ばしてパス文字列の手前で止まり、そこが不正な宣言として
        再度エラーになる)——**TS版(`src/parser.ts`)で同じ入力を実際に動かして確認したところ
        byte-identicalに同じ2重エラーが出ることを確認**(`syncToTopLevel`の停止トークン集合・
        前進保証ロジックが完全に同一設計のため)。移植固有の退行ではなくTS版由来の忠実な挙動と
        判断し、修正は見送った(直すならTS版側から意図的に変える設計判断が別途必要)
  - [x] **parser移植(第八弾・ジェネリクス+レシーバ)** ✅ 2026-07-22実装(kanayamaと討議のうえ、
        最後に残っていた候補〈ジェネリクス+レシーバ〉を採用。これで4候補すべて実装完了)。
        `fn first<T>(...)`(型パラメータ。トップレベル関数限定、`FnDecl.type_params`)・
        `fn (u: User) describe() ...`(Goスタイルのメソッドレシーバ。`FnDecl.receiver`+新規
        `Receiver`構造体)を追加。`export fn (u: User) ...`は`method-export-redundant`エラーに
        誘導(メソッドの可視性はstructに従うため個別exportは無意味)。レシーバとgenericsは
        併用不可(v1はfn限定。レシーバがあるとtype_paramsのパース自体をスキップする)。
        型パラメータ(`T`)を型位置で使う場合も通常の型名(`TypeNode::Name`)と構文上見分かず、
        レシーバのメソッド呼び出し(`obj.method(args)`)も通常のメンバーアクセス+呼び出しと
        構文上見分かないため、どちらも追加の式構文なしで既に表現できていた——TS版
        (`parser.ts`)の該当箇所をほぼ1:1移植するだけで、新しい設計判断は不要だった。
        `tests/parser.test.ts`相当のテスト(TS版は配列型/map型/fn型を使っていたが未実装の
        ためNameとunion型のみの簡略版に変更)+自作テスト(レシーバ・export誘導エラー・
        実例テスト)を新規作成(72→76件、全件パス)。`cargo clippy`クリーン。
        `examples/mathutil/point.mesh`(レシーバメソッド`fn (p: Point) magnitudeSq() int`を含む。
        `examples/*.mesh`11本のカウントには入らないmathutil系ファイル)が完全パース成功したことを
        確認。`examples/*.mesh`11本の集計自体は変化なし(残る`maps.mesh`はmapリテラルが原因で
        ジェネリクスとは無関係)。**これでRust移植着手時に挙がった4候補
        (並行処理/error・json構造化エラー/import・export/ジェネリクス+レシーバ)が全て完了**。
        残るギャップは配列/mapリテラル・defer・添字アクセス・範囲for・error/jsonマーカーと、
        checker/codegen自体の移植(現状はパーサのみ)
  - [x] **parser移植(第九弾・配列/mapリテラル+添字アクセス+範囲for)** ✅ 2026-07-22実装
        (kanayamaと討議のうえ、残る候補群の中から`maps.mesh`(examples/*.mesh 11本のうち
        唯一未対応だった1本)を最後まで通す一括りとして採用)。追加した構文:
        配列型`T[]`(`parse_array_suffix`。要素型が`chan<T>`/`map<K,V>`でも効く)・
        配列リテラル`[1, 2, 3]`(空は`[]`)・型付き配列リテラル`Todo[]{}`/`int[]{1, 2}`/
        `int[][]{...}`(F-9a: 空の型付き配列`T[]{}`は廃止済みで`empty-typed-array-literal-removed`
        エラーに誘導)・map型`map<K, V>`・mapリテラル`map<K, V>{"a": 1}`(`map`は文脈依存
        キーワード——`<`が続けば型/リテラル、それ以外は素の識別子に読み替えて`map(arr, f)`
        のような組み込み高階関数呼び出しと衝突しない)・添字アクセス`a[i]`(代入先としても可、
        `invalid-assignment-target`検査に`Index`/`Member`を追加)・範囲for`for i, v := range arr`/
        `for k, v := range m`/`for i := range 10`。TS版(`parser.ts`)の該当箇所をほぼ1:1移植
        するだけで、新しい設計判断は不要だった。
        **実装中に発見・修正した実バグ1件**: 型付き配列リテラル(`parse_postfix`)とmap
        リテラル/配列リテラル(`parse_primary`)の処理をそのまま関数本体にインライン展開したところ、
        既存の`文字列補間_上限を超えるネストはクラッシュせず構文エラーになる`回帰テストが
        **本物のスタックオーバーフローで実際にクラッシュするようになった**——文字列補間の
        再帰パースは`parse_primary`/`parse_postfix`を経由するため、これらの関数のスタック
        フレームサイズが`MAX_INTERP_DEPTH`(上限64)の安全マージンに直結しており、新規追加分の
        局所変数(dims/elem_type/elems/key/value/entries等)がフレームを肥大化させて安全マージンを
        食い潰していた。3箇所とも局所変数を専用の関数(`try_parse_typed_array_literal`/
        `parse_array_literal`/`parse_map_literal_or_ident`)に追い出し、それらの関数を実際に
        呼び出したときだけ専用スタックフレームが積まれる形に変更して解消(該当分岐が呼ばれない
        限りフレームサイズに影響しない)。code reviewではなく自己検証(`cargo test`)で発見。
        `tests/parser.test.ts`相当のテスト2件+自作テスト6件(型付き配列リテラル・map式位置での
        裸識別子扱い・添字読み書き・範囲for3形態・実例テスト)を新規作成(76→84件、全件パス)。
        `cargo clippy`クリーン。**`examples/*.mesh`11本 + mathutil系2本が全て完全パース成功**
        (`maps.mesh`が最後の1本として想定通り通った)。
        **対象外のまま**: defer・`error struct`/`json struct`宣言マーカー
  - [x] **parser移植(第十弾・defer文+error/jsonマーカー)** ✅ 2026-07-22実装(kanayamaと討議の
        うえ、残っていた最後の2件をまとめて採用)。`defer f(x)`(`parse_statement`。呼び出しか
        どうかの検証はcheckerに一本化——パーサは任意の式を受け取るだけ、TS版と同じ設計)・
        `error type X = ...`/`error struct X {...}`(`TypeDecl.is_error`。`?`/`or`の伝播対象と
        する意味論はchecker側)・`json struct X {...}`(`TypeDecl.is_json`。`decode<X>`自動生成も
        checker側)・`json type`は`json-type-not-supported`エラーに誘導(union の自動デコードは
        メンバー選択ロジックが要り複雑なため対象外、手書きデコーダへ誘導)を追加。
        `error`/`json`はどちらも予約語ではなく、直後が`type`/`struct`のときだけマーカーとして
        読む文脈依存キーワード(1トークン先読みで曖昧さなく判定)。`bare-struct-shape`エラーの
        自動fix提案は`is_error`付きだと出さない(struct化でerrorマーカーが消えて紛らわしいため。
        TS版から踏襲)。TS版(`parser.ts`)の該当箇所をほぼ1:1移植するだけで、新しい設計判断は
        不要だった。`tests/parser.test.ts`相当のテスト1件+自作テスト3件(defer・json struct/
        json type・error付きbare-struct-shapeのfix抑制)を新規作成(84→88件、全件パス)。
        `cargo clippy`クリーン。`examples/*.mesh`11本+mathutil系2本は変化なく全て完全パース成功
        (どの例もdefer/error-json構文を使っていないため)。
        **スコープ調査中に発見した新しい未着手項目**: 関数型注釈(`fn(int, string) bool`)と
        無名関数式(`fn(x: int) int { return x * 2 }`。`Expr::FnExpr`/`TypeNode::FnType`)が
        まだ移植されていないと判明——今回のマイルストーンの対象ではなかったため次回以降に
        持ち越し(`examples/*.mesh`のどれも無名関数を使っていないため、これまで見落とされていた)
  - [x] **parser移植(第十一弾・関数型注釈+無名関数式)** ✅ 2026-07-22実装(kanayamaと討議のうえ、
        milestone 10のスコープ調査で発覚した最後の1件を採用——**これでparser.tsを全面移植完了**)。
        関数型注釈`fn(int, string) bool`(`parse_type_atom`。宣言と同じ読みで戻り値のunionは
        戻り値側に束縛。パラメータ名を書くと`fn-type-with-param-names`エラーに誘導)・
        `(T)`型グループ化(同じく`parse_type_atom`。`(fn(int) int) | none`のように関数型自体を
        unionに入れるときの曖昧さ解消——地味だがfn型と対にして移植が必要だった見落としがちな部品)・
        無名関数式`fn(x: int) int { return x * 2 }`(`parse_primary`の新規`Fn`ケース。
        パラメータ・戻り値・本体は既存の`parse_params`/`parse_return_type`/`parse_block`を
        そのまま再利用——`fn`宣言と共通のヘルパーなので実装量はごく小さい)を追加。
        戻り値の有無判定用に`can_start_type`ヘルパーも新設(TS版の`canStartType()`を移植)。
        TS版(`parser.ts`)の該当箇所をほぼ1:1移植するだけで、新しい設計判断は不要だった。
        **milestone 9の教訓(スタックフレームサイズ)を踏まえ、実装直後に`cargo test`で
        文字列補間の深さ回帰テストを5回連続実行して安全マージンを確認**——今回は
        `parse_primary`の新規ケースが既存ヘルパー呼び出しへの委譲のみで局所変数が少なく、
        別関数への切り出しは不要と判断(実測でクラッシュしないことを確認済み)。
        `tests/parser.test.ts`相当のテスト2件+自作テスト2件(無名関数式・型注釈つき無名関数の
        代入の実例テスト)を新規作成(88→92件、全件パス)。`cargo clippy`クリーン。
        `examples/*.mesh`11本+mathutil系2本は変化なく全て完全パース成功(どの例も無名関数を
        使っていないため)。**これでRust版パーサはTS版parser.ts(1217行)の全機能を
        カバーした**(対象外の構文が無い状態)。残るのはchecker/codegen自体の移植のみ
  - [x] **checker(最小リゾルバ)+codegen移植(milestone 1・スカラーのMesh)** ✅ 2026-07-22実装
        (kanayamaと討議のうえ、フルchecker〈約2900行〉を先に移植するのではなく、codegenが
        必要とする最小限の型情報だけを解決する「最小リゾルバ」を先に作り、そのうえでcodegenに
        進む方針を採用。方針決定の経緯・アーキテクチャ全体は承認済みの計画書を参照——概要は
        本項目末尾に転記)。**目標達成**: `examples/hello.mesh`と`examples/fizzbuzz.mesh`を
        Rust版で最後まで実行し、生成JSを`bun`で走らせてTS版(`bun run mesh run`)の標準出力と
        完全一致することを確認した。
        - `rust/src/types.rs`(新規): `src/types.ts`(246行)の型システム移植。自己参照型
          (`struct Node { left: Node, ... }`・`json.Value`)は`Box<Type>`の所有権モデルでは
          表現できないため、struct milestone(2以降)まで意図的に先送り(型ファイル冒頭に
          理由を明記。伴ってTS版`typeEquals`の循環ガード`seen`も今回は不要)
        - `rust/src/checker.rs`(新規): TS版`src/checker/`のフェーズ1〜2相当(宣言収集)+
          フェーズ5の必要最小限(式推論)を移植した「最小リゾルバ」。**診断は一切出さない**
          (パーサを通った時点で構文的に正しい前提。未解決の型は`Type::Any`へ最善努力で
          フォールバックし、コンパイラ自体をpanicさせない)。
        - `rust/src/codegen.rs`(新規): TS版`src/codegen.ts`(762行)のmilestone 1部分
          (struct/map/channel/並行処理/エラー伝播/パッケージ抜きの「スカラーのMesh」)を移植。
          対象外の構文(struct/map/channel/spawn/`?`/`or`/import等)は明確な
          `Err("codegen: ... is not yet supported")`を返す——構文はパーサで既にパースできるが
          コンパイラをクラッシュさせない、これまでと同じ設計哲学を踏襲。
        - **Rust固有の設計判断(TS版からの意図的な逸脱)**: TS版は`expr.resolvedType = t`の
          ようにASTノードへ直接書き込み、codegenが後から読む「checker→codegen 2パス+
          共有ミュータブルAST」設計だが、Rustの`Expr`は不変構造体でこのパターンに向かない。
          代わりに**resolverとcodegenを1回のトラバーサルに融合**した——codegenが式を生成する
          直前に、その場で`checker::infer_expr`/`infer_binary`を呼んで必要な型情報だけを
          得る。TS側が2パスに分けていたのは主に「型エラーを1回で全部集めて報告する」ため
          (診断目的)だが、このリゾルバは診断を出さない設計なのでこの制約自体が無く、
          融合して問題なかった。
        - **PRELUDE(ランタイム)の扱いで見つかった実装上の落とし穴**: `src/runtime.ts`は
          TSファイルであり、ランタイムJS本体を`export const PRELUDE = \`...\`;`という
          テンプレートリテラルで包んでいる。`include_str!`で素朴にファイル全体を埋め込むと、
          このTSの宣言構文(`export const PRELUDE = `や末尾の`;`)まで生成JSに混ざって
          構文エラーになる実バグを実装中に発見——ファイル内でバッククォートが
          開始・終了の2箇所にしか現れないこと(ランタイム本体は文字列連結のみで書かれ
          テンプレートリテラルを使っていない)を確認したうえで、その2箇所の間だけを
          `find`/`rfind`で切り出す方式で解決した。二重管理・意味のズレを避けるため、
          今後もランタイム本体は`src/runtime.ts`側でのみ編集する
        - **その他の設計判断**: 複合代入(`x += 1`)は代入先を`infer_binary`の左辺式として
          そのまま渡し、int/float分類ロジック(`__idiv`/`__imod`/`__iarith`)をBinary式と
          共有。トップレベル定数の型は「型注釈があればそちら、無ければ値から推論」
          (TS版`checker/modules.ts`の`declared ?? valueType`と同じ優先順位)。
          Cスタイルforのヘッダ変数(init/post)は`mutable`フラグを見ず常に`let`で出す
          (TS版`genSimpleStmt`が`stmt.mutable`を無視して常に`let`を出すのと同じ挙動)。
        - CLI: `cargo run -- file.mesh --emit-js`で生成JSを標準出力へ書き出すモードを追加
          (`--emit-js`が無ければ従来通りASTダンプ)。
        - テスト: types.rs 11件・checker.rs 11件・codegen.rs 16件を新規作成
          (92→130件、全件パス)。`cargo clippy --all-targets -- -D warnings`クリーン。
        - **milestone 1のスコープ外(意図的)**: struct/メソッド・map/配列・channel/並行処理・
          `?`/`or`エラー伝播・import/export・パッケージ・ジェネリクス。パーサは全て
          パースできるが、codegenはこれらに出会うと明確なエラーを返す。次のmilestone以降で
          順に対応していく想定(struct/メソッド → error/json → 配列/map → 並行処理 →
          モジュール、の順で`examples/*.mesh`を1本ずつ動かす計画)
        - **追記(PR #16のcode reviewで発覚・同PR内で修正)**: (1) 組み込み関数を引数不足で
          呼ぶ(`round()`等)とパニックしていた——`gen_builtin_call`が個数検査無しで
          `args[0]`/`args[1]`へ直接インデックスしていたのが原因。呼び出し前に個数を検査し
          明確な`Err`を返すよう修正。(2) `round`/`floor`/`ceil`/`toInt`の戻り値型が
          `infer_call`で解決されずANYへ落ちていたため、その結果同士の演算
          (`round(5.0) / round(2.0)`)が本来のint除算(`__idiv`、結果2)ではなく
          浮動小数点除算(結果2.5)になっていた——組み込みの戻り値型を引く
          `infer_builtin_call`を追加して解決。テスト130→133件
  - [x] **checker+codegen milestone 2(struct宣言 + レシーバメソッド)** ✅ 2026-07-22実装
        (kanayamaと確認済みの順序——struct/メソッド → error/json → 配列/map → 並行処理 →
        モジュール——の最初)。TS版の該当実装(knot-tying・methodTable・フィールド/メソッド
        判別・struct関連codegen)を2本のExploreエージェント+1本のPlanエージェントで調査し、
        Plan Mode経由で承認を得たうえで実装。
        - **自己参照型を避ける設計判断**: TS版はstructを「空fieldsの殻を先に作ってmapへ登録し、
          あとから`.push()`で埋める」knot-tyingで自己参照型(`struct Node { next: Node }`)を
          表現するが、Rustの`Type::Struct{fields: Vec<StructField>}`は所有権ベースの木なので
          この「同じオブジェクトを後から書き換える」パターンに向かない(`Rc<RefCell<>>`が要る
          ——`types.rs`冒頭のコメントで将来のmilestoneへ先送り済みの判断)。代わりに
          **固定点反復**(`checker::resolve_struct_decls`)で解決する: `N = types.len()`回、
          現時点のレジストリを使って全struct宣言のfieldsを再解決するのを繰り返す——非循環
          (DAG)なら宣言順に関係なく必ず収束する。ただし循環(自己参照含む)は固定点反復では
          「クラッシュしないが深さが毎パス線形に伸びる中途半端な入れ子」になり「自己参照は
          未対応」という前提を静かに裏切ってしまうため、固定点反復の前に生のTypeNode参照
          関係だけを見た軽量なDFSサイクル検出を挟み、循環があれば明確な`Err`を返す。
        - `checker.rs`: `CheckerCtx`に`struct_types`(名前→解決済みstruct型)・`method_table`
          (struct名→メソッド名→関数型、レシーバを第1引数として含む)を追加。
          `resolve_type_node`/`resolve_return_type`は`ctx`を取るよう変更(`struct_types`を
          引けるようにするため)。`infer_expr`に`Expr::StructLit`(名前でstruct_typesを引く)・
          `Expr::Member`(targetがstructならfieldsから型を引く)を追加。`infer_call`に
          メソッド呼び出しの解決を追加(TS版`calls.ts`と同じ「フィールドが勝つ」順序——
          targetがstruct型でnameが宣言済みフィールドでなければメソッドとして解決)。
        - `codegen.rs`: struct宣言ごとの検査(error/json付き・非structは引き続き明確な
          `Err`)+`resolve_struct_decls`呼び出しを`generate_all`に追加。レシーバの型が
          未宣言/非struct型(`fn (x: int) foo()`等)なら、殻へ静かにフォールバックさせず
          明確な`Err`(でないと`__m_int_foo`のようなおかしなJS関数名を生成してしまう)。
          `gen_expr`に`Expr::StructLit`(オブジェクトリテラル。リテラルに書かれた順)・
          `Expr::Member`(フィールド読み。targetがstruct型かつnameが宣言済みフィールドの
          ときだけ許可——さもなくば「まだ対応していません」。パッケージ修飾参照
          〈`math.add`〉が実行時ReferenceErrorになる素のJSを静かに生成しないためのガード)
          を追加。`gen_call`にメソッド呼び出し分岐(`__m_Struct_method(recv, args)`。
          structだがfieldにもmethodにも無い名前は明確な`Err`)を追加。新設
          `gen_lvalue`ヘルパーで`Stmt::Assign`/`Stmt::IncDec`が`Expr::Member`ターゲット
          (フィールドの読み書き`u.age = ...`/`u.age += 1`)にも対応。
        - **`__proto__`ガード**: TS版が過去に実際に踏んだprototype汚染バグ(struct
          リテラルの素朴なobject literal化で`__proto__`フィールドがJSのプロトタイプ
          チェーンを汚染しうる)の再発防止として、struct literalのフィールド名と
          代入先のフィールド名の両方で`__proto__`を明確な`Err`にした(前者はTS版の
          checkFieldNameが担っていた保護、後者はTS版に無かった新しい代入経路——
          milestone 2で新規追加したフィールド書き込み機能に伴う、TS版には無い攻撃面)。
        - テスト: checker.rs 5件(前方参照structの解決・相互循環/自己参照structの検出・
          struct_lit/フィールドアクセスの型推論・メソッド戻り値型)・codegen.rs 9件
          (struct literal→フィールド読み書き・メソッド呼び出し・生成直後のリテラルへの
          チェーン呼び出し・フィールドと同名メソッドはフィールドが勝つ・`__proto__`拒否
          2件・未宣言レシーバ/未知メソッドの明確なエラー・error struct/パッケージ修飾
          struct literalは引き続き未対応)を新規作成(133→146件、全件パス)。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **実行確認**: 新規`examples/struct_methods.mesh`(README記載の`Todo`例——
          生成直後のリテラルへの直接メソッドチェーン込み——+ `User`構造体でのフィールド
          直接変更・複合代入・文字列補間)をRust版で実行し、`bun run mesh run`(TS版)の
          出力とbyte-for-byte一致することを確認。既存の`hello.mesh`/`fizzbuzz.mesh`も
          回帰無しを再確認。`examples/mathutil/point.mesh`(レシーバメソッド)も単体で
          クラッシュなくコンパイルできることを確認(import自体は引き続き対象外)。
        - **milestone 2のスコープ外(意図的)**: error/jsonマーカー付きstruct・判別可能union/
          `type X = A | B`・`match`/`is`式・パッケージ修飾structリテラル/メソッド
          (`math.Point{...}`)・配列/map・並行処理・`?`/`or`。次のmilestone(error/json)
          以降で順に対応する
        - **PR #17のcode reviewで見つかった検証漏れ3件(未修正・既知の限界として記録)**:
          5エージェントのレビューで(1) struct literalのフィールド名/値が宣言済みfieldsと
          照合されない(タイポ`User{nam: "x"}`・フィールド欠落・型不一致がいずれも無診断で
          コンパイルされ、実行時に`undefined`(`none`表示)や紛らわしいpanicになる)、
          (2) 代入先(`gen_lvalue`)はフィールド名を検証しない——read/callパス(`gen_expr`の
          `Expr::Member`・`gen_call`)は同じ判定を行っているのに、書き込み側だけ非対称に
          漏れている(`u.存在しないフィールド = x`が無診断でコンパイルされる)、
          (3) `__proto__`ガードはstruct literal/代入先の2箇所にしか無く、TS版
          (`checkFieldName`)が持つ**struct宣言時点**のガードが移植されていない
          (`struct Evil { __proto__: string }`自体は宣言できてしまい、後で`e.__proto__`と
          読むと本物のJSプロトタイプオブジェクトが返る)——の3件が見つかった。
          各々を独立検証エージェントでスコアリングした結果、80点未満(75・75・25)で
          ブロック対象外と判定(このリゾルバが明言する「診断は出さない」設計
          ——通常の関数呼び出しの引数個数/型も同様に検証しない——と整合的、という判断。
          PR #16の2件〈パニック・誤ったint/float演算〉のような「コンパイラ自身の判断ロジックが
          間違っている」バグとは性質が異なる)。**修正はせず既知の限界として記録**
          (kanayama確認済み、2026-07-22)。将来、error/json以降のmilestoneで診断機構を
          本格的に入れる際にまとめて対応する候補
          **→ (1)はchecker+codegen milestone 12(struct literalのフィールド検証、
          2026-07-24)で解消。(2)(gen_lvalueの代入先フィールド名検証)・(3)(struct宣言
          時点の`__proto__`ガード)は milestone 12のスコープ外のまま未対応で残る**
  - [x] **checker+codegen milestone 3(`?`/`or`/`error struct`)** ✅ 2026-07-22実装
        (kanayamaと確認済みの順序——struct/メソッド → error/json → 配列/map → 並行処理 →
        モジュール——の3番目)。`error type X = A | B`(union形式)・`json struct`/`json type`・
        `match`/`is`式・判別可能union・配列/map・パッケージ修飾は引き続き対象外
        (構文はパーサ済みだがcodegenは明確な「まだ対応していません」を返す)。
        - **milestone 2の実バグを発見・修正**: `resolve_struct_decls`のフィルタが
          `!t.is_error && !t.is_json && ...`になっており、`error struct`宣言そのものを
          丸ごと無視していた(milestone 2実装時の見落とし)。`!t.is_json && ...`に修正する
          だけで、struct構築コードが既に持っていた`is_error_type: decl.is_error`が
          正しく効くようになった(新しいタグ付けロジックは不要だった)。
        - `checker.rs`: `is_failure_type`(TS版`isFailureType`——none/errorに加えて
          error struct/error typeでタグ付けされたstructも「失敗」とみなす)・
          `or_binding_type`(`or e => ...`の`e`の型。**TS版の実際の挙動を忠実に移植**——
          unionでない被演算子は無条件でANYになるという、一見「賢くない」挙動もそのまま
          踏襲した)・`has_structured_failure`(**Rust版だけの追加ガード**、下記参照)を追加。
          `infer_expr`に`Expr::Prop`/`Expr::OrElse`(結果型はどちらも「被演算子の失敗
          メンバーを除いた残り」——TS版と同じ式で、contextやright/bindingの中身は
          結果型に影響しない)を追加。
        - `codegen.rs`: `generate_all`のTypeDecl拒否を「json structのみ拒否」に変更
          (`error type`のunion形式は`node`がUnionなので既存の「StructTypeのみ許可」
          チェックに自動的に引っかかり、追加のロジック無しで対象外のまま)。
          `gen_fn_decl`を「本体をいったん別バッファに生成し、`?`を使ったかどうかを
          事後に見てtry/catchで包むか決める」形に書き換えた(TS版`genFnBody`の
          `propStack`と同じ設計。Rustでは`mem::take`/`mem::replace`で代用——
          `Expr::FnExpr`がまだ未対応で関数本体生成がネストしないため、スタックではなく
          単一のフラグ〈`Codegen.prop_used`〉で足りる)。`Expr::Prop`(`__prop(...)`/
          `__propCtx(..., async () => ...)`)・`Expr::OrElse`(`__or(left, async (binding) =>
          right)`。bindingをスコープへ`or_binding_type`で宣言してからrightを生成)・
          `Expr::StructLit`の`__errTag`ラップ(`ctx.lookup_struct`が`is_error_type:true`を
          返すときだけ)を実装。
        - **Rust版だけの安全ガード(`has_structured_failure`)**: TS版の`?`のcontext形式
          diagnosticはunion内のケースしか見ないが、ランタイムの`__propCtx`は
          `null`/`instanceof Error`しか特別扱いせず、`__ERR`タグ付きの構造化errorは
          素通りして「成功扱い」になってしまう(`src/runtime.ts`参照)。TS版はこの
          組み合わせ自体を型検査で弾くので実害が無いが、診断を出さないこのリゾルバでは
          ここで拾わないと実行時に静かに壊れた挙動になるため、意図的にTS版より広い
          (bareの構造化errorも含めて再帰的に検出する)ガードを追加し、明確な`Err`にした。
        - テスト: `checker.rs`に6件(is_failure_type・resolve_struct_declsのerror struct
          解決・infer_exprのProp/OrElse・or_binding_type・has_structured_failure)、
          `codegen.rs`に既存1件を置き換え+新規11件(error structの`__errTag`・通常structは
          包まれない・`error type`/`json struct`は引き続き未対応・bare/context付き`?`・
          構造化errorへのcontext付き`?`は明確なエラー・`?`を使う/使わない関数のtry/catch
          有無・ネストした`?`でも関数レベルで包まれる・`or`の3形態〈裸/`_ =>`/`e =>`〉、
          束縛でのフィールドアクセス)を追加(150→162件、全件パス)。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **実行確認**: 新規`examples/error_propagation.mesh`(`error struct`+`divide`/
          `lookup`/`find`+bareとcontext付きの`?`+`or`の3形態、既存の`examples/errors.mesh`は
          `is`/`match`を使うため対象外のまま変更せず)をRust版で実行し、`bun run mesh run`
          (TS版)の出力とbyte-for-byte一致を確認。**検証中に踏んだ罠**: TS版は`or`の
          fallback式の型を成功側の残り型と照合する(`or-fallback-type-mismatch`)ため、
          診断を出さないRust版なら通ってしまう組み合わせ(例: `int`を期待する場所に
          `string`のfallbackを書く)を書くとTS版側でコンパイルエラーになり比較できない
          ——example作成時は必ずTS版でも成立する組み合わせにする必要がある(milestone 2
          までには無かった新しい落とし穴)。`hello.mesh`/`fizzbuzz.mesh`/
          `struct_methods.mesh`も回帰無しを再確認。
        - **milestone 3のスコープ外(意図的)**: `error type X = A | B`(union形式)・
          `json struct`/`json type`・`match`/`is`式・判別可能union・配列/map・並行処理・
          パッケージ修飾。次のmilestone(配列/map)以降で順に対応する
        - **PR #18のcode reviewで見つかった検証漏れ1件(未修正・既知の限界として記録)**:
          PR #17で見つかっていた「struct literalの名前/フィールドが宣言と照合されない」
          という既知の限界(score 25で未修正のまま)が、今回`?`/`or`が入ったことで
          より深刻な形で現れることが2エージェント独立で確認された——struct名を
          タイポした場合(例: 戻り値型に`int | NotFoundTypo`と書いたが`NotFoundTypo`という
          structは実在せず、正しくは`NotFound`)、`ctx.lookup_struct`がどこでも見つからず
          `is_error_type: false`の殻へ静かにフォールバックするため、structリテラルが
          `__errTag`で包まれない。結果、`x := find()?`の`?`が実行時の`__isFailureValue`
          チェック(`__ERR`タグを見る)に引っかからず、**本来伝播すべき失敗値を
          コンパイルエラーもクラッシュも無く「成功」として素通りさせてしまう**
          (このmilestoneで追加した`has_structured_failure`ガード自身の目的を、
          根本原因の別の穴から回避してしまう形)。独立検証で75点(80点未満)と判定——
          PR #17で既に受け入れ済みの限界の別の現れ方であり、このPR自体が新しい
          誤ったロジックを持ち込んだわけではないため、という判断だが、影響が
          `?`/`or`の安全性そのものに直結するため既知の限界として明記しておく
          (2026-07-22)。PR #17の3件と合わせて、将来struct literalの検証を
          入れる際にまとめて対応する候補
          **→ milestone 12(struct literalのフィールド検証、2026-07-24)で解消**
  - [x] **checker+codegen milestone 4(配列/map)** ✅ 2026-07-22実装(kanayamaと確認済みの
        順序——struct/メソッド → error/json → 配列/map → 並行処理 → モジュール——の
        4番目)。配列リテラル・mapリテラル・添字アクセス(読み書き)・範囲for(3形態)・
        配列/map対応の組み込み関数を実装。**`filter`/`map`/`reduce`は対象外のまま**
        (無名関数〈`Expr::FnExpr`〉のcodegenがまだ無く引数を生成できないため)。
        `match`/`is`式・判別可能union・`error type`(union形式)・`json struct`・
        並行処理・パッケージ修飾は引き続き対象外。TS版の該当実装を2本のExplore
        エージェント+1本のPlanエージェントで調査し、Plan Mode経由で承認を得たうえで実装。
        - `checker.rs`: `infer_expr`に`Expr::ArrayLit`(型注釈があればそれ、無ければ
          最初の要素の型を`widen_literal`したもの、空なら`Array(ANY)`)・`Expr::MapLit`
          (key/valueは構文上常に必須)・`Expr::Index`(Mapなら`V | none`、Arrayなら
          elemそのまま——`get()`と違い`a[i]`は範囲外panicの設計なので`| none`は付けない、
          文字列ならSTRING)を追加。**`infer_call`/`infer_builtin_call`に`args: &[Expr]`を
          通すよう変更**(`get`/`sort`/`keys`/`values`は引数依存の戻り値型を持つため——
          これを直さないと例えば`sort(nums)`の戻り値型が引けず、後続の算術が`__iarith`を
          経由しないTS版とのbyte単位の食い違いになる)。新設`declare_range_for_names`で
          range-forのループ変数をsubjectの型(Array/Map/int/Any)に応じて宣言。
        - `codegen.rs`: `Expr::ArrayLit`(素のJS配列リテラル、`elem_type`はcodegenでは
          一切参照しない)・`Expr::MapLit`(`new Map([[k,v],...])`、空なら`new Map()`)・
          `Expr::Index`(Mapなら`__mget`、それ以外は`__idx`——文字列もこのまま扱える)を追加。
          `Stmt::Assign`/`Stmt::IncDec`は`Expr::Index`ターゲットを`gen_lvalue`に渡す前に
          新設`gen_index_assign`/`gen_index_incdec`へ振り分け(Mapは`.set(k,v)`、Arrayは
          `__idxset`/`__idx`)。`Stmt::RangeFor`を新設`gen_range_for`で実装
          (Map→`for (const [k,v] of subject)`、単一名→`for (let i=0,__n=subject;...)`
          〈ブランク名は`__i`にフォールバック——Cスタイルループ変数は空文字列にできないため〉、
          それ以外→`for (const [i,v] of subject.entries())`)。`gen_builtin_call`に
          `len`(Map→`.size`、それ以外→`.length`)・`delete`・`keys`/`values`を追加
          (`push`/`get`/`contains`/`indexOf`/`sort`はmilestone 1時点で配列/mapが
          無かった頃から先行して移植済みだったコードがそのまま使えた)。
        - **Rust版だけの安全ガード3件(TS版では診断のおかげで到達不能な組み合わせを、
          診断を出さないこの設計では明確なErrで守る——milestone 2/3と同じ考え方)**:
          (1) mapへの複合代入(`m[k] += 1`)——「今の値」が`V | none`であり算術の対象に
          ならない、(2) mapへのIncDec(`m[k]++`)——TS版のcodegen自体は実は無条件で
          `__idx`/`__idxset`を使うが、`isNumeric`診断で`m[k]++`自体がTS本体では
          そもそも到達しないコードだった、(3) 明確な形のsubject(Array/Map/int)に
          対するrange-forのアリティ不一致——TS版のcodegenも無条件分岐なので、
          Array+1名だと「数値と配列を比較し続けて0回で終わるループ」を、int+2名だと
          `.entries is not a function`のクラッシュを生成してしまう(いずれもTS本体の
          range-arity診断で到達不能)。**意図的なスコープ縮小**: `gen_lvalue`自体には
          Indexアームを追加せず(forヘッダ内での添字代入は明確なErrのまま)——TS版の
          `genLValue`はこの経路で無条件`target[index]`という壊れた形(mapに対しては
          `.set`を呼ばない、ただの余計なプロパティ代入)を素通しするが、これは意図的に
          移植しなかった。
        - テスト: `checker.rs`に配列/mapリテラルの型推論・添字読みの3分岐・
          `infer_call`のargs配線(get/sort/keys/values)・`declare_range_for_names`の
          4分岐(前方/部分アリティ込み)を新規追加。`codegen.rs`に既存1件を置き換え+
          新規10件(配列/mapリテラル生成・添字読み書き〈複合代入・IncDec込み〉・
          map複合代入とmap IncDecの明確なエラー・範囲for 3形態〈ブランク名・
          アリティ不一致込み〉・`len`のmap/array使い分け・`delete`/`keys`/`values`・
          `get`/`sort`)を追加(162→180件、全件パス)。`cargo clippy --all-targets --
          -D warnings`クリーン。
        - **実行確認**: 新規`examples/collections.mesh`(mapリテラル+添字読み書き+
          `or`フォールバック+`delete`+`len`+mapへの範囲for、配列リテラル+`push`/`get`/
          `contains`/`indexOf`/`sort`+配列への範囲for〈2名・ブランク名〉+intへの範囲for)
          をRust版で実行し、`bun run mesh run`(TS版)の出力とbyte-for-byte一致を確認。
          既存の`examples/maps.mesh`は`is none`を使うため変更せず、そのままだと
          `codegen: 'is' is not yet supported`で明確に失敗することを確認(クラッシュ
          しない、誠実な「未対応」)。`hello.mesh`/`fizzbuzz.mesh`/`struct_methods.mesh`/
          `error_propagation.mesh`も回帰無しを再確認。
        - **milestone 4のスコープ外(意図的)**: `filter`/`map`/`reduce`(無名関数の
          codegenが必要)・`match`/`is`式・判別可能union・`error type`(union形式)・
          `json struct`・並行処理・パッケージ修飾・forヘッダ内での添字代入。次のmilestone
          (並行処理)以降で順に対応する
        - **PR #19の5エージェントcode reviewで見つかった問題(2026-07-23)**:
          - `delete()`をmap以外(配列等)に呼ぶと`.delete()`という存在しないメソッドを
            無条件で生成し実行時に`panic: xs.delete is not a function`でクラッシュする
            バグを発見・**PR内で修正済み**(`len`と同様、確実にMap以外だと分かる場合は
            `codegen: 'delete' requires a map argument`という明確なErrにする。ANYは
            この設計の他の箇所と同じく許容——確実に「Mapではない」と分かる場合だけ弾く)。
          - ネストしたmap(`map<K, map<K2,V2>>`)への二重添字(`m["a"]["b"]`)が、milestone 4で
            追加した3件の安全ガード(map複合代入・map IncDec)と、添字読み書きの
            Map/Array判定そのものを**すり抜けてしまう**バグを発見・**PR内で修正済み**。
            原因: mapの読みは常に`V | none`(`Union`型)を返すため、内側の`m["a"]`を
            さらに添字対象にすると、その型は厳密な`Type::Map`ではなく`Type::Union`になり、
            `matches!(container_ty, Type::Map{..})`という厳密一致のガードが素通りして
            配列扱い(`__idx`/`__idxset`)になってしまっていた。TS版のchecker
            (`src/checker/expressions.ts`)を調査した結果、TS本体はそもそも`Union`型への
            添字自体を`not-indexable`診断で拒否している(noneかもしれない値へ安全に
            添字を続けられないため)と判明したため、それに倣い、添字の読み・代入・
            複合代入・IncDecの4箇所すべてで`Type::Union`のcontainerを明確な
            `codegen: cannot index into '...' — narrow away 'none' first (e.g. with 'or')`
            というErrにする形で修正(独立検証で82点)。回帰テスト追加、`cargo test`
            182件・clippyクリーンを確認済み
          - 以下3件は独立検証で75点(80点未満)——PR #17/#18で既に受け入れ済みの
            「診断を出さないリゾルバ」設計の限界の再確認であり、このPR自体が新しい
            誤ったロジックを持ち込んだわけではないため、修正せず既知の限界として
            明記するに留める(2026-07-23判断):
            - 配列/mapリテラルの要素/エントリの型がPR #17の「struct literalのフィールドが
              検証されない」と同じ理由で相互検証されない(例:
              `xs := [1, "a"]; xs[1] + 1`が黙って`__iarith`に文字列を渡す、
              `xs["foo"]`〈非intの配列添字〉も明確なエラーにならない)。将来PR #17の
              3件とまとめて対応する候補
            - `gen_lvalue`のMember(構造体フィールド)は「確実にstructだと証明できないと
              拒否」という許可リスト方式だが、`gen_index_assign`/`gen_index_incdec`は
              「確実にMap以外は配列扱い」という拒否リスト方式であり非対称——例えば
              `mut x := true; x[0] = 5`は明確なエラーにならず`__idxset(x, 0, 5, at)`を
              生成し実行時に生の`TypeError`でクラッシュする(`x.field = 5`という等価な
              Member版は正しくコンパイルエラーになる)。今回のUnion修正で「確実に
              Map以外」の一部(Union)は塞いだが、bool/float/struct等の非indexable型は
              引き続き未対応のまま
            - range-forのアリティガードはArray/Map/intの3形態にのみ反応し、それ以外の
              確定した具体型(string/bool/struct等)には反応しない(TS版の`not-rangeable`
              診断に相当するものが無い)ため、例えば`for i, v := range someStruct`が
              明確なエラーにならず`.entries is not a function`でクラッシュしうる。
              ANYの限界(既知・対象外)より広い穴だが、今回は未修正
  - [x] **checker+codegen milestone 5(並行処理)** ✅ 2026-07-23実装(kanayamaと確認済みの
        順序——struct/メソッド → error/json → 配列/map → 並行処理 → モジュール——の
        5番目)。`chan`/`spawn`/`detach`/`wait`/`select`/`<-`(recv)/`ch <- v`(send)を実装。
        パーサー・型システム(`Type::Chan`/`Type::Closed`)・ランタイム(`class __Channel`・
        `__recv`/`__select`/`__spawn`/`__detach`/`__waitStack`)は既存の仕組み
        (`include_str!`でTS版`runtime.ts`を丸ごと埋め込み)で既に揃っていたため、
        `checker.rs`の式推論と`codegen.rs`のみが対象。TS版の該当実装
        (`src/checker/expressions.ts`・`src/checker/match-select.ts`・`src/codegen.ts`)を
        2本のExploreエージェント+1本のPlanエージェントで調査・設計検証のうえ実装。
        - `checker.rs`: `infer_expr`に`Expr::Chan`(`Type::Chan(elem)`、capacityは型に
          影響しない)・`Expr::Recv`(`T | closed`、chan以外はANY)・`Expr::Spawn`
          (`detached`は見ない——戻り値がvoidならvoid、それ以外は`chan<戻り値型>`)・
          `Expr::Select`(全アーム+defaultのunion、全void→void、混在→ANY〈TS版の
          `mixed-void-arms`診断に相当〉)を追加。selectのアーム束縛名は
          `infer_expr(ctx: &CheckerCtx, ...)`が不変参照しか取らずスコープをpushできない
          ため宣言しない(`match`/`is`絞り込みが未実装のこの移植の範囲内では、束縛変数への
          型依存処理〈算術等〉を要する正当なMeshプログラムがそもそも書けないため無害。
          未解決の参照はIdent推論の既存フォールバックでANYになるだけ)。
        - `codegen.rs`: 新規`spawn_used: bool`フィールド(`prop_used`と同じ単一フラグ
          パターン——`Expr::FnExpr`未対応のため関数本体生成はネストせずスタック不要)。
          `gen_fn_decl`を`used_prop`/`used_spawn`の2フラグ合成に書き換え——
          両方falseなら従来通り無包装、`used_prop`のみなら従来通りtry/catch、
          `used_spawn`のみなら`__waitStack.push([]); try {} finally { await
          Promise.all(...) }`、両方trueならtry/catch/finallyを1つのtry文にまとめる
          (TS版`genFnBody`のprop/spawn/defer 3フラグ合成と同じ設計。`defer`
          〈TS版usesDefer相当〉は`Stmt::DeferStmt`が常にErrを返すため対象外のまま)。
          `Stmt::Wait`は中身のspawn有無を見ずに常に`__waitStack.push([]); try {} finally
          { await Promise.all(...) }`で包む(TS版と同じ無条件——関数丸ごとの暗黙wait枠とは
          独立、`__waitStack`は本物のスタックなのでネストしても正しく動く)。`Expr::Chan`は
          capacityが`Expr::None`なら`new __Channel()`、それ以外は`new __Channel(cap)`。
          `Expr::Spawn`/`Expr::Detach`は新設`gen_spawn`で処理——メソッド呼び出し
          (`spawn recv.method()`)の判定を新設`resolve_method_target`ヘルパへ切り出し、
          既存`gen_call`もこのヘルパを使うようリファクタ(挙動は変えない。TS版が
          `genCall`/`spawn`ケースの2箇所に同じ判定を重複して持つのに対し、Rust版は
          1箇所にまとめた)。`Expr::Select`は新設`gen_select`——各アームの束縛名は
          (checker側とは違い)`&mut self.ctx`があるので`OrElse`の既存束縛パターンを
          再利用して`elem型 | closed`として正しくスコープに宣言してからbodyを生成する
          (外側の同名変数〈型が違う〉をshadowする際に誤って`__iarith`等を選んでしまう
          経路を防ぐため——checker.rs側の簡略化とは対称的に、codegen側は手を抜かない)。
          `channels`/`handlers`(`(async (name) => body)`)/defaultの3引数を組み立てて
          `__select(...)`に渡す(準備待ち・公平選択のロジックは100%ランタイム側)。
        - **Rust版だけの安全ガード(TS版の`not-a-channel`診断に相当)**: `Stmt::Send`・
          `Expr::Recv`・`gen_select`の各アームchannelで、型が確実に`Type::Chan`/`Type::Any`
          のいずれでもないと分かる場合は明確なErr(`delete()`ガードと同じ考え方——ANYは
          許容、確実に非chanと分かる場合だけ弾く)。milestone 4のIndexの前例(scoreが
          低く実装コストが高いため見送った)とは異なり、今回はコストが低く
          (`matches!`1個を3箇所にコピーするだけ)、かつ新規構文なので前例通り
          最初からガード付きで出す方が一貫している、とPlanエージェントの検討を踏まえ判断。
        - テスト: `checker.rs`に4件追加(chan生成・recvのT|closed化・spawnのvoid/chan分岐
          〈detachedの値では変わらないこと込み〉・selectの全アーム+defaultのunion
          〈全void/混在込み〉)。`codegen.rs`に15件追加(chan生成2形態・recv・send・
          非chanへのsend/recvの明確なエラー・spawn自由関数・detach・spawnのメソッド
          呼び出し・存在しないメソッドの明確なエラー・select〈default有無・非chanアームの
          明確なエラー〉・明示的wait・明示的waitの中だけにspawnがあっても関数丸ごとの
          暗黙wait枠が付くこと〈TS版と同じ「フラットなフラグ」挙動〉・`?`と`spawn`の
          組み合わせ〈try/catch/finallyの順序込み〉・spawn_usedが関数ごとにリセットされ
          漏れないこと)。162→201件(後述のcode review指摘の修正で204件)、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **実行確認**: 新規`examples/concurrency.mesh`(chan/spawn/detach/wait/select/
          recv/sendを一通り使用。受信/select束縛値には算術をせずprint/文字列補間のみ
          ——real TSチェッカーが`T|closed`への算術を`is`絞り込み無しに許さないため)を
          Rust版で実行し`bun run mesh run`(TS版)とbyte-for-byte一致を確認。既存の
          `examples/channels.mesh`(`is`未使用)も今回からフルに動くことを確認。既存の
          `examples/channel_spec.mesh`(`is closed`使用)は引き続き
          `codegen: 'is' is not yet supported`で明確に失敗することを確認(クラッシュ
          しない、対応不要)。`hello`/`fizzbuzz`/`struct_methods`/`error_propagation`/
          `collections`/`maps`も回帰無しを再確認。
        - **milestone 5のスコープ外(意図的)**: `match`/`is`式・判別可能union・
          `error type`(union形式)・`json struct`・`filter`/`map`/`reduce`・`defer`・
          パッケージ修飾。次のmilestone(モジュール)以降で順に対応する
        - **PR #20の5エージェントcode reviewで1件のバグを発見・PR内で修正済み
          (2026-07-23、3エージェントが独立に別々の切り口・再現コードで発見——
          過去PRコメントレビュー・git履歴レビュー・コードコメント準拠レビューの
          いずれもが同じ根本原因に到達したため、Haikuスコアリングを待たず確定的な
          バグとして即修正)**: `checker.rs`の`Expr::Select`アームが、束縛名を
          スコープに宣言せず無条件に`infer_expr(ctx, &a.body)`へ渡していたため、
          (1) `v := <-ch => v`のような典型的なイディオム(bodyが束縛名をそのまま
          返す)では、その参照が常にANYになり(未宣言のIdentはANYへフォールバックする
          既存挙動)、`union_of`がANYを含むunionを丸ごとANYへ潰すため、select式
          全体の型が正しい`T | closed`ではなくANYになってしまい、後続コードでの
          安全ガード(milestone 4のUnion添字ガード・このPR自身の非chan-recvガード
          等、いずれも「ANYは確実に危険だと分からないので通す」設計)を静かに
          すり抜けてしまう、(2) 束縛名が外側スコープの型が違う変数をshadowして
          いる場合、bodyの中の参照が外側の(誤った)型に解決されてしまう
          (`v := 42; ... select { v := <-ch => v }`で`v`が誤ってintと推論され、
          実際は文字列の値に対して`__iarith`を選んでしまい紛らわしい実行時
          パニックになる)。`CheckerCtx`に`#[derive(Clone)]`を追加し、アームごとに
          使い捨てのスクラッチctx(clone)を作って束縛名をchanのelem型(`| closed`)
          として正しく宣言してからそのスクラッチ上でbodyを推論する形で修正
          (codegen側の`gen_select`は元々`&mut self.ctx`で正しく束縛していたため
          修正不要——チェッカー側だけの穴だった)。回帰テスト3件追加
          (`infer_exprのselectはアーム束縛名を正しくelem_or_closedとして推論する`・
          `...shadowしても外側の型を漏らさない`・
          `selectの結果を使った添字_recvにも正しい型で安全ガードが効く`)、
          201→204件、`cargo clippy`クリーンを確認済み。
        - 以下4件は独立検証で80点未満(既存の「診断を出さないリゾルバ」設計の
          限界の再確認、または既に受け入れ済みのPR #19の限界の新しい現れ方であり、
          このPR自体が新しい誤ったロジックを持ち込んだわけではないため)修正せず
          既知の限界として明記するに留める(2026-07-23判断):
          - **(70点)** range-forのアリティガード(milestone 4で追加、Array/Map/int
            の3形態にのみ反応)は`Type::Chan`にも反応しない——`for i, v := range ch`
            は`ch.entries is not a function`でクラッシュし、`for i := range ch`は
            数値とchanオブジェクトの比較が常にfalseになり0回で終わる(エラーにも
            クラッシュにもならず静かに空振りする)。PR #19で既に文書化済みの
            「range-forのアリティガードがstring/bool/struct等をカバーしない」
            限界に`Chan`が新たに加わった形——Goに慣れたユーザーが直感的に書きそうな
            `for v := range ch`が該当するため今回の方が踏まれやすい可能性はあるが、
            既存の限界と同じ根本原因・同じ受け入れ基準
          - **(25点)** `<-ch`/selectの結果(`T | closed`というUnion型)への算術が
            `check_arith_op`の`is_numeric`チェック(Union非対応)を通らず、静かに
            `__idiv`等を経由しない生の`/`等になる(例: `x := <-ch; y := x / 2`で
            `x`が実際は7でも`3.5`になり、Meshの切り捨て除算にならない)。この
            `is_numeric`のUnion非対応自体はmilestone 1由来の既存の穴(mapの
            `V | none`読みでも同じことが起きる)で、このPRの新しいロジックでは
            ない——ただしmapには`or`という逃げ道があるのに対し、`is`絞り込みが
            未実装のchannel/selectには逃げ道が無く、より踏まれやすい経路になった
          - **(0点)** `gen_index_assign`/`gen_index_incdec`(milestone 4のUnion
            ガード込み)は`Type::Chan`への添字にも反応しない——`ch[0] = 5`が
            クラッシュもエラーも無く、chanオブジェクトへの無意味なアドホックな
            プロパティ書き込みとして静かに空振りする。PR #19で既に文書化済みの
            「`gen_lvalue`のMemberは許可リスト方式だが`gen_index_assign`/
            `gen_index_incdec`は拒否リスト方式であり非対称」という限界に`Chan`が
            加わっただけで、新しいロジックではない
          - **(75点)** `spawn`/`detach`の呼び出し先が組み込み関数(例:
            `spawn print("hi")`)だと、`gen_call`が持つ組み込み関数の特別扱いが
            `gen_spawn`には無いため、存在しない`print`という素の識別子を参照する
            JS(`__spawn(print, ["hi"])`)を生成し実行時に`ReferenceError`で
            クラッシュする。TS版の`codegen.ts`自体にも同じ穴があり(Rust版は
            忠実に移植した形)、新しいロジックではないが、milestone 5以前は
            spawn/detach自体が無条件Errだったためこのバグは到達不可能だった
            ——このPRで初めて到達可能になった。80点未満で僅差だが、既存の
            「pre-existing issue」判定基準に沿って今回は未修正
  - [x] **checker+codegen milestone 6(モジュール)** ✅ 2026-07-23実装(kanayamaと確認済みの
        順序——struct/メソッド → error/json → 配列/map → 並行処理 → モジュール——の
        最後、6番目)。**複数ファイルコンパイル・`import`・パッケージ修飾参照**
        (`mathutil.Point`・`mathutil.add(...)`等)を実装。これまでの5マイルストーンは
        既存の単一ファイル前提の構造に「足していく」形だったが、今回は初めて構造そのもの
        (`main.rs`の単一ファイル前提・`CheckerCtx`の単一名前空間)を拡張する必要がある、
        この移植で最大の構造変更。TS版の該当実装(`src/cli.ts`のロード処理・
        `src/compiler.ts`・`src/checker/modules.ts`・`src/codegen.ts`の命名規則)を
        1本のExploreエージェントで調査し設計した(Planエージェント呼び出しは今回ユーザーの
        判断で見送り、既存調査結果をもとに直接設計・実装)。
        - **新規`rust/src/modules.rs`**: TS版`cli.ts`の`loadModules`/`loadDependencies`の
          移植。`load_modules(entry_file)`——mainパッケージ=エントリファイル1本のみ
          (同じディレクトリの他の.meshファイルは含めない。TS版の設計を直接確認して
          明確化した点)、各`import "x"`は`root/x/`ディレクトリの全.meshファイルを
          1パッケージとして読み込む。ファイルI/O層の処理なので診断ではなく明確なErr
          (存在しないディレクトリ・空パッケージ・ネストしたパス)。単体テスト5件。
        - **`checker.rs`**: `CheckerCtx`に`PackageSymbols`(types/fns/consts)のレジストリ・
          現在処理中パッケージ名(`pkg`)・importエイリアス集合(`import_aliases`)を追加。
          `begin_package(pkg, aliases)`でパッケージ切り替え時にfn_decls/struct_typesだけ
          リセット(method_table/registryは全パッケージ共有、リセットしない)。新設
          `qualify_struct_name(pkg, name)`(mainは無修飾、それ以外は`pkg.name`——TS版
          `types-resolve.ts`と同じ)を`resolve_struct_decls`でstructの内部識別名に適用
          (struct_typesのキー自体は素の名前のまま——パッケージ内部からは無修飾で引ける)。
          **パッケージ間でのstruct循環は構造的に起こり得ない**(パッケージレベルの
          import循環が依存順ソートの時点でErrになるため)ので、milestone 2の固定点反復は
          パッケージ内だけで従来通り動かせばよい。`resolve_type_node`/`infer_expr`の
          `TypeNode::Name`/`Expr::StructLit`に`pkg: Some(alias)`分岐を追加(レジストリから
          引く)。`infer_call`にパッケージ修飾の自由関数呼び出し判定を追加(ローカル変数
          によるshadowが優先——TS版`tryPackageMember`と同じ優先順位)。単体テスト4件追加。
        - **`codegen.rs`**: `generate(program, file)`(既存API)を1パッケージ("main")だけの
          `ModuleUnit`リストを作って新設`generate_modules(&[ModuleUnit])`を呼ぶ薄い
          ラッパーに変更——既存220件近いテストが無変更で通ることで無回帰を保証。
          `generate_all`を`generate_all_modules`(パッケージごとにファイルをまとめ、import
          依存グラフを依存順〈importされる側が先〉にトポロジカルソート——循環は明確な
          `Err`)+`generate_package`(1パッケージぶんの処理: struct解決→fn/メソッド
          シグネチャ登録〈同一パッケージの全ファイルぶん、前方参照対応〉→exportedシンボルを
          レジストリへ確定登録→本体生成〈ファイルごとに`self.file`を切り替え、パニック
          位置情報が生成元ファイルを正しく指すようにする〉)に分割。新設`fn_js_name(pkg, name)`
          (mainは無修飾、それ以外は`{pkg}${name}`——TS版`fnJsName`と同じ)をトップレベル
          自由関数・メソッド以外のfn/constの命名に使用(既存`method_js_name`の
          `.replace('.', "$")`は元々パッケージ未対応時から先取り実装されていたコードで、
          今回で初めて「効く」ようになった)。`gen_call`にパッケージ修飾の自由関数呼び出し
          判定を追加(未export/存在しない関数は明確なErr)。新設`resolve_free_fn_value`
          (自パッケージの既知のトップレベル関数ならpkg接頭辞付きの名前、それ以外はgen_exprへ
          素通し)を`gen_call`/`gen_spawn`の自由関数フォールバックで共有。`Expr::StructLit`の
          `pkg: Some(_)`分岐をレジストリ参照に置き換え(未export/存在しないstructは明確な
          Err)。単体テスト7件追加(パッケージ修飾呼び出し・同一パッケージ複数ファイル・
          struct literal/型注釈/メソッド・未export関数の明確なErr・ローカル変数shadow・
          未登録パッケージの明確なErr・import循環の明確なErr)。
        - **意図的なスコープ縮小**(`modules_demo.mesh`+`mathutil/*.mesh`を動かすのに
          不要なもの): 未exportシンボルの「見えない」という*挙動*はレジストリに載らない
          ことで自然に得られるが、専用の「not exported」診断文言は出さない(ANY/未解決
          フォールバックかcodegenの汎用Errになるだけ)。パッケージ名の妥当性検査・stdlib名
          衝突検査・エイリアス衝突検査・「型を関数として呼んだ」等の誤用診断は診断専用の
          ため対象外。パッケージ修飾された「呼び出しを伴わない」値参照(裸の
          `mathutil.SomeConst`や関数値としての参照)・パッケージ修飾レシーバ(拡張メソッド
          的な書き方)は今回の検証対象のいずれも使わないため対象外のまま(既存の
          `Expr::Member`/`receiver_struct_name`の即Errにフォールバックする)。exportedな
          constのレジストリ登録も同じ理由で対象外(`PackageSymbols.consts`は常に空——
          将来pkg修飾constの読み出しに対応する際に埋める)。`mesh test`相当や
          `_test.mesh`除外もRust版にはまだ`run`/`build`以外のCLIサブコマンドが無いため対象外。
        - **PR #21の5エージェントcode reviewで4件のバグを発見・PR内で修正済み
          (2026-07-23、いずれも独立に実際のビルド+実行で再現確認済み)**:
          (1) `spawn`/`detach`でパッケージ修飾された自由関数(`spawn mathutil.add(...)`)を
          呼ぶと、`gen_call`は解決できるのに`gen_spawn`が使う`resolve_free_fn_value`には
          同じ分岐が無く、素の関数値を得られず「package/member access is not yet
          supported」という紛らわしいエラーになっていた——`resolve_free_fn_value`に
          `gen_call`と同じパッケージ修飾判定を追加して修正。
          (2) pkg修飾された型注釈(`otherpkg.Point`)の循環検出(`collect_referenced_names`)が
          素の名前だけを見ていたため、同一パッケージ内にたまたま同じ素の名前のstructが
          あると無関係な相互参照だと誤認し、実際には循環していないのに
          「self-referential/cyclic struct」という誤ったエラーになっていた——
          `TypeNode::Name`の`pkg: Some(_)`分岐(他パッケージへの参照)を循環検出の
          収集対象から除外して修正。
          (3) 2つのパッケージ(または同一パッケージの2ファイル)が同じ名前のトップレベル
          constを宣言すると、トップレベル関数/メソッドと違いconstのJS名にはpkg接頭辞を
          付けていないため、生成JSの同じフラットスコープに同名の`const`宣言が2つ現れ、
          実行時クラッシュではなく**JS自体が構文エラーでパースできず起動不能になる**
          (delete()クラッシュ等より悪い壊れ方)——新規`declared_consts`集合で全パッケージに
          わたるトップレベルconst名の重複を検出し、明確なErrにして修正(constへの
          pkg接頭辞付けそのものは、参照側の配線まで必要になり今回のスコープを超えるため
          見送り、検出だけで対処)。
          (4) 「パッケージ間でのstruct循環は構造的に起こり得ない」という設計上の前提が、
          実際には`resolve_type_node`/`infer_expr`のpkg修飾分岐がそのエイリアスが本当に
          importされているか(`is_package_alias`)を確認せずレジストリを直接引いていた
          ため成り立っていなかった——2つのパッケージが互いの型を(import文を宣言せずに)
          参照し合うと、依存グラフ(import文だけを見て構築される)がその循環を検出
          できず、どちらが先に処理されるか(HashSetの反復順に依存)によって結果が
          非決定的に変わってしまう。`infer_call`の自由関数呼び出し判定は既に
          `is_package_alias`を確認していたので、型注釈・struct literal(checker.rs・
          codegen.rs双方)にも同じ確認を追加して修正——これによりパッケージ間の型参照は
          必ずimport文を経由するようになり、前提の正しさが回復した。
          回帰テスト6件追加、既存の軽微なコメントの誤り2件(`begin_package`が単一
          パッケージでも実際には毎回呼ばれる点/`fn_js_name`がconstには適用されない点)も
          修正。220→226件、全件パス、`cargo clippy`クリーンを確認済み。
        - テスト: `modules.rs`5件+`checker.rs`4件+`codegen.rs`7件を追加(milestone本体)、
          上記code review対応で6件追加。201→226件、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **実行確認**: 新設のマルチファイルエントリポイント(`main.rs`→`modules::load_modules`
          →`codegen::generate_modules`)経由で既存の`examples/modules_demo.mesh`+
          `examples/mathutil/{ops,point}.mesh`をコンパイル・実行し、`bun run mesh run`
          (TS版)の出力とbyte-for-byte一致を確認(`mathutil.add`/`mathutil.quadruple`
          呼び出し・`mathutil.Point{x,y}`構築・`mathutil.Point`型注釈・
          `p.magnitudeSq()`メソッド呼び出し・`mathutil.origin()`呼び出しを一通り確認)。
          生成JSのパニック位置情報が各関数の実際のソースファイル
          (`examples/mathutil/ops.mesh:5:11`等)を正しく指すことも確認。既存の全example
          (`hello`/`fizzbuzz`/`struct_methods`/`error_propagation`/`collections`/`maps`/
          `concurrency`/`channels`/`channel_spec`)も新しいマルチファイルエントリポイント
          経由で回帰無しを再確認。
  - [x] **checker+codegen milestone 7(match/is式・判別可能union)** ✅ 2026-07-23実装
        (確認済みの6マイルストーン——struct/メソッド・error/json・配列/map・並行処理・
        モジュール——が全て完了した後、kanayamaと相談し次の対象として選んだ)。
        パーサー・型システムは既に完全実装済み(`Expr::Is`/`Expr::Match`/`MatchArm`/
        `MatchPattern`/`Type::Union{members, discriminant_tag}`)。
        - **最重要の設計上の発見**(TS版`src/codegen.ts`を2本のExploreエージェントで
          深掘りして確認): narrowing(絞り込み)は**checkerのスコープだけの概念**で、
          生成JSには一切影響しない——`match`のアーム本体は`__m`という合成パラメータを
          一切参照せず、元のMesh変数名をそのまま(クロージャ経由で)参照する生JSになる
          (JSは動的型付けなので、narrowされていようがいまいが同じコードになる)。
          つまりRust版でも、narrowingはcodegen側の型依存判断(`__iarith`等)を
          正しくするためだけに必要で、生成JSの「形」自体は変えない
          (milestone 5のselect/orElseの束縛パターンと同じ設計の再利用)。
        - **スコープ**(実際の検証対象example——`discriminated_union`/`status`/`errors`/
          `users`/`channel_spec`——に基づく): narrowing対象は裸の識別子のみ
          (`n.next.value`のような多段フィールドパスは対象外)。`is`の条件式は単純な
          `x is T`形のみ(`&&`/`||`/`!`との組み合わせは対象外)。判別可能unionの
          discriminant_tagの実計算は実装しない——TS版のcodegen自体、`is`/`match`の
          実行時テストをASTの`TypeNode`から直接構築するため、コード生成には不要。
          **自己参照する判別可能union(`examples/tree.mesh`)は対象外**——milestone 2の
          自己参照structと全く同じ理由(無名structの構造的比較では自己参照を安全に
          表現できない)。
        - `checker.rs`: `CheckerCtx`に`union_types`テーブルを追加。
          `resolve_struct_decls`を`resolve_type_decls`へ汎化——struct宣言・union型alias
          宣言(`type X = A | B`)を同じ依存グラフの中で扱い(一方が他方を参照しうるため)、
          サイクル検出(`find_type_decl_cycle`、旧`find_struct_cycle`)もunion declの
          メンバーを見るよう拡張。union declは固定点反復の中で既存の`resolve_type_node`の
          `TypeNode::Union`分岐をそのまま再利用して`ctx.declare_union`に登録
          (discriminant_tagは計算せず常に`None`)。`resolve_type_node`/`infer_expr`の
          struct literal分岐は`lookup_struct`失敗時に`lookup_union`も試すよう拡張
          (union alias名でのstruct literal構築に対応、discriminant一致による厳密な
          member disambiguationはしない——union全体を近似型として返す)。新設
          `pattern_matches_member`(TS版`structPatternMatches`の移植——リテラル値一致・
          裸型名〈`"error"`はis_error_typeタグ付きstructも拾う特別扱い〉・struct形
          パターン〈リテラル値フィールドは厳密照合、非リテラルは名前だけの緩い判定〉)・
          `narrow_for_match_patterns`(アームの複数パターンで絞り込み、ワイルドカードなら
          絞り込まない)・`narrow_for_is`(is/if-is用、then/elseそれぞれの絞り込み型)・
          `match_is_exhaustive`(TS版の`match-not-exhaustive`診断と同じロジックだが、
          診断は出さずcodegenが最後のアームを無条件elseとして信用してよいかを内部判断
          するためだけに使う)。`infer_expr`に`Expr::Is`(常にBOOL)・`Expr::Match`
          (milestone 5のselectと同じロジック——subjectが裸Identなら使い捨てスクラッチctx
          で同名を絞り込んだ型で再宣言してからアームbodyを推論、全アームの型をvoid-only→
          VOID/混在→ANY/それ以外→union_of)を追加。
        - `codegen.rs`: 新設`gen_type_test`(TS版`genTypeTest`の移植——ASTの`TypeNode`から
          直接構造的な実行時テストを組み立てる、discriminant_tagは一切参照しない)。
          `Expr::Is`は裸Identならそのままテスト、非Identは二重評価を避けるIIFEで包む
          (TS版と同じ)。`Expr::Match`はTS版と同じ三項演算子の連鎖
          (`(await (async (__m) => test1 ? body1 : ... : lastBody)(subject))`)——
          **exhaustiveなら**TS版と全く同じ形(最後のアームは無条件)でbyte-for-byte
          一致、**exhaustiveでない場合だけ**(Rust版だけの安全ガード、milestone 2〜6と
          同じ考え方)最後のアームにも明確なテストを付け、どれにも一致しない場合は
          `__Panic`(既存のランタイム例外クラス、`at`位置情報込み)で明確に落とす。
          `Stmt::If`は`if x is T {...}`という単純形なら`narrow_for_is`で絞り込んだ型を
          then/else節それぞれに一時宣言し、**else節が無くthen節が必ず終端する
          (return/break/continue)場合は絞り込んだ「残り」の型を現在のスコープへ
          再宣言し、後続の同ブロック内の文が絞り込みを引き継ぐ**(`examples/
          channel_spec.mesh`の`if v is closed { break } total = total + v`で`v`が
          正しく`int`とnarrowされ`__iarith`を使うために必須——実際に検証して確認)。
        - テスト: `checker.rs`に9件(union型alias登録・struct/union相互参照・自己参照
          union循環検出・`pattern_matches_member`各パターン種別・`narrow_for_match_patterns`・
          `match_is_exhaustive`・`is`常にBOOL・`match`のnarrowing・`match`全void/混在)、
          `codegen.rs`に10件(`is`裸Ident/非IdentのIIFE/struct形パターン・union alias
          struct literal構築・matchのexhaustive時のTS版と同じ形・非exhaustive時の
          安全ガード・matchのnarrowingでの`__iarith`選択・if-isのthen/else/fallthrough
          narrowing・自己参照union循環の明確なErr)を追加。226→244件、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **実行確認**: `examples/discriminated_union.mesh`・`examples/status.mesh`・
          `examples/errors.mesh`・`examples/users.mesh`・`examples/channel_spec.mesh`
          (`is closed`が今回で初めてフルに動く)を実行し`bun run mesh run`(TS版)の
          出力とbyte-for-byte一致を確認。**`examples/maps.mesh`も今回`is`実装の
          副産物として初めてフルに動くことを確認**(milestone 4時点では`is none`未対応で
          明確な「未対応」エラーだったが、今回の実装で解消——想定外の副次的成果)。
          `examples/tree.mesh`(自己参照union)は`checker: self-referential/cyclic type
          definitions are not yet supported`で明確に失敗することを確認(クラッシュ
          しない、対応不要——milestone 2の自己参照structと同じ扱い)。既存の全example
          (`hello`/`fizzbuzz`/`struct_methods`/`error_propagation`/`collections`/
          `concurrency`/`channels`/`modules_demo`+`mathutil/*`)も回帰無しを再確認。
        - **milestone 7のスコープ外(意図的)**: 多段フィールドパスのnarrowing
          (`n.next.value`)・複合条件(`&&`/`||`/`!`)でのnarrowing・discriminant_tagの
          実計算・struct literal構築時のdiscriminant厳密disambiguation・自己参照する
          判別可能union(`tree.mesh`)・`error type`(union形式の名前付きエラー型)・
          `json struct`・`filter`/`map`/`reduce`・`defer`。
        - **PR #22コードレビュー(5エージェント)で発見・実行確認して即修正した4件**
          (score付けを待たず、milestone 4/5/6と同じ「実行して再現確認済みのバグは
          即修正」の前例に従った):
          1. `match_is_exhaustive`が(0アーム含め)Union以外のsubjectを常に「網羅的」
             扱いしていたため、非union型へのmatchや空の`match x {}`が安全ガード無しで
             最後のアームを無条件に信用していた(0アームでは空のアーム本体がそのまま
             出力され構文的に壊れたJSになる、非union structへのmatchでは想定外の値が
             静かに誤ったアームへ落ちる)。0アームは常にfalse、確実に非union(ANY以外)な
             subjectも常にfalseに修正——安全ガード(各アームへの実テスト+`__Panic`
             フォールバック)が正しく効くようにした。
          2. `pattern_matches_member`のstruct形パターンで非リテラルフィールドが
             「同名フィールドがあれば型を問わず一致」という緩い判定だったため、
             同名だが型の異なるフィールドで判別するunion(例:
             `{tag: int, ...} | {tag: string, ...}`)で`match_is_exhaustive`が
             カバレッジを過大評価し、値が実行時に誤ったアームへ静かに振り分けられて
             いた(3エージェント中3件が独立に指摘・再現)。TS版`structPatternMatches`と
             同じく非リテラルフィールドも`type_equals`で型まで厳密照合するよう修正。
          3. `pattern_matches_member`の裸型名`"error"`パターンが`is_error_type`付き
             named error structも拾っていたが、codegen側の`gen_type_test`は
             `(ref instanceof Error)`のみをテストし、named error struct
             (タグ付きの普通のobject)には決してマッチしない——checker/codegenの
             認識が食い違い、`r is error`/`match r { error => ... }`が静かに誤った
             narrowing/exhaustiveness判断をしていた。TS版はそもそもこの組み合わせ
             (プリミティブerrorを含まないunionでnamed error structを`error`
             パターンで捕捉しようとする)を`impossible-pattern`診断でコンパイル
             エラーにする——このリゾルバは診断を出さないため、`"error"`パターンは
             プリミティブERROR型のみに一致するよう修正しcodegenの実際のランタイム
             テストと一致させた(named error structを`error`パターンで捕捉する構文は
             元々TS版でも不可能パターンとして拒否される組み合わせであり、今回の
             修正でその「捕捉できない」という事実がchecker/codegenで一貫するように
             なった、という位置付け)。
          4. `gen_if`のnarrowing伝播が「else節が無くthen節が必ず終端する」場合しか
             実装されておらず、「else節がありthen節が必ず終端する」場合(if/elseの
             後に到達できるのはelse経由だけ)が漏れていた——`if x is error { return 0 }
             else { ... }`の後で`x`が絞り込み前の`int | error`のまま扱われ、
             `__idiv`ではなく素の`/`が生成され浮動小数点除算になっていた。
             else節ブロックの生成後にも同じ再宣言ロジックを追加。
          いずれも`cargo test`(246件、+2件)/`cargo clippy --all-targets -- -D
          warnings`クリーンを再確認し、既存の全example(milestone 7で検証したもの含む)
          がbyte-for-byte一致のまま回帰していないことも再確認済み。
        - **PR #22コードレビューで発見したが、既存の別スコープ決定の帰結として
          todo.mdに記録するだけに留めた3件**(スコアリング前だが、いずれも
          「既に受け入れ済みの大きなスコープカットの帰結」または「現状どの
          example/testからも到達不能」に該当し、milestone 7自体のバグではないため
          即修正はしなかった):
          - struct literalのfield名は宣言済みstruct/unionの形と照合されない
            (`GetUserResponse{knd: "ok", ...}`のようなtypoがあっても検出されない、
            PR #17以来繰り返し指摘されてきた既知のギャップ)。今回の`match`の
            exhaustive最適化(最後のアームを無条件elseにする)と組み合わさると、
            以前は`undefined`表示や分かりやすいpanicで済んでいたのが、typoしたstruct
            literalが「もっともらしいが誤った」別のアームへ静かに振り分けられる
            (`undefined`より発見しづらい)形に変わる。struct literalのfield検証は
            それ自体大きな別スコープなので、今回は「帰結が悪化した」という事実を
            記録するに留める。
            **→ milestone 12(struct literalのフィールド検証、2026-07-24)で解消**
          - **(訂正、2026-07-24)** 当時「union alias名でのstruct literal構築
            (`Resp{kind:"ok", value:5}`)は構築直後のフィールドアクセスがANY型になる
            (Rust版だけの穴)」と記録していたが、milestone 12でTS版
            `src/checker/expressions.ts`の`structLit`ケースを読み直した結果、これは
            Rust側の欠陥ではなく**TS版自身の意図的な設計**だったと判明した——TS版も
            struct literal式全体の型は(disambiguationで絞り込んだ具体的なメンバー
            型ではなく)常にunion自身を返す(「match/isで絞り込むまでは常にunionとして
            扱う」という一貫したポリシー)。したがって構築直後のフィールドアクセスが
            ANYになる(算術演算が`__iarith`ではなく素の演算子になる)のはTS版と
            完全に一致した挙動であり、修正対象ではない(PR #20のunion/ANY型算術演算
            ギャップとは無関係の、単なる正しい仕様)。
          - 裸のstruct名パターン(`match shape { Circle => ..., Square => ... }`、
            構文的にはパース可能)は、checker側の`pattern_matches_member`は名前で
            厳密に判別できるが、codegen側の`gen_type_test`(TS版`genTypeTest`を
            忠実に移植)は「オブジェクトかどうか」という汎用テストしかできず、
            形が似た複数のstructを判別できない(TS版自体の既知の制約、discriminant_tag
            を計算しない設計上避けられない)。現状どのexample/testもこの構文を
            使っておらず到達不能。
  - [x] **checker+codegen milestone 8(error type・union形式の名前付きエラー型)**
        ✅ 2026-07-23実装。milestone 7完了後、kanayamaから「error typeとjson structは
        一緒にできるか」と聞かれ、TS版`src/checker/types-resolve.ts`
        (`tagErrorMembers`、約20行)と`src/json-decode.ts`(313行、AST合成による
        `decode<X>`自動生成)を直接読んで調査した結果、分量・複雑さが1桁近く違い
        技術的にも無関係と分かったため、分けて進めることに決定——error type
        (union形式)を先に小さいmilestoneとして実装し、json structは
        `mesh/json`スタブ実装込みの独立した大きめmilestoneとして後日別途進める。
        - **TS版の挙動**: `resolveAlias`がstruct/union/その他いずれの解決でも
          `ctx.errorTypeNames.has(name)`なら`tagErrorMembers`を呼ぶ。対象memberは
          「このunionのために今まさに作られた無名`{...}`」だけ——既存の名前付き型への
          参照(`error type Aliased = Existing`)は診断`error-type-aliases-existing`で
          常に拒否される(共有される型オブジェクトを介した意図しない波及を防ぐため)。
          非struct形のmember(`error type Bad = int`)も診断`error-type-must-be-struct`
          で拒否。union全体は他の型と組み合わせて使われる際`union_of`で平坦化される
          ため、DbErrorの各メンバーは外側のunionに直接展開され、既存の
          `isFailureType`/`or`の束縛型計算がDbError固有の特別扱い無しにそのまま働く
          (`tests/checker.test.ts:1037-1117`で確認済み、`or e => match e {...}`との
          組み合わせも同テストで検証済み)。
        - `checker.rs`: `resolve_type_decls`の`TypeNode::Union`分岐で`decl.is_error`が
          真の場合、新設`tag_error_union(name, source_members, resolved)`を呼ぶ。
          元のソースmembers(TypeNodeレベル)がすべて`TypeNode::StructType`
          (union内の無名`{...}`由来)であることを検証し(1つでも既存型への参照や
          非struct形があればTS版の2つの診断をまとめた明確なErrを返す——診断を
          出さない設計なのでmilestone 2〜7と同じ「TS本体は診断で弾くが、この
          リゾルバではErrにする」パターン)、通れば解決済みUnionの各Struct memberに
          `is_error_type: true`を立て直す(単体の`error struct`と違い、union形は
          「全メンバーが等しく失敗を表す」——discriminated unionの各バリアントが
          それぞれ別の種類の失敗、という設計)。`is_failure_type`/
          `has_structured_failure`/`or_binding_type`(milestone 3実装)・
          `pattern_matches_member`/`match_is_exhaustive`(milestone 7実装)は
          いずれも変更不要——既存のis_error_typeベースのロジックがそのまま効く。
        - `codegen.rs`: `generate_package`の型宣言ゲートから`is_error && Union`を
          拒否する専用チェックを削除(milestone 7時点では明示的に対象外としていた
          組み合わせ)。`Expr::StructLit`の`__errTag`ラップ判定(`None`分岐)を、
          `lookup_struct`が失敗したら`lookup_union`も試しいずれかのmemberが
          `is_error_type`付きなら`true`にするよう拡張(union形は全メンバーが
          等しくタグ付けされる設計なので"any"判定で十分)。`?`/`or`のランタイム
          呼び出し生成(`__prop`/`__or`)は値ベースの`__errTag`検出なので変更不要。
        - テスト: `checker.rs`に5件(union形式の全メンバーis_error_type解決・
          非struct形memberのErr・既存型参照memberのErr・
          has_structured_failure/or_binding_typeの統合確認)、`codegen.rs`に1件
          (union形error typeのstruct literalが`__errTag`でラップされること)を追加。
          既存の「error type union形式は未対応」テストは削除(now supported)。
          244→250件(前milestoneのPR #22コードレビュー修正分246件から+4)、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **実行確認**: `tests/checker.test.ts:1037-1117`の実際のシナリオを元に新規
          `examples/db_error.mesh`(`error type DbError = {kind:"notFound",...} |
          {kind:"timeout",...}`+`find`/`useIt`+bareな`?`での伝播+
          `or e => match e {...}`での分岐)を作成しRust版で実行、`bun run mesh run`
          (TS版)の出力とbyte-for-byte一致を確認。宣言時検証(非struct形member・
          既存型参照member)がそれぞれ明確なErrになることも確認。既存の全example
          (`hello`/`fizzbuzz`/`struct_methods`/`error_propagation`/`errors`/
          `discriminated_union`/`status`/`users`/`channel_spec`/`collections`/
          `maps`/`concurrency`/`channels`/`modules_demo`+`mathutil/*`)も回帰無しを
          再確認。`tree.mesh`(自己参照union)は引き続き明確なErrのまま。
        - **milestone 8のスコープ外(意図的)**: `error type`の単体宣言以外の形
          (`type X = SomeExistingStruct`をerror typeとしてタグ付けする形——TS版でも
          常に拒否される組み合わせなので実質的な機能損失は無い)。`json struct`
          (独立した大きめmilestoneとして後日別途進める)。`filter`/`map`/`reduce`・
          `defer`。
        - **PR #23コードレビュー(5エージェント)で発見・実行確認して即修正した2件**
          (score付けを待たず、milestone 4〜7と同じ「実行して再現確認済みのバグは
          即修正」の前例に従った):
          1. `Expr::StructLit`の`__errTag`ラップ判定(このPR自身の新規コード)が
             "any"(union内のいずれかのmemberがis_error_type付き)判定だったため、
             通常structとerror type unionを混ぜたさらに外側のunion
             (`type Result = Success | DbError`)で、DbError由来のタグ付き
             メンバーが1つ混じっているだけで`Result{value:...}`という普通の
             成功値までerrTagで包んでしまい、`?`/`or`が成功値を誤って失敗として
             握り潰していた。"all"判定(`tag_error_union`は対象unionが自身
             error typeとして宣言された場合に限り全メンバーへ揃ってタグ付けする
             ため、"all"にしてもerror type宣言そのものの構築は変わらず正しく
             ラップされる)に修正、新設`type_is_error_instance`ヘルパへ集約。
          2. **milestone 6/7由来の既存ギャップ**(このPR自身の新規ロジックの
             バグではないが、milestone 8がこのギャップを実害のある形で顕在化
             させた——2エージェント独立指摘): パッケージのexportedシンボル登録
             (`generate_package`)が`TypeNode::StructType`宣言だけを見ており、
             union型alias宣言(milestone 7)が一切登録されていなかった。
             milestone 7時点では判別可能unionは通常「各named memberが自分自身の
             名前で構築される」ため実害が無かったが、milestone 8はunion
             メンバーを無名{...}限定にする設計のため、union自身の名前が構築・
             型解決の唯一の手がかりになり実害が顕在化した。2つの失敗モードを
             実行確認: (a) exportされた`error type`をパッケージ越しに構築すると
             `no exported struct`という明確なErrになる(TS版は成功する)、
             (b) `fn f() int | pkg.DbError`のようなpkg修飾された戻り値型注釈は
             is_error_typeの付かない殻structへ静かにフォールバックし、
             milestone 3の`has_structured_failure`安全ガード(文脈付き`?`が
             構造化errorに対して使われるのを弾く役目)を素通りしてしまい、
             文脈付き`?`が構造化errorに対してコンパイルを通り、実行時に
             `__propCtx`が構造化errorを「成功扱い」して静かに壊れた挙動になって
             いた(`[object Object]1`のような出力)。修正: `generate_package`の
             export登録を`TypeNode::Union`宣言(`lookup_union`経由)にも拡張
             (`lookup_package_type`は型の種類を区別しないので、pkg修飾側の
             参照箇所〈`resolve_type_node`/`infer_expr`〉は無変更で自動的に
             正しく動くようになった)。codegen側のpkg修飾`__errTag`判定も
             新設`type_is_error_instance`ヘルパで統一。
          いずれも`cargo test`(251→253、+2)/`cargo clippy --all-targets --
          -D warnings`クリーンを再確認し、既存の全exampleがbyte-for-byte一致の
          まま回帰していないことも再確認済み。2件目の実行確認は
          `export error type DbError = {...}|{...}`をパッケージ越しに構築・
          文脈付き`?`で伝播する2ケースで、修正前後の差分をTS版と直接比較して
          確定させた。
  - [x] **checker+codegen milestone 9(json struct)** ✅ 2026-07-23実装。milestone 8
        完了後、kanayamaから「error typeとjson structは一緒にできるか」と聞かれ、
        TS版`stdlib.ts`(組み込みパッケージ定義)と`json-decode.ts`(313行、AST合成)を
        直接読んで調査した結果、分量・複雑さが1桁近く違い技術的にも無関係と判明した
        ため分けて実装することに決定(kanayamaに提示し選択された)。
        - **これまでの8マイルストーンと質的に違う点**: checker/codegenの「解析」だけ
          でなく、`json struct X {...}`宣言から`decode<X>(v: json.Value) X | error`
          という新しいMesh関数を**構文レベルのAST(Stmt/Expr)として合成し
          program.fnsへ追加する新しいパイプライン段階**が必要(TS版`compiler.ts`が
          `parse`直後・`check`前に`synthesizeJsonDecoders(program)`を挟むのと同じ)。
          合成後のASTは以降の通常のcheck/codegenをそのまま流用する。
        - **`mesh/json`は`.mesh`ソースを持たない組み込みパッケージ**: TS版`stdlib.ts`の
          `BUILTIN_PACKAGES`に相当する新設`json_stdlib_symbols()`(`codegen.rs`)で
          `PackageSymbols`(Value型+parse/stringify/field/optField/asString/asInt/
          asFloat/asBool/asArrayの9関数)を直接手組みし、`generate_all_modules`の
          パッケージループ開始前に`register_package("json", ...)`で1回だけ登録する
          (`topo_sort_packages`はpackagesに無い名前への参照を無視するため無害だと
          確認済み)。**ランタイムJS側は既に完全に揃っていた**——`prelude()`には
          H-2実装時にruntime.ts全体が移植済みで`json$parse`等が既に実装済みだった
          ため、codegen自体への変更は「registryへの1回の登録」だけで済み、pkg修飾
          関数呼び出し・struct literal構築の既存経路(milestone 6)は無改造で流用できた。
        - **`json.Value`の自己参照は不透明な殻structとして扱う(意図的なスコープ縮小)**:
          TS版の`json.Value`は真に自己参照する判別可能union(`arr`/`obj`メンバーが
          Value自身を配列/map越しに参照、共有可変オブジェクトとして手組み)——
          milestone 2以来一貫して「対応不可、明確なErr」としてきた自己参照型の壁
          そのもの(`examples/tree.mesh`と同じ)。TS版のテスト(`tests/e2e.test.ts`)を
          確認し、`json struct`が自動生成する`decode<X>`は`json.field`/`json.asString`
          等の**不透明なヘルパー呼び出しだけ**を経由しValueを直接構造分解しないこと
          (construct側の`json.Value{kind:"null"}`という定番イディオムも、struct
          literalのフィールドが宣言済み型と照合されない既存のギャップのおかげで
          殻structでも問題なく動くこと)を検証してから、`Value`を
          `Type::Struct{name:"json.Value", fields:vec![], is_error_type:false}`と
          いう不透明な殻として登録する設計を選んだ。**一方、TS版のテストには
          `if v is {kind:"obj"} { print(len(v.entries)) }`という、生のjson.Valueを
          直接is/matchで構造分解する手書きデコーダの例も存在する**——これは
          `tree.mesh`と同じ理由でmilestone 9のスコープ外とした(`json struct`の
          自動生成機能自体には一切影響しないことを確認済み)。
        - **既存バグの発見・修正(このmilestoneの副産物)**: TS版`checker/types-resolve.ts`
          を確認したところ、`isJson`フラグはstruct自体の型解決(構築・フィールド
          アクセス)には一切影響せず、decode<X>自動生成の対象を決めるだけだと判明。
          Rust側は milestone 3時点で(json struct未実装のプレースホルダとして)
          `resolve_type_decls`の対象から`is_json`宣言を丸ごと除外していたため、
          json structを手書きの`X{...}`で構築してもフィールドが空の殻へ静かに
          フォールバックし、フィールドアクセスが`ANY`型になる(`__iarith`が
          選ばれない等)潜在バグになっていた。TS版と同じく`is_json`を除外条件から
          削除(`checker.rs`)、`codegen.rs`の「json struct宣言は未対応」という
          明確なErrゲートも撤去(json structは今や普通のstruct宣言として解決できる)。
        - `rust/src/modules.rs`: `load_dependencies`に`path == "mesh/json"`の早期
          continueを追加(ファイルシステムを見ない)——`'/'`を含むパスのため、
          この判定が無いと「ネストしたパッケージパスは非対応」で誤って弾かれていた。
        - 新規`rust/src/json_decode.rs`(TS版`json-decode.ts`の忠実な移植):
          `pub fn synthesize_json_decoders(program: &mut Program) -> Result<(), String>`。
          `program.types`から`is_json`なdeclを集め(無ければ即Ok)、
          `import "mesh/json"`が無ければErr、各json structについて
          `decode<Name>(v: json.Value) Name | error`のFnDeclをMesh構文レベルの
          AST(Stmt/Expr)として合成しprogram.fnsへ追加する。対応するフィールド型
          (TS版と同じv1スコープ): int/float/string/bool(`json.asXxx`経由)・
          同一ファイル内の他のjson struct(`decode<Other>`経由、再帰)・それらの
          配列(range-forで1件ずつデコードして`push`)・それらの`T | none`
          (`json.optField`+`!(x is none)`ガード)。未対応の型は明確なErr
          (TS版`json-struct-unsupported-field`相当)。TS版の`MultiCompileError`
          (複数エラー蓄積)は、このリゾルバの「Result<_, String>単一エラー」設計と
          馴染まないため移植せず、最初に見つかったエラーだけを返す簡略化(診断を
          出さない設計なので実害は無い)。TS版の小さなAST合成ヘルパー関数群
          (`identExpr`/`stringLit`/`callExpr`/`jsonCall`等)も1:1で移植。
        - `rust/src/main.rs`: `parse(&m.source)`成功直後、`ModuleUnit`を作る前に
          `json_decode::synthesize_json_decoders(&mut program)?`を呼ぶ(TS版
          `compiler.ts`の`parse`直後・`check`前という挿入位置と同じ)。
        - テスト: `json_decode.rs`に8件(flat構造体・ネストしたjson struct・配列
          フィールド・optionalフィールド・`import`無しはErr・未対応フィールド型
          〈map/配列要素map〉はErr・宣言0件は即Ok)、`modules.rs`に1件(`mesh/json`が
          ファイルシステムを見ずに解決できる)、`codegen.rs`に3件(json structが
          普通のstructとして解決・構築できフィールドアクセスが正しい型で推論される
          〈`__iarith`が選ばれる〉こと・`mesh/json`組み込みパッケージの関数呼び出しと
          Value構築が解決できること・`mesh/json`とユーザーパッケージのimportが
          共存できること)を追加。253→264件、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **実行確認**: `tests/e2e.test.ts:2738-2859`(H-2: json structの節)の実際の
          シナリオ(フラットなstruct decode・欠落/型違い/非objectフィールドの
          エラー・ネストしたjson struct+配列+optionalフィールドの組み合わせ・
          配列-of-ネストstruct+optional配列・整数フィールドへの小数値の拒否)を元に
          新規`examples/json_decode.mesh`を、cross-packageのexportシナリオを元に
          新規`examples/json_models_demo.mesh`+`examples/jsonmodels/models.mesh`を
          作成し、Rust版で実行して`bun run mesh run`(TS版)の出力とbyte-for-byte
          一致を確認。既存の全example(`hello`/`fizzbuzz`/`struct_methods`/
          `error_propagation`/`errors`/`discriminated_union`/`status`/`users`/
          `channel_spec`/`collections`/`maps`/`concurrency`/`channels`/
          `modules_demo`/`db_error`+`mathutil/*`)も回帰無しを再確認。`tree.mesh`
          (自己参照union)は引き続き明確なErrのまま。
        - **副産物: TS版自体のフォーマッタのバグを発見・修正**。CI(TS版の全example
          往復整形テスト)で`examples/json_decode.mesh`が失敗——`src/formatter.ts`の
          `printTypeDecl`が`decl.isError`(→`"error "`)だけを見て`decl.isJson`を
          見ておらず、`json struct X {...}`を再整形すると普通の`struct X {...}`に
          化けてしまい(`json`キーワードが消える)、再整形後のソースを実行すると
          `decodeX`が合成されず壊れた挙動になっていた。`jsonKw`を追加して修正、
          回帰テスト1件追加(`tests/formatter.test.ts`)。あわせて、cross-package
          example(`json_models_demo.mesh`)の往復整形テストが依存パッケージ
          (`examples/jsonmodels`)を一時ディレクトリへ複製していなかった問題も
          (`modules_demo.mesh`/`mathutil`の既存の特別扱いと同じパターンで)修正。
          TS版テストスイート484→485件、全件パス。
        - **PR #24コードレビュー(5エージェント)で発見・実行確認して即修正した2件**
          (score付けを待たず、milestone 4〜8と同じ「実行して再現確認済みのバグは
          即修正」の前例に従った):
          1. **`json.Value`を完全に不透明な殻(フィールド0個)にする当初の設計は
             スコープの見積もりが甘かった**: `json struct`が自動生成する
             `decode<X>`自体は不透明なヘルパー呼び出ししか使わないため実害が
             無いことは検証済みだったが、`tests/e2e.test.ts:1146-1160`(json struct
             機能より前からある既存のmesh/json手書きdestructure、`if v is
             {kind:"obj"} { len(v.entries) }`)という現存するTS版の検証済み機能を
             見落としていた——`mesh/json`のimport自体がこのPRで初めて可能になる
             (以前は`'/'`を含むパスとして「ネストしたパッケージパスは非対応」で
             丸ごと弾かれていた)ため、このPRが初めてこの既存機能の到達可能性を
             左右する立場になっていた。修正: `json.Value`をkind判別フィールド+
             実フィールドを持つ本物のunion(6メンバー)にし、**真に自己参照する
             再帰位置(`arr.items`/`obj.entries`の要素/値型)だけ**を名前だけの
             不透明な殻に留める設計に変更(自己参照のRust版の壁——milestone 2・
             `tree.mesh`と同じ——を回避しつつ、1階層の絞り込み+フィールド
             アクセスは正しい型〈`len`が`.size`を選ぶ等〉で動くようにした)。
          2. **合成する`decode<Name>`という名前が手書きの同名関数と衝突すると
             検出されず、無効なJS(二重宣言のSyntaxError)を静かに出力していた**。
             このシンセシス自体が初めて「利用者から見えない隠れた予約名」を
             生む処理だったため、`json struct User {...}`の横に(気づかず、
             または偶然)`fn decodeUser(...)`を書くという自然な間違いが、
             一般的な「トップレベル関数名の重複」検出(このリゾルバにはまだ
             存在しない、別スコープの既知のギャップ)よりずっと踏みやすい形に
             なっていた。修正: `synthesize_json_decoders`が合成前に既存の
             関数名との衝突を確認し、明確なErrにする。
          いずれも回帰テスト追加(`json_decode.rs`+1、`codegen.rs`+1)、
          `examples/json_decode.mesh`に生destructureの実例セクションを追加、
          264→266件、`cargo clippy --all-targets -- -D warnings`クリーン、
          既存の全exampleがbyte-for-byte一致のまま回帰無しを再確認済み。
        - **milestone 9のスコープ外(意図的、上記の修正を踏まえて再整理)**:
          `json.Value`の2階層以上の入れ子destructure(絞り込み後さらに絞り込む
          ケース——checker側の型推論の精度だけが劣化し`is`/`match`自体はASTから
          直接テストを組み立てるため実行時には動く、が算術演算等の型依存判断が
          `__iarith`等を選べずANY相当になる)。**自己参照する`json struct`宣言
          自体**(木構造・連結リストのような、フィールドが〈配列/optional越しに
          間接的にでも〉自分自身を参照する形、例: `json struct TreeNode { value:
          int, children: TreeNode[] }`)——`json_decode.rs`自身のフィールド対応
          表はこれを弾く専用ロジックを持たず、`resolve_type_decls`の汎用サイクル
          検出(`tree.mesh`と同じ、milestone 2以来の壁)に委ねる形になり、
          「json struct宣言以外の一般的な自己参照structと全く同じ理由・同じ
          扱い」という前提で許容した(2エージェント独立指摘、いずれも
          「回帰ではなく既存の壁がこの機能でも顕在化しただけ」と判定)——
          専用の分かりやすいエラーメッセージにする余地はあるが、汎用サイクル
          検出を複製する必要がありコストに見合わないため見送り。`mesh/io`・
          `mesh/http`(無関係、対象外のまま)。cross-file/cross-packageの
          json struct同士のネスト参照(TS版自体がv1スコープ外)。
          `filter`/`map`/`reduce`・`defer`。
  - [x] **checker+codegen milestone 10(filter/map/reduce)** ✅ 2026-07-23実装。
        milestone 9完了後、kanayamaと相談し次の対象として選んだ(todo.mdの既知の
        未対応機能のうち`defer`は今回のスコープ外——`filter`/`map`/`reduce`とは
        別の独立した機能のため次のmilestone以降に回す)。TS版`src/codegen.ts`
        (`fnExpr`ケース・`genFnBody`のpropStack/spawnStack設計)・
        `src/checker/builtins.ts`(filter/map/reduceの型推論)を直接読んで調査。
        - **これまでの9マイルストーンと違う点**: `filter`/`map`/`reduce`自体の
          codegen(`(await __filter(...))`等——ランタイムヘルパーはH-2実装時に
          runtime.ts全体を移植済みで既に揃っていた)は3行で済んだが、その引数と
          なる**無名関数式(`Expr::FnExpr`)のcodegenがそれまで一切実装されて
          おらず**(milestone 4以来ずっと明確なErrスタブ)、これを実装するのが
          今回の本題だった。
        - **`prop_used`/`spawn_used`を単一フラグからスタックへ変更**: 無名関数は
          他の関数の中にネストしうる(`g := fn() int { return f()? }`のように
          無名関数自身が`?`/`spawn`を使うことがあり、外側の関数の使用状況とは
          独立に判定する必要がある)ため、TS版の`propStack`/`spawnStack`と同じ
          設計に合わせ`Vec<bool>`のスタックにし、`gen_fn_body`という呼び出し
          単位でpush/pop。これに伴い、FnDecl/Expr::FnExpr共通の「本体をいったん
          別バッファに生成し、`?`/`spawn`の使用有無で事後にtry/catch/finally包みを
          決める」ロジックを`gen_fn_decl`から`gen_fn_body`という共有ヘルパーへ
          切り出した(`defer`は`Stmt::DeferStmt`が常にErrを返すため今回も対象外の
          ままでよい)。
        - `checker.rs`: `infer_expr`に`Expr::FnExpr`(TS版`fnType(ctx, params,
          ret)`相当——`Type::Fn{params, ret}`を返すだけで本体は検査しない、
          TS版`checkFn`に相当する処理は診断を出さない設計上不要)を追加(これで
          `infer_expr`の`match`が全`Expr`variantを尽くす形になり、既存の
          `_ => ANY`最終フォールバックが到達不能になったため削除)。
          `infer_builtin_call`に`filter`(対象配列と同じ型をそのまま返す)・
          `map`(コールバックの戻り値型の配列)・`reduce`(コールバックの第1引数
          〈累積値〉の型、コールバックの型が確実に分からなければ初期値の型、
          それも無ければANY)を追加(TS版`builtins.ts`と同じロジック、診断部分は
          省略)。
        - `codegen.rs`: `Expr::FnExpr`のcodegen(TS版と同じトリック——
          indentを0にリセットして隔離バッファへ生成し、出来上がった各行の
          先頭に呼び出し元の実際のindentを後付けで足す。パラメータは
          `push_scope`/`declare`してから本体を生成し`pop_scope`)。
          `filter`/`map`/`reduce`の組み込み呼び出しcodegen(`(await
          __filter(arr, pred))`等)。組み込みの引数個数安全ガード
          (PR #19由来の「パニックさせない」設計)に`filter`/`map`→2・
          `reduce`→3を追加(無いとアリティ不足でpanicする)。
        - テスト: `checker.rs`に2件(`Expr::FnExpr`のfn型推論・
          filter/map/reduceの戻り値型推論)、`codegen.rs`に4件(無名関数式の
          即時評価アロー関数生成・filter/map/reduceのランタイムヘルパー呼び出し・
          無名関数内の`?`使用が外側の関数を汚さない・無名関数内の`spawn`使用が
          外側へ漏れない)を追加。266→272件、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **実行確認**: `tests/e2e.test.ts`の実際のシナリオ(名前付き関数を値として
          渡す・インラインクロージャで外側のmut変数を捕捉・mapで要素の型を変える・
          reduceの2用途〈数値合計・文字列畳み込み〉・filter→map→reduceの
          パイプライン合成)を元に新規`examples/filter_map_reduce.mesh`を作成し、
          Rust版で実行して`bun run mesh run`(TS版)の出力とbyte-for-byte一致を
          確認。既存の全example(`hello`/`fizzbuzz`/`struct_methods`/
          `error_propagation`/`errors`/`discriminated_union`/`status`/`users`/
          `channel_spec`/`collections`/`maps`/`concurrency`/`channels`/
          `modules_demo`/`db_error`/`json_decode`/`json_models_demo`+
          `mathutil/*`)も回帰無しを再確認。`tree.mesh`の明確なErrも回帰無し。
        - **PR #25コードレビュー(5エージェント)で発見・実行確認して即修正した4件**
          (score付けを待たず、milestone 4〜9と同じ「実行して再現確認済みのバグは
          即修正」の前例に従った、うち3件が独立に複数エージェントから指摘された):
          1. **裸の識別子がトップレベル関数名を「値として」参照する場合に型が
             ANYへ落ちる**(`evens := filter(nums, isEven)`のように名前付き関数を
             コールバックとして渡す形——milestone 10で初めて到達可能になった経路)。
             `CheckerCtx`はローカル変数用の`scopes`とトップレベル関数用の
             `fn_decls`を別テーブルで持つが、`infer_expr`の裸`Expr::Ident`分岐は
             `scopes`(`ctx.lookup`)しか見ておらず、`fn_decls`へのフォールバックが
             無かった。`ctx.lookup(name).cloned().or_else(|| ctx.lookup_fn(name)
             .cloned())`に修正(ローカル変数がトップレベル関数名を覆う場合は
             ローカル優先、TS版の実際のスコープ規則と同じ)。
          2. **ローカル変数に代入した無名関数を直接呼び出す場合も同じ理由で戻り値型が
             ANYへ落ちる**(`inc := fn(x: int) int {...}; inc(5) * 2`——milestone 10で
             `Expr::FnExpr`をローカル変数へ代入できるようになって初めて到達可能に
             なった経路、1件目とは別の呼び出し式側の分岐)。`infer_call`のIdent呼び出し
             分岐が`fn_decls`(`ctx.lookup_fn`)しか見ておらず、ローカルスコープに
             保持されたfn値を見ていなかった。同じ優先順位(ローカル→fn_decls→組み込み)
             で修正。1件目と2件目はいずれも「`__iarith`等の型依存の安全ガードが
             選ばれず、整数オーバーフローを静かに素通りしてしまう」という実害を
             実際にオーバーフローする入力で再現確認済み(3エージェント中2件が
             独立に指摘)。
          3. **入れ子になった`Expr::FnExpr`の再インデントが崩れる**(内側の無名関数の
             出力〈複数行の文字列〉を外側が`body_lines`の1要素として扱うため、
             2行目以降に外側のpadが付かない)。TS版`codegen.ts`のfnExprケースと
             同じく、一旦全体を1つの文字列に結合してから改めて改行で分割し全ての
             物理行へpadを付け直す形に修正(実行結果自体はJSの空白が意味を持たない
             ため正しかったが、この移植が一貫して検証基準にしてきた「TS版とbyte-
             for-byte一致」から外れていた、2エージェント独立指摘)。
          4. **`toInt`が常に失敗する既存のバグを発見**(milestone 10自体とは無関係だが
             このPRのレビュー中に発覚・実行確認): `codegen.rs`の`prelude()`が
             `runtime.ts`のテンプレートリテラルの中身を単純な部分文字列として
             取り出すだけで、JSのテンプレートリテラル自身が持つエスケープ解決
             (`\\`→`\`)を一切評価していなかった。`runtime.ts`の`__toInt`は
             (外側のテンプレートリテラルが1段エスケープを解決する前提で)正規表現を
             `\\d+`と2つ重ねて書いているため、TS版は実際に評価されて`\d+`になるが、
             単純抽出しかしないRust版はソースの`\\d+`をそのまま出力し、
             `\\d`(バックスラッシュ文字自体を要求する、実質何にもマッチしない
             正規表現)になって`toInt`がどんな入力に対しても常に失敗していた
             (`toInt`を使う既存exampleが無かったため今まで発覚しなかった)。
             `prelude()`の戻り値を借用から所有(`String`)に変え、抽出後に`\\`→`\`
             を1回置換して修正(runtime.ts全体を確認し、他のエスケープ〈`` \` ``や
             `\$`〉はこのテンプレートリテラル内に存在しないことも確認済み)。
          いずれも回帰テスト追加(`checker.rs`+2、`codegen.rs`+2)、
          `examples/filter_map_reduce.mesh`に3件の回帰確認セクションを追加、
          272→276件、`cargo clippy --all-targets -- -D warnings`クリーン、
          既存の全exampleがbyte-for-byte一致のまま回帰無しを再確認済み。
        - **milestone 10のスコープ外(意図的)**: `defer`(独立した別機能、次の
          milestone以降で対応)。ジェネリック関数(`fn myFilter<T>(...)`のような
          利用者定義の総称関数、既にF-1後半として別スコープで管理されている
          既知の未対応機能——`filter`/`map`/`reduce`は組み込み関数であり
          ジェネリクスとは無関係)。
  - [x] **checker+codegen milestone 11(defer)** ✅ 2026-07-23実装。milestone 10
        完了後、todo.mdに残っていた既知の未対応機能が`defer`のみになり、kanayamaと
        相談し次の対象として選んだ。TS版`src/codegen.ts`の`genDeferStmt`/
        `genFnBody`・`src/checker/statements.ts`の`deferStmt`検査を直接読んで調査。
        パーサーは既に完全実装済み(`Stmt::DeferStmt{call: Expr, pos: Pos}`——
        「パーサーは任意の式を許して渡すだけ、callがCallであることの検証は
        checker/codegenの仕事」という設計がast.rsのコメントに明記済み)。checker.rsは
        文(Stmt)を検査しない設計なので、**今回の実装は完全にcodegen.rsだけで完結**。
        - **TS版`genDeferStmt`の「影武者(かげむしゃ)call式」トリック**: `defer f(a,
          b)`/`defer recv.method(a)`は、Goと同じく引数(メソッドならレシーバも)を
          defer文を書いた時点の値で固定する(後で書き換えても古い値を見る)。TS版は
          これを、引数・レシーバをその場で一時変数(`__d0`,`__d1`,...、コンパイル
          全体で1つのカウンタ、関数ごとにリセットしない)へ評価してから、一時変数
          への参照に差し替えた「影武者」のcall式を組み立て、**既存のgen_call
          (パッケージ修飾/メソッド/組み込み/素の関数呼び出しの分岐ロジック)に
          そのまま渡す**ことで実装している——呼び出し形の判定を重複させずに済む
          巧妙なトリックで、Rust版もそのまま踏襲した。**TS版との違い**: TS版は
          checker/codegenが完全に分かれた2パスなので、影武者のIdentノードへ
          `resolvedType`を直接埋め込むだけで済むが、このリゾルバはchecker/codegenが
          融合していて`gen_call`が`self.ctx`を都度引くため、一時変数の型を
          `self.ctx.declare`でも宣言しておかないと`gen_call`自身のメソッド判定等が
          ANY扱いになってしまう(実装中に発見・対応)。
        - `gen_fn_body`(milestone 10で切り出し済みのFnDecl/Expr::FnExpr共通の
          ボディ生成+ラップ判定ヘルパー)に`defer_used: Vec<bool>`
          (prop_used/spawn_usedと同じスタック)を追加。ラップ判定を`used_prop ||
          used_spawn || used_defer`に、`used_defer`なら`try`の前に`const __defers =
          [];`を追加、`finally`節の中身をTS版と同じ順序(**spawnした子タスクを
          先に待ってから**、自分のdeferを最後の後片付けとして走らせる——LIFO=登録の
          逆順)に拡張。無名関数式の中で`defer`を使っても、milestone 10のprop/spawn
          スタック分離のおかげで外側の関数を汚さず独立して働く(無名関数自身を
          抜けるときに実行される、TS版のテストでも確認済みの挙動)。
        - 新設`gen_defer_stmt`(TS版`genDeferStmt`の移植)。`Stmt::DeferStmt`が
          常にErrを返していた既存の分岐をこの呼び出しに差し替え。`call`が
          `Expr::Call`でなければ(パーサーは任意の式を許すため)明確なErrにする
          (TS版の`defer-requires-call`診断に相当)。
        - テスト: `codegen.rs`に7件(複数deferのLIFO順・引数のdefer時点固定・
          メソッド呼び出しdefer〈レシーバも一時変数へ捕捉〉・組み込み/パッケージ
          修飾関数のdefer・deferでない式は明確なErr・無名関数式の中のdeferが
          独立して働く・spawn併用時のfinally内の順序)を追加。276→283件、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。
        - **副産物: TS版自体のフォーマッタのバグを発見・修正**(milestone 9の
          `json struct`キーワード欠落と同じ構図)。CI相当の確認(TS版の全example
          往復整形テスト)で`examples/defer.mesh`が失敗——`src/formatter.ts`の
          `printStmt`のswitchに`deferStmt`のcase自体が無く(default節も無いため
          型検査でも検出できなかった)、`defer`文を再整形すると文そのものが丸ごと
          消えてしまっていた。caseを追加して修正、回帰テスト1件追加
          (`tests/formatter.test.ts`)。あわせて、cross-package example
          (`defer_pkg_demo.mesh`)の往復整形テストが依存パッケージ
          (`examples/loggerpkg`)を一時ディレクトリへ複製していなかった問題も
          (既存の特別扱いと同じパターンで)修正。TS版テストスイート486→490件、
          全件パス。
        - **実行確認**: `tests/e2e.test.ts`の"defer文"節(複数defer LIFO・引数固定・
          メソッドdefer〈レシーバコピー〉・組み込み/パッケージ修飾defer・早期
          return・panicでの巻き戻り・ループ内での積み上がり・spawn併用・無名関数
          内のdefer・複数パッケージでのdefer)を元に新規`examples/defer.mesh`
          (通常シナリオ一式)・`examples/defer_panic.mesh`(panicで巻き戻る
          ケース——終了コード・標準出力・標準エラーの3点をTS版と突き合わせ)・
          `examples/defer_pkg_demo.mesh`+`examples/loggerpkg/logger.mesh`
          (cross-package defer)を作成し、Rust版で実行してTS版の出力とbyte-for-byte
          一致を確認。既存の全example(`hello`/`fizzbuzz`/`struct_methods`/
          `error_propagation`/`errors`/`discriminated_union`/`status`/`users`/
          `channel_spec`/`collections`/`maps`/`concurrency`/`channels`/
          `modules_demo`/`db_error`/`json_decode`/`json_models_demo`/
          `filter_map_reduce`+`mathutil/*`)も回帰無しを再確認。`tree.mesh`の
          明確なErrも回帰無し。
        - **PR #26コードレビュー(5エージェント)で発見・実行確認して即修正した1件、
          記録に留めた2件**:
          1. **影武者call式に、defer文自体の`pos`ではなく元のcall式自身の`pos`を
             使っていなかった**(バグスキャンエージェント発見・実行確認済み)。
             TS版`genDeferStmt`は`{ ...call, callee: calleeForInvoke, args:
             argTemps }`で元のcall式のposをそのまま引き継ぐが、Rust版は
             `Stmt::DeferStmt`自体の`pos`を影武者call式に渡してしまっていたため、
             deferした組み込み呼び出しの型エラー・パニック位置情報が(defer文の
             位置ではなく)呼び出し式自身の位置を指すべきところ、defer文自体の
             位置を指してしまっていた(例: `defer round(x)`でxがオーバーフロー
             する場合、TS版は`round(`の位置でpanicするが、修正前のRust版は
             `defer`キーワードの位置でpanicしていた)。値やフロー自体は壊れて
             いなかったが、位置情報の不一致は確認済みの回帰なので修正(元のcall式の
             `pos`を捕捉し影武者call式へ引き継ぐ)。回帰テスト1件追加。
          2. **`ast.rs`/`parser.rs`の古いコメントを修正**(コードコメント準拠
             エージェント発見): 「checkerがcall.kind===Callであることを検証する」
             という記述が、実際には(checker.rsは文を検査しない設計のため)
             このPRで実装した`codegen.rs`の`gen_defer_stmt`が検証している、という
             実態と食い違っていた(パーサー移植時からの古い記述で、このPRより
             前から不正確だったが、defer自体がこのPRで初めて実装されたことで
             食い違いが明確になった)。「codegen(gen_defer_stmt)」に修正。
          3. **記録に留めた既知の限界1件**(過去PRコメントエージェント発見・
             TS版でも再現確認済み): `defer recv.fieldFn(args)`(structのフィールドが
             保持する関数値経由の呼び出し、真のメソッド呼び出しではない)は、
             レシーバがdefer時点の値で固定されない(後の再代入後の値を見てしまう)。
             TS版`genDeferStmt`自身が「struct型かつ同名フィールドが無い」場合だけ
             メソッド呼び出しとみなしレシーバを捕捉する設計になっており、この
             ケース(フィールドがある=同名メソッドではない)はTS版でも同じ理由で
             捕捉されない——Rust版はTS版を忠実に移植しているだけで、Rust側だけの
             新しい退行ではないため修正はせず記録に留める。
          いずれも回帰テスト追加(`codegen.rs`+1)、284件、
          `cargo clippy --all-targets -- -D warnings`クリーン、既存の全example
          がbyte-for-byte一致のまま回帰無しを再確認済み。
        - **milestone 11のスコープ外**: 無し——`defer`はtodo.mdに残っていた最後の
          既知の未対応機能であり、これでTS版リファレンス実装の主要機能をRust版が
          ひととおり移植し終えた(細かな既知の限界・意図的なスコープ縮小は引き続き
          このtodo.mdに記録済みの通り残る: 自己参照型・`json.Value`の2階層以上の
          destructure・ジェネリック関数・`mesh/io`/`mesh/http`・
          cross-file/cross-packageのjson struct参照 等)。
  - [x] **checker+codegen milestone 12(struct literalのフィールド検証)**
        ✅ 2026-07-24実装。milestone 11(defer)完了後、kanayamaとこれまでの既知の限界を
        整理し、最も古く(PR #17以来)・影響範囲が広い(match/is・`?`/`or`・一般的な
        構築の正しさ全てに関わる)「struct literalのフィールドが宣言済みの形と一切
        照合されない」穴を本気で直すことに決定(`is_numeric`のUnion/ANY対応と2択で
        提示し、こちらを先に選択)。
        - **TS版の挙動**(`src/checker/expressions.ts`の`structLit`ケース、約140行、
          直接読んで調査): 単純な「フィールド名の照合」だけでなく、**判別可能unionの
          構築時disambiguation**(どのメンバーを構築しているかの特定)も含む、想定より
          大きい機能だった。(1)無名`{...}`メンバーが2個以上の判別可能unionは、宣言時に
          確定済みの`discriminantTag`の値だけを見てメンバーを特定する(フィールド集合は
          見ない)。(2)無名メンバー1個以下ならその1個。(3)それ以外(名前付きstruct同士の
          union)はフィールド名の集合で候補を絞り、複数残れば値のassignableでタイブレーク。
          特定した具体的なstruct型に対し重複/未知/型不一致/欠落フィールドを検証する。
          **式全体の推論型は絞り込んだ具体的なメンバーではなく常にunion自身**
          (match/isで絞り込むまでは常にunionとして扱うという一貫した設計)。
        - `checker.rs`: F-7判別可能unionのタグ計算が必要になったため(milestone 7時点では
          「codegenが参照しないため計算しない」という意図的な決定だったが、struct literal
          の正しいdisambiguationにはタグ名が要る)、新設`find_discriminant_tag`(TS版
          `findDiscriminantTag`の移植)を`resolve_type_decls`のUnion分岐から呼び、
          `Type::Union.discriminant_tag`に実際の値を持たせるようにした。無名メンバー
          2個以上なのに有効な共有タグが無ければ、TS版`discriminated-union-tag-required`
          相当の明確なErrにする(milestone 2以来の「TS本体は診断で弾くが、この
          リゾルバではErrにする」パターン)。新設`resolve_struct_lit_member`(3分岐の
          disambiguation)・`validate_struct_lit_fields`(重複/未知/型不一致/欠落の検証、
          型互換性は既存の`types::assignable`をそのまま再利用)。`infer_expr`の
          `Expr::StructLit`分岐自体は不変(「式全体はunion自体」という既存動作を維持)。
        - `codegen.rs`: `Expr::StructLit`のpkg無し分岐で上記2関数を呼び、Errなら伝播する。
          milestone 8の`type_is_error_instance`(pkg無し時の"all"判定ヒューリスティック)は
          disambiguationで特定した具体的なメンバー自身の`is_error_type`を直接見るように
          置き換え、より正確になった(pkg修飾側は現状の"all"ヒューリスティックを維持、
          スコープを広げすぎないための意図的な決定——**訂正(2026-07-24、git historyレビュー
          エージェント指摘)**: 当初「他パッケージの構造をここでは持たない制約のため」と
          説明していたが、これは不正確——`lookup_package_type`は解決済みの完全な`Type`
          〈fields/discriminant_tag/is_error_type込み〉を返すため、技術的にはpkg修飾側でも
          同じdisambiguation/検証ができる。単にスコープを広げすぎないための意図的な選択
          であり、技術的制約ではない。**結果、`mathutil.Point{x: 1, typo: 2}`のような
          pkg修飾struct literalのフィールドtypo/欠落/型不一致は今回も無検証のまま
          (`json.Value{kind: "bogus", extra: 1}`のような組み込みパッケージ経由の構築も
          同様、実行確認済み)——次にpkg修飾側を厳密化する際にまとめて対応する候補**)。
        - **検証中に発覚**: milestone 8の既存回帰テスト
          (`error_type_unionと通常structを混ぜたさらに外側のunionでは成功値をerrtagで
          包まない`)のmeshソースが`Result{value: 42}`(union自身の名前で名前付きメンバー
          Successを構築しようとしていた)だったが、実際にTS版へ通したところ
          `discriminated-union-tag-missing`で**TS版自身が拒否する**ことが判明した——
          `DbError`由来の無名メンバー2個がResultユニオン自身にもタグ("kind")を要求し、
          無名メンバー以外(名前付きのSuccess)はタグ経由のdisambiguationの対象外になる
          ため。有効な構築方法は具体的なstruct名(`Success{value: 42}`)を使うことだと
          TS版で実行確認し、テストのソースをそちらに修正(退行ではなく、milestone 12の
          検証がTS非互換だった既存テストを正しく検出した、という位置付け)。
        - 新規ユニットテスト`checker.rs`14件(`find_discriminant_tag`3件・
          `resolve_type_decls`のタグ必須化2件・`resolve_struct_lit_member`6件・
          `validate_struct_lit_fields`1件〈重複/未知/型不一致/欠落を1件にまとめて検証〉)+
          `codegen.rs`7件(typo/欠落/型不一致/重複フィールドが明確なErrになること・
          int↔floatの非対称assignable・判別可能unionの正しい構築とタグ不一致のErr・
          error type unionの新経路での`__errTag`付与)。284→304件、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。既存の全example(21本、
          自己参照型で対象外の`tree.mesh`を除く)を再実行しbyte-for-byte一致で回帰
          無しを確認。加えて`discriminated_union.mesh`のフィールド名を意図的に
          typoさせた変種を実際にRust版・TS版両方でコンパイルし、両者とも
          `unknown-field`相当のエラーで拒否することを確認(以前のRust版はここを
          静かに素通ししていた)。
        - **5エージェントのcode reviewで発見・即修正した2件**(いずれもこのPR自身の
          新規コードに対する指摘で、実行確認済み):
          1. `compute_discriminant_tag`のErrに位置情報`(line:col)`が一切付いていなかった
             ——同じPRの兄弟関数(`resolve_struct_lit_member`/`validate_struct_lit_fields`)
             は位置情報を付けているのに、この関数だけこのコードベース全体の慣習
             (ほぼ全エラーに`(line:col)`が付く)から外れていた。呼び出し元で既に
             手元にある`decl.pos`をそのまま渡すだけで解消(回帰テストに位置情報の
             アサーションを追加)。
          2. 組み込み`mesh/json`パッケージの`json.Value`(milestone 9、コード直組みの
             union、`.mesh`の型宣言を経由しないため`resolve_type_decls`のタグ計算を
             通らない)が`discriminant_tag: None`のまま放置されていた——古いコメントは
             「milestone 7以来の設計判断・codegenは一切参照しない」としていたが、今回
             `resolve_struct_lit_member`がpkg無し側で実際にこのフィールドを読むように
             なったため前提が崩れていた。現状はjson.Valueの構築が常にpkg修飾
             (`json.Value{...}`)経由でこの新しい検証をまだ通らないため実害は無いが、
             将来pkg修飾側も同じ検証経路に統一すると即座に「タグが無い」という誤った
             Errになる潜在的な地雷だった(TS版`src/stdlib.ts`は最初からこの手組みunionに
             `discriminantTag: "kind"`を手で設定済みで、Rust版のmilestone 9移植時に
             見落としていた)。6メンバー全てが共有する`"kind"`の値を直接
             `Some("kind".to_string())`として設定し解消。
          **記録に留めた1件**(過去PRコメントレビューエージェント指摘・現状どの
          example/testからも到達不能): milestone 8の`tag_error_union`にも
          `compute_discriminant_tag`と同じ「Errに位置情報が無い」という欠落がある
          (`error type Bad = { kind: "a" } | Existing`のようなケース)。今回の
          PRが新規に持ち込んだものではなくmilestone 8由来の既存の欠落であり、
          このPR自体は変更していないため、今回はスコープ外として記録に留める
          (次にこの関数へ触れる際にまとめて直す候補)。
        - **milestone 12のスコープ外(意図的)**: pkg修飾された(他パッケージの)struct
          literalの厳密なmember disambiguation(pkg無しの場合だけ厳密化、pkg修飾側は
          既存の"all"ヒューリスティックのまま——**git historyレビューエージェントが
          `mathutil.Point{x: 1, typo: 2}`〈未知フィールド+欠落フィールド〉や
          `json.Value{kind: "bogus", extra: 1}`〈組み込みパッケージ経由の不正なタグ値〉が
          いずれも無検証で素通りすることを実行確認済み——技術的な制約ではなく
          単なるスコープ判断、上記参照**)・`gen_lvalue`(代入先)のフィールド名検証・
          struct宣言時点の`__proto__`ガード(いずれもPR #17以来の既知の限界のうち、
          今回のスコープに含めなかった残り)。
  - [x] **checker+codegen milestone 13(算術演算子の妥当性検査・is_numericの
        Union/ANY問題)** ✅ 2026-07-24実装。milestone 12完了後、残る既知の限界の
        うち「`is_numeric`のUnion/ANY対応」に着手した。当初は「union型への算術が
        おかしくなる」という狭い問題だと見積もっていたが、TS版
        `src/checker/expressions.ts`の`checkArithOp`を読み、実際にTS版へ複数
        パターンを通して検証した結果、**Rust版には算術演算子(`+ - * / %`)の
        妥当性検査が一切無い**という、より根本的な穴だと判明した——両辺が
        「両方int/float」でも「両方stringで`+`」でもない組み合わせは、TS版なら
        `invalid-operation`で拒否するところを、Rust版は無条件にANY型・フラグ無しへ
        フォールバックし、生のJS演算子をそのまま出力してしまっていた。実機確認:
        `x := <-ch; y := x / 2`(`x`は`int | closed`、未絞り込み)はTS版が
        `invalid-operation`で拒否するが、Rust版は素通りしてJSの浮動小数点`/`を
        生成(本来Meshの切り捨て除算`__idiv`が必要)。`true - false`(bool同士の
        引き算)も同様にTS版は拒否・Rust版は`(a - b)`という無意味なJSを生成していた。
        「union型への算術」(map読み取り`V | none`・channel受信/select結果
        `T | closed`を`is`/`match`で絞り込む前に演算するケース、PR #19以来
        繰り返し記録されてきた限界)は、この一般的な穴の一種類にすぎなかった。
        - kanayamaと相談し、milestone 2以来一貫している「TS本体は診断、Rust版は
          明確なErr」パターンをそのまま適用することで合意。スコープは**算術演算子
          (`+ - * / %`)のみ**とし、比較演算子(`< <= > >=`)の`incomparable-types`
          検査・`&&`/`||`の`not-bool`検査・`==`/`!=`絡みの検査(`use-is-none`/
          `incomparable-types`)は別カテゴリの診断のため対象外(struct literal
          検証のときにpkg修飾側を切り離したのと同じ考え方)。
        - `checker.rs`: `check_arith_op`/`infer_binary`の戻り値を`BinaryInfo`から
          `Result<BinaryInfo, String>`へ変更。TS版`checkArithOp`と同じロジック
          (両方numeric・両方stringy+`+`・どちらかANYはそれぞれOk、それ以外は
          `invalid-operation`相当のErr)を移植。**重要な点**: 「どちらかANYなら
          常に許す」チェックがTS版と同じく2箇所ある(is_numeric分岐の中と外)——
          is_numeric(ANY)は`true`なので、`ANY op 非数値非ANY型`(例:
          `ANY + 構造体`)は最初のis_numeric分岐(両方numericの条件)を満たさず、
          2つ目のANY安全弁で初めて拾われる。1段しか無いとこのケースが誤ってErrに
          なってしまうところだった。`infer_expr`の`Expr::Binary`分岐自体は
          「checkerは診断を出さず常に何かを返す」設計を維持するため、Errは
          ANYへ飲み込む(milestone 12の`resolve_struct_lit_member`等を`infer_expr`
          からは呼ばず、codegen側からだけ呼んだのと同じ構図)。
        - `codegen.rs`: `Expr::Binary`のcodegenと`gen_compound_value`(`+=`等の
          複合代入)の2箇所で`infer_binary`の呼び出しに`?`を追加。**複合代入の
          Indexターゲット(`m[k] += v`等)は影響を受けない**——mapは
          `gen_index_assign`の別経路で複合代入自体を既に明確なErrにしており
          (TS版`compound-assign-on-map`診断と同じ理由、milestone 4以来)、配列は
          添字読みがelem型を直接返すため(union化しない、`checker.rs`の
          `Expr::Index`推論参照)そもそも対象外——今回の変更で影響を受けるのは
          ident/構造体フィールドへの複合代入のみ。
        - 新規ユニットテスト`checker.rs`4件(bool同士の算術・struct同士の算術・
          未絞り込みunion型への算術がいずれもErrになること、ANYが絡む場合は
          相手がstruct/unionでも常にOkのままなこと)+`codegen.rs`6件
          (chan受信/map読み取りの未絞り込み算術がErr・bool算術がErr・`is closed`/
          `or`で絞り込んだ後は今まで通り`__idiv`/`__iarith`を経由・ANYが絡む算術は
          相手の型に関わらず許可される)。既存の`infer_binary`直接呼び出し
          テスト6件は`.unwrap()`化。304→314件、全件パス。
          `cargo clippy --all-targets -- -D warnings`クリーン。既存の全example
          (22本、自己参照型で対象外の`tree.mesh`を除く)を再実行しbyte-for-byte
          一致で回帰無しを確認(milestone 12と同じ理由——既存の全exampleは既に
          TS版のこの`checkArithOp`検査を通過しているプログラムのため、理論上も
          回帰しないはず、実際に回帰無しだった)。実際に`<-ch`/map読み取り/
          `true - false`の3パターンをRust版・TS版両方でコンパイルし、両者とも
          同じ理由(`invalid-operation`)で拒否すること、エラーメッセージの
          位置情報まで完全一致することを確認済み。
        - **milestone 13のスコープ外(意図的)**: 比較演算子(`< <= > >=`)の
          `incomparable-types`検査・`&&`/`||`の`not-bool`検査・`==`/`!=`絡みの
          `use-is-none`/`incomparable-types`検査(いずれも別カテゴリの診断、
          必要なら次のmilestone候補)。TS版がコンパイル時に検出するリテラル`0`
          除算の`division-by-zero`診断(Rust版は既に実行時の`__idiv`/`__imod`が
          ゼロ除算をpanicで検出しており、milestone 1由来——正しさの観点では
          既に担保済みのため、コンパイル時の早期発見という利便性の差のみ)。
  - Rust学習を兼ねる(所有権とASTの付き合い方が最初の山)

## 言語機能(中期)

- [x] **struct のメソッド**(2026-07-18実装)— Goスタイルの `fn (t: Todo) render() string { ... }`。
      名前空間は自由関数と分離(`render(t)`は不可、`t.render()`のみ)しP1を維持。
      レシーバはstruct限定。テスト182件。関数型注釈は2026-07-19に実装済み(F-1前半)
- [ ] **標準ライブラリ** — 言語カード実験(2026-07-18)で必要性が浮上。第一弾・第二弾実装済み
      - [x] 配列/map操作: contains / indexOf(`int | none`)/ keys / values / sort(非破壊)— 2026-07-18
      - [x] 文字列操作: split / join / trim / upper / lower / toInt(`int | error`)— 2026-07-18
      - [x] 高階関数: filter / transform(map改名。型キーワードと衝突するため)/ reduce — 2026-07-18
      - [ ] 層分け設計(core共通 / 環境別: http・json・file・DOM)は requirements C-6 / Q3 と統合して検討
- [x] **E-1決着: 2段スコープの構造化並行**(2026-07-18実装)— spawn=関数所有で暗黙wait
      (リーク構文的に不可能)/ detach=プログラム所有のバックグラウンド。channelは維持。
      kanayamaの「deferのように後片付けを自動保証」発案が起点。テスト188件
- [x] **channel仕様の完成**(2026-07-18実装)— 容量指定(Go互換の本物のブロッキング送信、
      panic方式ではなくkanayama選択)/ close+`T | closed`(C-5決着、union路線をそのまま適用)/
      select式(matchの見た目を踏襲した独立構文、擬似ランダム公平選択)。テスト206件
- [x] **defer 文**(2026-07-21実装)。**討議で決定**: `defer f(x)`は関数呼び出しのみ許可
      (Go準拠。任意の式/ブロックは不可 — P1と、複数defer時のLIFO順を素直に保てることを優先)。
      panic発生時もdefer自身は必ず実行される(JSのtry/finallyへ愚直に載せているだけで、
      recover()相当は追加しない — Meshのpanicは「バグはバグとして落ちる」方針のまま)。
      引数(メソッドならレシーバも)はGoと同じくdefer文の実行時点で即時評価して固定する
      (後で`mut`変数を書き換えても古い値を見る)。実装は「レシーバ・引数を一時変数へ退避し、
      その一時変数だけを参照する“影武者”のcall式を組み立ててgenCallに渡す」方式 — 呼び出し形
      (素の関数/メソッド/パッケージ修飾/組み込み)ごとの分岐ロジックを複製せずに済んだ。
      `mesh card --for`は`defer`検出時のみ節を含める(未使用プログラムのカードを肥大させない)。
      テスト13件追加(checker 3件・e2e 10件: LIFO順・引数の即時評価・メソッドレシーバの固定・
      panic時の実行・早期returnでの実行・ループ内蓄積・spawn併用時の順序・クロージャ内のスコープ・
      複数パッケージでの呼び出し)
  - ~~副産物として見つかった既存のバグ: `spawn recv.method()`/`detach recv.method()`は検査を
        通るが実行時にクラッシュする(`f is not a function`)~~ ✅ **2026-07-21修正**。
        `__spawn(callee, args)`/`__detach(callee, args)`が`this.genExpr(callee)`の結果を
        素の関数値として扱っており、struct メソッドが`recv.method`という(存在しない)プロパティ
        アクセスとして誤ってコンパイルされていたのが原因(メソッドは実際には
        `__m_Struct_method(recv, ...)`という別関数)。defer文の実装(genDeferStmt)と同じ
        メソッド判定を再利用し、`spawn`/`detach`ともレシーバを引数列の先頭に回して
        `__m_Struct_method`を素の関数として渡す形に修正(`genSpawnOrDetach`)。
        素の関数・パッケージ修飾関数・引数ありメソッドの回帰なしを確認。テスト3件追加(e2e)

## ツール・品質(中期)

- [x] エラーメッセージにソース行の表示(2026-07-20実装)— `formatDiagnostics`(compiler.ts)が
      file→ソース文字列のMapを受け取り、非JSON出力(`mesh run`/`build`/`check`/`test`)の
      各診断の下にソース行+`^`を添える。`^`のみ(`~~~`の範囲下線は無し — `Pos`が開始位置しか
      持たないため、Fix.rangeのような範囲情報を全診断に持たせる大きな変更をしないと出せない)。
      桁位置はlexerがタブも1文字と数えるので、位置合わせの空白列もタブはタブのまま残して
      端末のタブ描画に委ねた。`mesh check --json`等の機械可読フォーマットは変更なし
- [x] **エラーからの復帰**(2026-07-21実装)。パニックモード復帰(Go/TS/Rust等と同じ標準手法) —
      トップレベル宣言・文それぞれの単位で、構文エラーを1件記録したら次の宣言/文の先頭らしき
      トークンまで読み飛ばして再開する。1件しか無ければ従来どおり素の`CompileError`を投げ
      (`instanceof`に依存する既存の呼び出し側・テストは無改造で動く)、2件以上集まったときだけ
      新設の`MultiCompileError`(`errors: CompileError[]`)を投げる。`compiler.ts`/`cli.ts`(mesh fmt)
      の両方で対応済み。復帰時は「エラー発生時点で開いたまま閉じていない`{`があれば、まずそれを
      全部閉じてから次の宣言/文を探す」という深さ計算をしないと、壊れた文の中の`}`を囲むブロック
      自身の終わりと誤認してカスケードする実バグを実装中に発見・修正。**既知の限界**: 閉じ括弧が
      本当に足りない(対応する`}`が無い)壊れ方は、この種のパニックモード復帰では原理的に
      カスケードする(TypeScript/Go等でも同様) — 独立した小さいtypo複数は正しく個別に報告される。
      安全弁として1ファイルあたり最大50件で打ち切り。テスト6件追加(parser 5件・e2e 1件)
- [x] フォーマッタ `mesh fmt`(2026-07-20実装)。**討議で決定**: インデントはタブ固定、
      改行の有無は幅による自動折り返みをせずユーザーの選択をそのまま尊重する(gofmt方式)、
      コメントは「同じ行→trailing / それ以外→直後のノードのleading」という位置ベースの
      単純な規則で再割り当て。素朴に「パース→AST印字」すると全コメントが消える
      (lexerが捨てていたため)と判明し、コメント保持を先に実装する2段階で進めた。
  - [x] **フェーズ1: コメント保持の土台** — lexerが行コメントを`{text, pos}[]`として
        別配列(`Program.comments`)へ退避(トークン列には乗せない=既存の文法規則・
        checker・codegenは無改造で影響ゼロ)
  - [x] **フェーズ2: 印字本体**(`src/formatter.ts`)— struct/array/map literal・
        関数呼び出し引数・union宣言はparserが`multiline`フラグを記録し(開き括弧と
        閉じ括弧の行番号を比較)、印字はそれを尊重するだけ。CLI `mesh fmt file.mesh [-w]`
        (引数無しはstdout、`-w`で書き戻し。gofmtと同じ既定)。
        **実装中に見つけて直した実バグ2件**:
        (1) 呼び出し引数を素朴に複数行化すると、最後の引数の後に自動挿入される`;`が
        カンマ必須の引数リスト文法と衝突して構文エラーになった → 呼び出し引数だけ
        末尾にも`,`を付けて解消(`,`の直後の改行にはセミコロンが自動挿入されないため)。
        (2) 文字列中のリテラル`\${...}`(補間させないためのエスケープ)を素朴に再構築すると
        `$`のエスケープが失われ、再パース時に本当に評価されてプログラムの意味が変わって
        しまっていた → `${`の直前には常に`\`を復元するよう修正。
        **既知の限界(v1)**: 空行の保持はしない(トップレベル宣言間は常に1行、文の間は
        常に0行という固定ルールに正規化。各ノードの終了位置を今のASTが持たないため)。
        複合式の途中(struct literalのフィールド間等)にあるコメントは、位置がずれて
        直後の文のleadingとして出る(消えはしない)。
        テスト: `tests/formatter.test.ts`(24件、examples全11本のべき等性・実行結果一致を含む)+
        `mesh fmt` CLI配線テスト3件。tsc/bun test(422件)全てグリーン
- [x] **VS Code拡張**(2026-07-21実装)。v1はシンタックスハイライトのみ(言語サーバー無し —
      診断・定義ジャンプ等は`mesh check`/`mesh fmt` CLIに委ねる)。`editors/vscode/`に
      TextMate文法(`syntaxes/mesh.tmLanguage.json`)+ 拡張マニフェスト(`package.json`)+
      `language-configuration.json`(コメント`//`・括弧の自動補完)。キーワード・組み込み型・
      組み込み関数・struct/typeの宣言名・関数宣言名/呼び出し・文字列補間(`${...}`。エスケープ
      `\$`と区別)・判別可能unionの部分構造パターン(`{ kind: "..." }`)・ジェネリクス(`fn f<T>`)を
      個別のスコープでハイライト。実装時に`vscode-textmate`/`vscode-oniguruma`(VS Code本体と
      同じトークナイズエンジン)を使い、代表的なMeshソース片を実際にトークナイズして
      スコープを検証($nix向けCLIツールでは検証できないため、スクラッチパッドで一時的に使用。
      リポジトリの依存には追加していない)。マーケットプレイス未公開 —
      `editors/vscode/README.md`にローカルインストール手順を記載
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
