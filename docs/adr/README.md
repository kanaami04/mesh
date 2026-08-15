# ADR(Architecture Decision Record)運用ルール — 是正対策の仕組み

言語開発では「やっぱり出来た/やっぱり出来なかった」で前提がひっくり返ることが必ず起きる。決定を覆すこと自体は健全だが、覆した事実が文書に反映されないと矛盾したまま進んでしまう。ADRと前提台帳はその是正対策。

さらに本プロジェクトではAIが実装を担うため、ADRは**AIエージェントの長期記憶**でもある。AIはセッションごとに記憶を失うので、「なぜこう決めたか」がリポジトリに無いと、過去の決定を蒸し返したり決定に反するコードを書いたりする。ADRはそれを防ぐ読み込み前提のコンテキストとして書く(結論を先頭に、短く、機械が拾いやすい構造で)。

**作成は `adr` スキルで行う**(決定が確定したら自動発動、`/adr` で明示呼び出しも可。採番・索引・前提台帳の更新まで一括実施)。検査は `drift-check` スキル。

## 運用ルール

1. **大きな決定をしたら、番号を振ったADRを1ファイル書く**(テンプレは template.md)。
2. ADRは**追記専用**。過去のADRの本文は書き換えない(歴史を消さない)。ただし**mainにマージされる前のADR**(作業ブランチ上で執筆中のもの)は推敲として改稿してよい(マージ後は追記専用)。
3. **決定を覆すとき**は: 新ADRを書く → 旧ADRのStatusを `superseded by ADR-XXXX` に変更(これだけは旧ファイルを触ってよい)→ ASSUMPTIONS.md の関連前提を更新 → spec/等の影響文書を直す。
4. 各ADRには「**この決定が依存している前提(Assumptions)**」を明記する。前提が崩れた時にどのADRを見直すべきか逆引きできるようにするため。
5. 迷ったらADRにする。「書くほどでもない」と思った決定が後で一番揉める。

## 前提台帳(ASSUMPTIONS.md)

「まだ検証していないが、そうであると信じて進めている事柄」の一覧。各前提に検証状況と、崩れた場合の影響範囲(関連ADR)を記録する。フェーズが進むたびに見直す。

## 現在のADR一覧

- [ADR-0001](0001-implementation-language-rust.md) — 実装言語をRustにする ✅
- [ADR-0002](0002-target-js-transpile.md) — コンパイルターゲットはJS(トランスパイル方式) ✅
- [ADR-0003](0003-ui-builtin-syntax.md) — UIは言語組み込み構文 ✅
- [ADR-0004](0004-language-name-musubi.md) — ~~言語名はMusubi~~ ⛔ superseded by 0039
- [ADR-0005](0005-absence-and-failure-as-union.md) — 不在と失敗はunion型(`T | none` / `T | error`)✅
- [ADR-0006](0006-nominal-typing.md) — 型の同一性は名前的 ✅
- [ADR-0007](0007-local-type-inference.md) — 局所型推論(関数境界の注釈必須)✅
- [ADR-0008](0008-bindings-let-mut.md) — let/mut・不変デフォルト・シャドーイング禁止 ✅
- [ADR-0009](0009-if-as-expression.md) — ifは式(三項演算子なし)✅
- [ADR-0010](0010-newline-statement-separator.md) — セミコロン無し・改行区切り ✅
- [ADR-0011](0011-return-type-go-style.md) — 戻り値の型はGo流・空白のみ(アロー廃止)✅
- [ADR-0012](0012-struct-receiver-methods.md) — struct+レシーバfn(classなし)✅
- [ADR-0013](0013-match-arms-brace-form.md) — ~~matchの腕はブレース形~~ ⛔ superseded by 0016
- [ADR-0014](0014-string-codepoint-unit.md) — 文字列はコードポイント単位 ✅
- [ADR-0015](0015-numbers-int53-float.md) — 数値はint(53bit)+float ✅
- [ADR-0016](0016-match-arms-fat-arrow.md) — ~~matchの腕は `=>` +式形~~ ⛔ superseded by 0025
- [ADR-0017](0017-modules-package-directory.md) — パッケージ=ディレクトリ・常に修飾 ✅
- [ADR-0018](0018-string-interpolation.md) — 文字列補間 `${式}` ✅
- [ADR-0019](0019-for-three-forms.md) — ループはforの3形のみ ✅
- [ADR-0020](0020-generics-all-no-constraints.md) — ジェネリクスは全宣言・制約なし ✅
- [ADR-0021](0021-colorless-async.md) — 並行処理は色なし+選択的async出力 ✅
- [ADR-0022](0022-declarative-ffi-verified-boundary.md) — 宣言的FFI+境界検証 ✅
- [ADR-0023](0023-union-naming-type-keyword.md) — unionの命名は `type Name = A | B` ✅
- [ADR-0024](0024-comments-and-naming.md) — コメントは//のみ・camelCase/PascalCase ✅
- [ADR-0025](0025-match-arm-body-always-block.md) — matchの腕は `=> { ... }` 常にブロック(0016を上書き)✅
- [ADR-0026](0026-line-continuation-binary-only.md) — 行継続は二項演算子限定・後置`?`は文終端(0010を精密化)✅
- [ADR-0027](0027-panic-boundary-recovery.md) — panicは境界回復・recover非公開 ✅
- [ADR-0028](0028-or-binding-block-form.md) — orの束縛形は `or e => { 式 }`(0005を精密化)✅
- [ADR-0029](0029-anonymous-fn-contextual-typing.md) — 無名関数は文脈的型付けで省略可(0007を精密化)✅
- [ADR-0030](0030-lexical-details.md) — 字句詳細: 識別子ASCII・数値フルセット・単一行文字列 ✅
- [ADR-0031](0031-continuation-list-bracket-depth-contextual-keywords.md) — 継続一覧確定・括弧深度・UI語の文脈キーワード化 ✅
- [ADR-0032](0032-type-system-details.md) — 型詳細: list/map記法・type透過・文字列添字・範囲外panic ✅
- [ADR-0033](0033-value-semantics.md) — structは値意味論・等価は値の比較(list/map==はエラー)✅
- [ADR-0034](0034-expression-details.md) — matchは型パターンのみ・範囲式はforヘッダ限定・narrowingは変数のみ ✅
- [ADR-0035](0035-statement-details.md) — パラメータ不変・効果なし式文/未使用変数はエラー・deferなし ✅
- [ADR-0036](0036-closures-and-implicit-return.md) — クロージャはmut参照捕捉・暗黙のreturn ✅
- [ADR-0037](0037-error-handling-details.md) — error値の構造・error type不採用・cause連鎖 ✅
- [ADR-0038](0038-module-details.md) — トップレベルはletのみ・定数式初期化・as別名・リーク禁止・ルートimport不可 ✅
- [ADR-0039](0039-language-name-mesh.md) — 言語名はMesh(0004を上書き)✅
- [ADR-0040](0040-spec-authoring-decisions.md) — 仕様執筆時確定事項の追補採録(X-29/F-1/T-26/H-10/M-16/L-7)✅
- [ADR-0041](0041-newline-codes-crlf.md) — 改行はLF/CRLFを受理・単独CRはE0117・Unicode改行類は非改行 ✅
