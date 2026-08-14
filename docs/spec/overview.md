# Musubi 言語仕様 — 骨子(Phase 1で確定させる)

各章の状態: ✅決定(ADRあり) / ⬜未着手

| 章 | 内容(カッコ内は議論のたたき台であり未決定) | 状態 |
|---|---|---|
| 1. 字句 | トークン、コメント(`//`のみ)、識別子(ASCII)、リテラル(数値フルセット・文字列は単一行+`${式}`補間)、改行区切り+継続規則 | **✅執筆済み → [01-lexical.md](01-lexical.md)** |
| 2. 型 | int+float、文字列、struct、ジェネリクス、union(判別可能性検査・フラット化)、名前的型付け、type透過別名、期待型伝播 | **✅執筆済み → [02-types.md](02-types.md)** |
| 3. 式と演算子 | 優先順位、値意味論、等価、if式/match式、narrowing、or/`?` | **✅執筆済み → [03-expressions.md](03-expressions.md)** |
| 4. 文と制御構造 | let/mut(不変デフォルト・シャドーイング禁止)、if式、forの3形のみ、return | ✅方針(ADR-0008/0009/0019)、詳細⬜ |
| 5. 関数 | 宣言、局所型推論(名前付きは注釈必須、無名関数は文脈的型付けで省略可) | ✅方針(ADR-0007/0029)、詳細⬜ |
| 6. エラー処理 | `T \| error`+`?`伝播+`or`束縛形 `or e => { 式 }`+構造化エラー+panicは境界回復(例外・recoverなし) | ✅方針(ADR-0005/0027/0028)、詳細⬜ |
| 7. モジュール | パッケージ=ディレクトリ、export明示、常に修飾、ネストパス可、循環禁止 | ✅方針(ADR-0017)、詳細⬜ |
| 8. 並行処理 | 色なし(async/await非露出)+選択的async出力。並行構文(spawn系)の詳細はPhase 5 | ✅方針(ADR-0021)、詳細⬜ |
| 9. UI構文 | コンポーネント、state、view(JSX風) | ✅方式のみ(ADR-0003)、中身⬜ |
| 10. JS interop (FFI) | extern宣言+js.Value+検証つき変換(無検証のJS値は入らない) | ✅方針(ADR-0022)、詳細⬜ |
| 11. 標準ライブラリ | core/json(検証つきデコード)/http/ui | ⬜ |
| 12. ツール仕様 | musubi CLI、fmt、test | ⬜ |

## 構文スケッチ(たたき台 — **未決定**。Phase 1の議論素材)

```musubi
// BE も FE も同じ言語。これは全体の雰囲気を掴むためのスケッチ。

struct User {
  name: string
  age:  int
}

// 戻り値の型は閉じ括弧の後に空白のみで置く(ADR-0011)
// 「見つからない」はunion型で値として返す(ADR-0005)
fn findUser(id: int) User | none {
  ...
}

fn greet(u: User) string {
  return "こんにちは、" + u.name
}

// ---- FE: UI は言語組み込み構文(ADR-0003) ----
component Counter() {
  state count: int = 0

  view {
    <div>
      <p>カウント: {count}</p>
      <button onClick={count += 1}>+1</button>
    </div>
  }
}

// ---- BE: サーバも同じ言語 ----
fn main() {
  http.serve(3000, fn(req) {
    match findUser(1) {
      User u => { respond(200, greet(u)) }
      none   => { respond(404, "not found") }
    }
  })
}
```

## Phase 1 検討メモ(仕様の詳細を書くときに拾うこと)

- **3章からの申し送り(執筆章の完了条件に含める)**: 4章=continue/breakへの「通過する側の型」narrowing規則(X-21)+ヘッダ位置structリテラルの括弧必須規則(if/for/match共通)。5章=関数本体の最終式が値位置か(暗黙のreturnの可否。X-12)。

- **エラーの文脈付き伝播(`? "文脈"`)の内部表現**: 文字列への畳み込みではなく `{message, cause}` の連鎖で元エラーを保持する(構造化エラーのフィールドを失わないため。ADR-0005の詳細設計)。
- **UI構文と不在の相互作用**: `T | none` をviewで表示するには絞り込み必須=「画面にundefinedと表示される」バグがコンパイルエラーになる。Phase 6で実証する。
- **channel終端**: 並行処理を決める際は新語彙を作らず `T | closed` (ADR-0005の語彙)に揃える。

## 仕様の書き方ルール

- 各章は「文法(EBNF風)+ 意味 + 正例 + **負例(コンパイルエラーになるべき例)**」の4点セットで書く。
- 意味の記述は**EARS風の受け入れ基準**で書く(例:「matchの腕がunionの全ケースを網羅していないとき、コンパイラはエラーE0301を報告すること」)。曖昧な散文でなく、そのままテストに落ちる文にする。
- 正例・負例はそのまま conformance テスト(tests/)になり、1:1対応を維持する。
- 仕様変更は必ずADR経由(CLAUDE.mdの正文書ルール参照)。
