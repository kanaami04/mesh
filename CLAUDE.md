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
2. ソースコード(`rust/src/types.rs` の `type_equals` 等)
3. `todo.md` — 次にやること

## 開発コマンド

```sh
mise run check          # bunx tsc --noEmit(playgroundのTSだけ)
mise run playground     # ブラウザプレイグラウンド
mise run run-examples   # examples/*.mesh を全部実行
mise run rust-test      # cd rust && cargo test(コンパイラ本体)
mise run rust-check     # cd rust && cargo clippy --all-targets
mise run parity         # 診断の出力が記録どおりか(旧: TS版との突き合わせ)
mise run sweep          # 「形の組み合わせ」を機械生成してTS版と突き合わせる(parityの死角)
mise run drift          # 自分の変更が偽にしたコメントの候補を出す
mise run oracle-hunt    # TS版オラクル(git履歴から復元)とランダム生成物を突き合わせる
                        # **rust/が編集中・バイナリが古いと見送る**(exit 3。測っていない
                        # ので「差0件」ではない)。測りたいなら ORACLE_HUNT_FORCE=1

scripts/agent-timeout.sh 10 <エージェントID...>   # レビュー用エージェントの目覚まし
                                                 # (run_in_background で起動する)
```

**出荷する前に `mise run parity` と `mise run drift` を回す。**
parityは診断の出力が記録と違えば失敗する(生成JSの方は`rust/tests/codegen_snapshot.rs`が見る)。
driftは候補を出すだけなので、出た行と**変更した関数のdocコメント**は自分で読んで判断する。

**`mise run oracle-hunt` は出荷前の必須ではない**(探索用)。ただし**移植漏れを疑うとき**と
**診断の効き方を変えたとき**は回す価値がある——parity/sweepは記録としか比べないが、
こちらは**オラクルが判定する**ので「記録が凍結時点で既に間違っていた」場合に届く。
回すなら**ビルドしてコミットしてから**(汚れたツリーでは見送られる)。

**「オラクルを失った」は正しくない。** TS実装は`cd7273a`で削除されただけで、
git履歴から**復元して実行できる**:

```sh
mkdir -p /tmp/ts-oracle
git archive cd7273a^ src package.json tsconfig.json bun.lock | tar -x -C /tmp/ts-oracle
(cd /tmp/ts-oracle && bun install --frozen-lockfile && bun src/cli.ts check <file>)
rm -rf /tmp/ts-oracle   # 使い終わったら消す
```

失ったのは常時稼働の並走であって、問い合わせる手段ではない。移植漏れを埋めるときは
**挙動を推測する前にまずオラクルに聞く**——2026-07-28はこの手順で3件の移植漏れを
「本当にバグか」「どう直すか」「直って合っているか」まで実測で決めた。
`mise run oracle-hunt`はこれを自動化したもの(キャッシュは`/tmp/mesh-ts-oracle`。
**プロセス間で共有されるので、手で壊す実験をするときは他のセッションに注意**)。

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

## 実装は `rust/` だけ(2026-07-27にTS実装を撤去した)

**`rust/` が本番のコンパイラ**。元はTS実装(`src/`)からの移植で、TS版が
「正しさの基準(オラクル)」を務めていたが、撤去条件(1)〜(5)を満たして
2026-07-27に撤去した。経緯は `docs/ts-removal-plan.md`。

**常時稼働の並走が無くなったので、CIと日常の基準は「記録との一致」になった**:
`tests/parity/*/expected.txt`・`tests/parity-examples/`・`tests/codegen-snapshots/`・
`tests/sweep-expected.txt` が撤去時点の出力を凍結している。
**これらを `--update` で更新したら、差分を必ず読むこと**——説明できない変化は退行。

**ただし「オラクルに聞けなくなった」わけではない**(上記「『オラクルを失った』は正しくない」節)。
記録は*凍結時点で既に間違っていたもの*には永遠に届かないので、**移植漏れを疑うときは
記録ではなくオラクルに聞く**。役割分担: 記録=CI・退行検出 / オラクル=移植の正しさの判定。

移植期に効いた原則は撤去後も有効: 診断の優劣は
**誤検知＞違う診断コード＞検出漏れ** の順に悪く、迷ったら検出漏れ側に倒す。
検証手順は docs/handoff.md「検証の進め方」が一次情報源。

## メモリとdocsの使い分け

チーム/マシン横断で共有したい内容(進め方の合意・設計決定など)はClaudeのメモリではなく
このリポジトリのdocsに書く。メモリはマシンごとに独立していて同期されないため、
別マシンのセッションからは読めない。
