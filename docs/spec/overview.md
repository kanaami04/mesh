# Mesh 言語仕様 — 骨子(Phase 1で確定させる)

各章の状態: ✅決定(ADRあり) / ⬜未着手

| 章 | 内容(カッコ内は議論のたたき台であり未決定) | 状態 |
|---|---|---|
| 1. 字句 | トークン、コメント(`//`のみ)、識別子(ASCII)、リテラル(数値フルセット・文字列は単一行+`${式}`補間)、改行区切り+継続規則 | **✅執筆済み → [01-lexical.md](01-lexical.md)** |
| 2. 型 | int+float、文字列、struct、ジェネリクス、union(判別可能性検査・フラット化)、名前的型付け、type透過別名、期待型伝播 | **✅執筆済み → [02-types.md](02-types.md)** |
| 3. 式と演算子 | 優先順位、値意味論、等価、if式/match式、narrowing、or/`?` | **✅執筆済み → [03-expressions.md](03-expressions.md)** |
| 4. 文と制御構造 | let/mut、代入・複合代入、if文、forの3形、break/continue、return、ヘッダ規則 | **✅執筆済み → [04-statements.md](04-statements.md)** |
| 5. 関数 | 宣言、レシーバfn、無名関数と参照捕捉クロージャ、暗黙のreturn、main | **✅執筆済み → [05-functions.md](05-functions.md)** |
| 6. エラー処理 | 失敗メンバー、error値(message/cause)、error struct、`?`のメンバー単位伝播、panic一覧と境界回復 | **✅執筆済み → [06-errors.md](06-errors.md)** |
| 7. モジュール | パッケージ・import(as別名)・可視性(リーク禁止)・名前解決・トップレベルlet限定・main配置 | **✅執筆済み → [07-modules.md](07-modules.md)** |
| 8. 並行処理 | 色なし(async/await非露出)+選択的async出力。並行構文(spawn系)の詳細はPhase 5 | ✅方針(ADR-0021)、詳細⬜ |
| 9. UI構文 | コンポーネント、state、view(JSX風) | ✅方式のみ(ADR-0003)、中身⬜ |
| 10. JS interop (FFI) | extern宣言+js.Value+検証つき変換(無検証のJS値は入らない) | ✅方針(ADR-0022)、詳細⬜ |
| 11. 標準ライブラリ | core/json(検証つきデコード)/http/ui | ⬜ |
| 12. ツール仕様 | mesh CLI、fmt、test | ⬜ |

## 構文スケッチ(たたき台 — **未決定**。Phase 1の議論素材)

```mesh
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

- **3章からの申し送り**: ~~4章=continue/breakのnarrowing+ヘッダstructリテラル~~(**4章S-17/S-15で消化済み**)。~~5章=関数本体の値位置~~(**5章F-5で消化済み。暗黙return採用=ADR-0036**)。~~7章への申し送り~~(**7章M-8/M-9/M-13で消化済み**)。

- ~~エラーの文脈付き伝播の内部表現~~(**6章H-6で明文化済み**。cause連鎖=ADR-0037)。
- **UI構文と不在の相互作用**: `T | none` をviewで表示するには絞り込み必須=「画面にundefinedと表示される」バグがコンパイルエラーになる。Phase 6で実証する。
- **channel終端**: 並行処理を決める際は新語彙を作らず `T | closed` (ADR-0005の語彙)に揃える。
- **9章への申し送り**: 文脈キーワード(state/view)と同名のパッケージ参照名を `component` 内で使えるか(7章M-3の参照名とL-27の字句モードの相互作用)。8章への申し送り: なし(トップレベル初期化は定数式限定=M-10のため、async初期化問題は構造的に不存在)。

## 仕様の書き方ルール

- 各章は「文法(EBNF風)+ 意味 + 正例 + **負例(コンパイルエラーになるべき例)**」の4点セットで書く。
- 意味の記述は**EARS風の受け入れ基準**で書く(例:「matchの腕がunionの全ケースを網羅していないとき、コンパイラはエラーE0301を報告すること」)。曖昧な散文でなく、そのままテストに落ちる文にする。
- 正例・負例はそのまま conformance テスト(tests/)になり、1:1対応を維持する。
- 仕様変更は必ずADR経由(CLAUDE.mdの正文書ルール参照)。
