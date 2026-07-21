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
