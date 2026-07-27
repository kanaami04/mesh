# CLAUDE.md

Mesh(TypeScriptの型 × Goのシンプルさ・並行処理を持つ、JSにトランスパイルされる言語)の開発リポジトリ。
ここは「知らないと事故る/遠回りする」ことだけを書く。詳細は各docsが一次情報源なので重複させない。

## 最初に読むもの

新しいセッションはまず **docs/handoff.md** を読む。他のdocs(README/requirements/features/design-agenda/todo)
のどこに何が書いてあるかの案内役になっている。

## 開発の進め方(協働スタイル)

- ユーザー(kanayamaさん)は自分でコードを書かない。Claudeが実装しながら日本語で解説し、一緒に学ぶスタイル。

## ドキュメントのdriftに注意

**自分の変更が偽にしたコメントを、出荷前に必ず潰す。** milestone 46〜51で**7回連続**、
code review指摘の大半がこれだった(実装は正しいのに、周りのコメントが「その診断は未移植」
「〜は効かない」と言ったまま残る)。**特に自分が変更した関数のdocコメントは必ず読み直す**
——7件中4件がそれだった回がある。手順(3種類の検索語でgrepする)は
docs/handoff.md「検証の進め方」の2''が一次情報源。

`docs/requirements.md` は「正」の文書のはずだが、後続の設計決定(design-agenda.mdでの討議)に
追随できず古い記述が残ることがある。現在地を確認したいときは以下を優先する:

1. `docs/features.md` — 「できる・できない表」。現在地の一次情報源
2. ソースコード(`src/types.ts` の `typeEquals` 等)
3. `todo.md` — 次にやること

## 開発コマンド

```sh
mise run test           # bun test(TS実装)
mise run check          # bunx tsc --noEmit
mise run playground     # ブラウザプレイグラウンド
mise run run-examples   # examples/*.mesh を全部実行
mise run rust-test      # cd rust && cargo test(Rust移植版)
mise run rust-check     # cd rust && cargo clippy --all-targets
mise run parity         # TS版(オラクル)とRust版のcheck出力を突き合わせる
mise run sweep          # 「形の組み合わせ」を機械生成してTS版と突き合わせる(parityの死角)
mise run drift          # 自分の変更が偽にしたコメントの候補を出す

scripts/agent-timeout.sh 10 <エージェントID...>   # レビュー用エージェントの目覚まし
                                                 # (run_in_background で起動する)
```

**Rust移植のマイルストーンを出荷する前に `mise run parity` と `mise run drift` を回す。**
parityは「Rust側だけに出る診断」(この移植で最悪の不具合)と**生成JSの差**があれば失敗する。
driftは候補を出すだけなので、出た行と**変更した関数のdocコメント**は自分で読んで判断する。

**parityは「コーパスに載っている形」しか測れない。** milestone 65で見つけた誤検知5種は
`!`/`&&`/`||`で包んだ条件式——**単体では全部コーパスにある要素の組み合わせ**だったので
parityは0件のまま通り続けていた。**検査の効き方を変えたら `mise run sweep` も回す**
(軸の直積を機械生成して突き合わせる)。新しい構文を足したら
`rust/tests/corpus_coverage.rs` が「コーパスに一度も出ない構文」を検出して落ちる。

環境構築(mise・system パッケージ・gh認証・`/code-review`プラグイン)は **docs/setup.md** が一次情報源。

## PRワークフロー

feature branch → PR → CI green + `/code-review --comment` → **squash mergeのみ**。
`.claude/hooks/enforce-code-review.sh` が `### Code review` 見出しのコメント無しでの
`gh pr merge` を機械的に拒否する(確認できないときは常にdenyする設計)。
**`git stash` も `.claude/hooks/block-git-stash.sh` が拒否する**——別の状態を見たいときは
`git worktree` を使う(経緯は `.claude/skills/git-worktree`。読み取り専用の
`git stash list`/`show` は通る)。

レビューが不要だと判断した場合(docsのみの変更など)は、黙って飛ばさず
`### Code review skipped: <理由>` という見出しのコメントを残す(**理由の記載は必須**。
経緯と根拠は docs/handoff.md「開発の進め方」節)。

**レビュー用サブエージェントを起動したら、同時に目覚ましを仕掛ける**
(`Bash(run_in_background: true)` で `scripts/agent-timeout.sh 10 <エージェントID...>`)。
`TaskList`にエージェントは出ず経過時間も見えないので、**これが無いと放置に気づけない**。
**調べる範囲も必ず指定し、止まったら`TaskStop`で止める。**
1セッションで3回、範囲を絞らなかったエージェントが1時間以上ハングして手が止まった
(対象PRを列挙しない / 件数検証にworktree+フルビルド / 確認観点を7項目も列挙)。
**`TaskList`には出ないので起動時のエージェントIDを渡す。** 1観点落ちても他が実機検証して
いれば網羅性は保てる。マージ後は`git worktree list`と`git branch`で残骸を確認する
(比較用ブランチが5本残っていたことがある)。詳細は docs/handoff.md「開発の進め方」節。

**PR番号の注意**: 2026-07-21のリポジトリ移管より前のコミットメッセージには、旧リポジトリの
squash mergeで付いたPR番号(`(#41)`等)が文字列として残っている。**これらは現リポジトリの
PRとは無関係**で、現リポジトリのPR番号は移管後に1から振り直されている(このPR自体が#36)。
移管前の作業を指すときはPR番号ではなくコミットSHAを使う。

## Rust移植について

`rust/` はTS実装(`src/`)の書き換えではなく、並行してゼロから育てている移植版。
TS実装が引き続き本番として動き続けている。進捗はコミット単位のマイルストーンで、
詳細はtodo.mdの各マイルストーン項目・docs/handoff.mdの「Rust移植の現状」節が一次情報源。

**TS実装がオラクル**。正しさの基準は「TS版と一致するか」で、優劣は
**誤検知(Rust側だけに出る診断)＞違う診断コード＞検出漏れ** の順に悪い。
迷ったら検出漏れ側に倒す。**実装前にTS版を実測する**(位置・件数・順序まで)。
milestone 46〜48は3回連続で誤検知を出荷しかけ、いずれも実装者の検証をすり抜けて
code reviewが見つけた——検証手順は docs/handoff.md「検証の進め方」が一次情報源。

## メモリとdocsの使い分け

チーム/マシン横断で共有したい内容(進め方の合意・設計決定など)はClaudeのメモリではなく
このリポジトリのdocsに書く。メモリはマシンごとに独立していて同期されないため、
別マシンのセッションからは読めない。
