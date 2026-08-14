# 仕様 6章 — エラー処理(Error Handling)

失敗メンバーの定義・組み込みerror・error struct・`?` のメンバー単位伝播・panicと境界回復を定める。根拠ADR: [0005](../adr/0005-absence-and-failure-as-union.md)(union路線)/ [0027](../adr/0027-panic-boundary-recovery.md)(panic境界回復)/ [0028](../adr/0028-or-binding-block-form.md)(or束縛)/ [0037](../adr/0037-error-handling-details.md)(error値の構造・error type不採用・cause連鎖)。

エラーコードは `E06xx`。負例のテストIDは `tests/06-errors/` 配下。規則番号(H-n)は安定ID。本章は設計監査#15(`?` のメンバー単位検査)とspec/overview検討メモ(cause連鎖)を消化する。

## 6.1 失敗メンバー

- **H-1**: 型の**失敗メンバー**とは、unionのメンバーのうち `none`・組み込み `error`・`error struct` で宣言された型のこと。失敗メンバーは `?`(伝播)・`or`(フォールバック)・match/is(分岐)の4手段で処理できる(3章X-24〜X-26)。それ以外のメンバーが**成功メンバー**である。
- **H-10**(型パラメータの分類): 失敗メンバーの分類は**宣言時の型**で行い、型パラメータ `T` は常に**成功メンバー**として扱うこと(インスタンス化で再分類しない)。したがって `fn f<T>(x: T | none) T | none { x? }` に `T = DbError` を与えても、`x?` が伝播するのはnoneのみで、DbError値は成功値として流れる(実行時タグで判別可能なため健全)。〔挙動検証: `generic-failure-classification`〕

## 6.2 組み込み `error`

- **H-2**: 組み込み `error` 値は2つのフィールドを持つこと: `message: string` と `cause: error | none`。生成は**生成式** `error(式)`(`message = 式`、`cause = none`。式はstring)。`error` は完全予約語(L-7)のため、この生成式は識別子の呼び出しではなく専用の構文形である(3章 `errorExpr`)。したがって `error` を関数値として参照する(`let f = error`)ことはできない(E0104)。フィールドは**読み取り専用**であり、代入(`e.message = "x"`)はエラー E0603(S-7に対する制限)。生成式の引数がstringでないときはE0213(T-24)。v1ではcauseを手動で設定する手段は無い(`? "文脈"` のみが連鎖を作る。ADR-0037)。〔正例: `error-fields` / 負例: `error-field-assign`、`error-as-value`(`let f = error`)、`error-arg-type`(`error(42)`)〕
- errorのメッセージ文言はstyle-guide原則4(短く・責めない・平易・修正候補つき)に従うことを推奨する(規範はコンパイラ自身の診断にのみ課す)。

## 6.3 error struct

```ebnf
errorStructDecl = [ "export" ] "error" "struct" identifier [ "<" typeParams ">" ] "{" { field } "}"
```

(`typeParams`・`field` は2章structDeclの定義を共用。宣言可能位置はトップレベルのみ=7章)

- **H-3**: `error struct` はstructの全規則(フィールド・レシーバfn・リテラル生成・名前的同一性・等価=2章/3章/5章)に従い、加えて**失敗メンバーになる**(H-1)。組み込み `error` との上位・下位関係は無い(T-21)。`message`/`cause` という名前のフィールドも通常のフィールドとして定義できる(組み込みerrorとは別の型なので衝突しない)。ジェネリックなerror structも書けるが、`E<int> | E<string>` はT-9(同一構成子)で判別不能エラーになる(既存規則がそのまま適用される)。〔正例テスト: `error-struct-basic`(宣言・生成・matchでの分岐)〕
- **H-4**: `error type` という宣言は**存在しない**(ADR-0037)。error structのunionに名前を付けたいときは通常の `type` を使う(透過別名=T-5)。`error type` と書いたらE0104のメッセージ変種で `type` を案内すること。〔負例: `error-type-decl`(期待コードはE0104変種)〕

```
error struct DbError { table: string }
error struct Timeout { ms: int }
type StoreErr = DbError | Timeout          // 通常のtypeで束ねる

fn find(id: int) User | DbError | Timeout { ... }
```

## 6.4 `?` のメンバー単位伝播(設計監査#15の消化)

- **H-5**(素の `?`): `expr?` の**囲む関数**とは、最内の関数(無名関数を含む。F-15のreturnと同じ)のこと。関数の外(トップレベル)の `?` はエラー E0604。`expr?` は、オペランドの型の**各失敗メンバーが、囲む関数の戻り値型のunionにそれぞれ含まれる**ときにのみ書けること。不足があればエラー E0601 とし、不足メンバーを列挙して「戻り値型に追加するか、`? "文脈"` でerrorに昇格するか、matchで処理してください」と案内すること。伝播時は失敗値がそのまま(変換なしで)returnされる。戻り値型が `none` の関数(mainを含む)での `x?`(x: `T | none`)は合法であり、「不在なら静かに関数を抜ける」guardとして機能する。

```
fn load(id: int) User | DbError | error {
  let row = query(id)?          // query: Row | DbError → DbErrorは戻り値に含まれる: OK
  parse(row)?                   // parse: User | error → errorも含まれる: OK
}

fn bad(id: int) User | error {
  let row = query(id)?          // エラー E0601: DbError が戻り値型にありません
  ...
}
```

〔正例: `question-member-ok`、`none-guard-in-none-fn` / 負例: `question-member-missing`(noneの不足= `int | none` を `int | error` 戻りで `?` する形を含む)、`question-top-level`〕
- **H-6**(`? "文脈"`): `expr ? "文脈"` は、オペランドの**すべての失敗メンバーを組み込み `error` に昇格**して伝播する。囲む関数の戻り値型に `error` が**含まれてさえいればよい**(他の成功メンバー・失敗メンバーと共存可。errorが無ければE0602)。生成されるerrorは `message = 文脈`(補間可)、`cause` は: 元が組み込みerrorならその値、error structなら**型名とフィールドを表示文字列に展開したmessageを持つ通常のerror値**(そのerror値の `cause` は `none`。表示の書式は診断表示の実装定義とし、スナップショットテストで固定する)、noneなら `none`(ADR-0037。表示でフィールドの情報は失われないが、型付きで取り出す手段はv1に無い)。文脈式は失敗時のみ評価される。**文脈式の中に `?` は書けない**(E0326のメッセージ変種)。〔正例: `question-context-ok` / 負例: `question-context-no-error`、`question-in-context` / 挙動検証: `error-cause-chain`(2段の `? "文脈"` を経たログにDbErrorのフィールド値が現れること)〕
- `?` の式としての型・構文上の位置はX-26、orとの関係はX-24/X-25。orで受けて `error("...")` で包み直すとcause連鎖は繋がらない(連鎖を作るのは `? "文脈"` のみ)ことに注意。

## 6.5 panicと境界回復(ADR-0027)

- **H-7**(panicする操作の一覧): v1でpanicするのは次のみであること。ユーザーがpanicを起こす構文・関数は存在しない。

| 操作 | 規則 |
|---|---|
| intの `+ - *` の安全整数域超過 | X-5 |
| intの `/ %` のゼロ除算 | X-5 |
| list・stringの添字範囲外 | T-16 |
| 処理系資源の枯渇(スタックオーバーフロー等。JSエンジン由来) | 本行(エンジン例外を捕捉してpanicに分類する。確実な分類の可否は境界実装時=Phase 5/6に検証) |
| ランタイム内部の防波堤(ユーザーからは不可視) | — |

- **H-8**(境界回復): panicが発生したとき、**最も近いランタイム境界**がそれを受け止め、その処理単位だけを失敗として扱い、プログラム全体は継続すること。境界は: `http.serve` のリクエスト処理(500応答)・UIイベントハンドラ(イベント中断+開発時エラー表示)・`mesh test` の各テスト(fail)。境界の外(mainまで到達)では、位置・原因つきの診断を出力してプロセス終了(Node)またはコンソール報告(ブラウザ)。〔挙動検証: `panic-diagnostics`(範囲外アクセスの診断に位置と原因が含まれること。境界回復自体の検証はランタイム実装後=Phase 5/6のテストへ申し送り)〕
- **H-9**: コンパイラ・ランタイムが**生成する**panicは、生成JSにおいて専用エラー(`MeshPanic`)のthrowであり、FFI由来のJS例外と区別されること(資源枯渇のエンジン例外はthrow時点ではMeshPanicでなく、**境界で事後分類**してpanic扱いになる=H-7)。FFI例外はpanicではなく、境界で `T | error` に正規化される(ADR-0022)。区別の規則とテストは10章へ申し送る。

## conformance対応表

| テストID | 種別 | 規則 |
|---|---|---|
| generic-failure-classification | 挙動検証 | H-10 |
| error-fields | 正例 | H-2 |
| error-field-assign / error-as-value / error-arg-type | 負例 | H-2 |
| error-struct-basic | 正例 | H-3 |
| error-type-decl | 負例 | H-4(E0104変種) |
| question-member-ok / none-guard-in-none-fn | 正例 | H-5 |
| question-member-missing / question-top-level | 負例 | H-5 |
| question-context-ok | 正例 | H-6 |
| question-context-no-error / question-in-context | 負例 | H-6 |
| error-cause-chain | 挙動検証 | H-6 |
| panic-diagnostics | 挙動検証 | H-8 |
