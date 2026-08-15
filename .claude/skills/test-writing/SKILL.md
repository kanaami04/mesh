---
name: test-writing
description: Meshコンパイラ(crates/mesh)のテストの書き方規約。テストを書く・レビューするとき、およびtddスキルのRed/Greenサブエージェントへ規約を渡すときに使う。字句解析器の最初の3 TDDサイクル(2026-08-15)で実証した規約の蒸留。
---

# test-writing — Meshコンパイラのテスト規約

役割分担: **tddスキル**=プロセス(Red→Green→Refactor)、**docs/testing-strategy.md**=戦略(4層・正負例セット)、**このスキル**=具体的な書き方。

## コマンド(このマシン固有)

cargoはPATH外にある。すべてのcargo実行は次の形:

```
export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/kanayama/kanaami/language && cargo test -p mesh
```

該当テストのみは `--test <ファイル名>`。Greenの成功条件には `cargo fmt --all --check` と `cargo clippy --all-targets -- -D warnings` を必ず含める。

## 配置と命名

- TDD対象のテストは `crates/mesh/tests/<モジュール名>.rs`(統合テスト。例: `tests/lexer.rs`)。
- fn名は英語snake_caseで「入力→期待」を表す(`empty_source_produces_no_tokens`)。
- 各テストに日本語docコメントで振る舞いを1文書く。**仕様の規則に対応するときは規則番号を書く**(例: `仕様1章L-6`)。
- 1テスト関数=1振る舞い。同一振る舞いの正例なら複数ケースを同関数に入れてよい(`"42"` と `"1 22"`)。

## 期待値の書き方

- **丸ごと明示**する: `assert_eq!(tokens, vec![Token { kind: TokenKind::Int, text: "42".to_string() }])`。`is_ok()` だけ等の曖昧検証は不可。
- `expect` のメッセージは日本語で「〜であること」(失敗時にそのまま仕様文として読める)。

## TDDサイクル中の分担(実証済みの運用)

- **Redのスタブ規則**: テストのコンパイルに必要な型定義のみ追加してよい(enumバリアント・struct・シグネチャ。中身は `todo!()`)。ロジックは1行も書かない。「コンパイルエラーで落ちる」はRedではない——assertionまたは`todo!()`のpanicで落ちること。
- **Greenの最小主義**: テストに無い挙動を先取りしない。実例: 未対応文字は最小の `Err(LexError)`(黙殺スキップは暗黙の仕様決定になるので不可)、予約語は `"let"` 1個のif比較(一覧テーブル化はテストが増えてから)。
- **Refactorはメイン**: 2回出た走査パターンは部品化する(実例: `scan_while`=最長一致の共通部品)。リファクタ後に全テスト+fmt+clippyを再実行。

## スナップショット(insta)の併用

- **TDDサイクルの検証には使わない**(期待値明示のassertで回す)。サイクル群の完了後に**回帰の網**として追加する。
- 書き方: テストファイル末尾に `insta::assert_debug_snapshot!(対象式)`。スナップショットは `tests/snapshots/` に生成される。
- 初回生成: `INSTA_UPDATE=always cargo test -p mesh --test <名前>` → 生成された `.snap` を**必ず目視レビュー**(内容が仕様と一致するか)→ 通常実行で緑を確認 → `.snap` もコミットする。
- 意図しない差分が出たら「実装のバグ」か「仕様の意図的変更」かを判断し、後者のみ更新する。

## 未確立(次のサイクル群で規約化する)

- **負例テスト**(期待エラーコードE01xx・位置の検証方法): LexErrorが位置・コードを持つようになるサイクルで確立する。
- **conformance対応**(仕様のテストID `tests/01-lexical/semicolon` とRustテストの対応表): 負例規約とあわせて確立する。
- **E2Eハーネス**(生成JSのNode実行): コード生成のサイクルで確立する。

確立したらこのスキルに追記し、「未確立」から消す。
