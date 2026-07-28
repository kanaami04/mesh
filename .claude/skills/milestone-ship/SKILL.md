---
name: milestone-ship
description: Mesh(特にRust移植)のマイルストーン1件を、検証→docs更新→featureブランチ→PR作成まで一気通貫で出荷する。mergeは必ず手前で止め、ユーザーの明示指示を待つ。「このマイルストーンをPRにして」「出荷して」「ship して」等で使う。
---

# milestone-ship

実装が一段落した変更を、Meshの標準ワークフロー(docs/handoff.md「開発の進め方」節が一次情報源)に
沿って **PR作成まで自動で** 進めるための手順書。**mergeは絶対に自動でやらない**
(2026-07-24のkanayamaさんとの合意。`gh pr merge` はユーザーが「マージして」と明示するまで
実行しない)。この「PR作成まで自動・mergeは明示指示まで待つ」合意はマシン横断で共有すべき内容
なので docs/handoff.md「開発の進め方」節に記載済み(このマシンのローカルメモリ
[[pr-flow-autonomy]] にも同内容があるが、メモリは別マシンから読めないためdocsが正)。

## 前提

- 実装(コード変更)自体は済んでいる想定。このスキルは「出荷パイプライン」だけを担う。
- `cargo`/`bun`/`gh` は mise 管理下。非対話シェルでは PATH に乗らないので、各コマンドの前に
  `eval "$(mise env -s bash)"` を一度流すか、`mise exec -- ...` を使う。
- git identity がこのマシンで未設定なことがある。過去コミットに合わせる:
  `git config user.name "kanaami" && git config user.email "knym4a.r0613@gmail.com"`。
- `gh` は認証済みでも git と未連携なことがある。push 前に `gh auth setup-git` を一度実行。

## 手順

### 1. 検証(ここを飛ばさない)

Rust移植の変更なら:

```sh
mise run rust-test    # 全テストパスを確認(件数が増えているはず)
mise run rust-check   # = cargo clippy --all-targets(警告はエラー化しないので出力を目視)
```

**注意**: `mise run rust-check` は `-D warnings` を付けない(`mise.toml` の定義がそう)ため
警告が出ても exit 0 になる。CI は `cargo clippy --all-targets -- -D warnings` で警告を
エラー扱いするので、CIと同じ基準で確認したいなら `(cd rust && cargo clippy --all-targets -- -D warnings)`
を直接実行するか、`mise run rust-check` の出力に警告が無いことを目視で確かめること。

さらに、TS版が正(オラクル)なので **必ずTS版と突き合わせる**:

- codegen変更 → 生成JS(`cargo run -- <file> --emit-js`)を `bun` で実行し、TS版
  (`bun run mesh run <file>`)の標準出力と **byte-for-byte 一致** を確認。
- checker/diagnostics変更 → `mesh check <file>`(Rust版 `cargo run -- check <file>` /
  TS版 `bun run mesh check <file>`)を代表的な入力数パターンで走らせ、**コード・
  メッセージ・位置(line:col)まで一致** を確認。

TS版でしか通らない/落ちる組み合わせがある点に注意(例: `or` の fallback 型照合など、
診断を出すTS版だけが弾くケース)。比較用のexample/入力は必ずTS版でも成立するものにする。

### 2. ドキュメント更新

- `todo.md` の該当マイルストーン項目に `[x]` と実装記録(設計判断・スコープ・
  テスト件数の before→after・code review 指摘があればその結末)を追記。
- `docs/handoff.md` の進捗リスト/候補リストを更新。
- 言語仕様に触れる変更なら `src/card.ts` / `docs/features.md` / `docs/design-agenda.md` も。

### 3. featureブランチ → commit → **code review** → push → PR作成

デフォルトブランチ(main)にいるなら **必ず先にブランチを切る**。

```sh
git checkout -b <topic-branch>
git status --short && git diff --stat   # ステージ前に確認(意図しないファイルを混ぜない)
git add -A
git commit -F <message-file>   # 決定の経緯・却下した代替案もメッセージに書く
# ここで code review を回す(下の「push の前にレビュー」を参照)
scripts/record-review.sh "<レビュー結果の要約>"   # または --skip "<理由>"
git push -u origin <topic-branch>
gh pr create --base main --title "..." --body "..."
```

**push の前にレビュー**(2026-07-28にmerge直前から移動)。`.claude/hooks/enforce-review-before-push.sh`
が、送るコミットのshaに対する記録(`.claude/review-log/<sha>`)が無ければ `git push` を拒否する。
PRがまだ無いので**PR単位のプラグイン版はここでは使えない**——ローカルの差分を
レビュー用サブエージェント(範囲・観点の縛りは下記「注意」と同じ)で見るか、
自分で検証できる規模なら自分で回す。ユーザーの組み込み `/code-review` を使いたい場合は、
Claudeからは起動できないのでpushの手前でユーザーに依頼して止まる。
**記録はshaに紐づく**ので、記録の後にコミットを積んだら再レビューしてから記録し直す。

`git add -A` は自動フローでも**必ず直前に `git status`/`git diff --stat` を目視**してから
使う——無関係なローカル生成物・別作業の変更を巻き込まないため(CLAUDE.md「無関係な変更は
別コミットに分ける」)。

コミットメッセージ末尾には環境指定のトレーラ(`Co-Authored-By:` / `Claude-Session:`)、
PR本文末尾には `🤖 Generated with [Claude Code]...` を付ける(ハーネス既定に従う)。

**無関係な変更は同じPRに混ぜない**(例: スキル追加とマイルストーン実装は別PR)。

### 4. CI とレビュー結果の記録(mergeの前提)

PR作成後、次の2つを **並行** で(順不同):

```sh
gh pr checks <番号> --watch   # CI green を確認
```

**手順3で回したレビューの結果を `### Code review` 見出しのコメントとしてPRに投稿する**
(`gh pr comment <番号> --body-file <file>`。見出しは**コメントの先頭行**に置く)。
`.claude/hooks/enforce-code-review.sh` が、そのコメントが無い状態での `gh pr merge` を
機械的に拒否する。**レビューを2回回すという意味ではない**——push前のレビュー結果を
PRに残し、マージ時に確認できるようにするためのもの。
レビュー不要と判断した場合は `### Code review skipped: <理由>`(**理由は必須**)。

PR作成後に追加でPR単位のレビューを回したい場合は、`code-review` プラグインが入っていれば
`code-review:code-review` スキルとしてClaude自身が起動できる(2026-07-24に`/plugin`で導入して
確認済み)。組み込みの `/code-review` は `disable-model-invocation` でユーザーしか起動できない。
レビュー指摘(80点以上)があれば対応コミットを足し、CIとレビューをやり直す。

### 5. merge(=ここでいったん止まる)

**自動でやらない。** CI green + code review コメントが揃っていても、kanayamaさんが
「マージして」と明示するまで待つ。指示が出たら:

```sh
gh pr merge <番号> --squash --delete-branch
git checkout main && git fetch --prune origin && git merge --ff-only origin/main
```

squash merge のみ(リポジトリ設定でmerge commit/rebase mergeは無効)。

### 5-b. 積み上げ(stacked)PRの注意 — 2026-07-24に踏んだ落とし穴

milestone N+1 が未マージの N のコードに依存する場合、開発中は N+1 を N のブランチの上に
積む(`gh pr create --base <Nのブランチ>`)。だが**マージ時が危険**:

- **`gh pr merge <親> --squash --delete-branch` で親ブランチを消すと、その上に積んだ子PRが
  自動で main へ張り替わらず CLOSED になることがある**(2026-07-24に実際に発生)。GitHubの
  auto-retarget は当てにしない。
- **安全な順序**(親ブランチを消す前に子を main へ逃がす。**順序が重要** — 子を rebase する
  時点で親の変更が既に main に入っている必要がある):
  1. 親を**`--delete-branch`無しで**先に squash マージ: `gh pr merge <親> --squash`
     (main に親の変更が入るが、親ブランチはまだ残る)
  2. 子PRの base を main へ張り替える: `gh pr edit <子> --base main`(親ブランチが
     まだ生きているのでこの時点では CLOSED リスク無し)
  3. 子ブランチを main に rebase して親由来の重複コミットを落とす:
     `git checkout <子> && git fetch origin && git rebase origin/main && git push --force-with-lease`
     ——**この順序なら**手順1で親の変更が既に main にあるので、親のコミットは
     「適用済み」として自動スキップされる(先に子を rebase しても main に親が無く効かない)
  4. 親ブランチを手動削除: `git push origin --delete <親ブランチ名>`(子は既に retarget 済みで安全)
  5. 子の CI 再確認 → 子をマージ(**マージは §5 の合意どおり明示指示を待つ**)
- **既にCLOSEDになった子の復旧**(先に親を`--delete-branch`してしまった場合): 消えた base
  ブランチを元コミットで復元する。元コミットのSHAは記憶や reflog より
  `gh pr view <親> --json commits`(または `git fetch origin refs/pull/<親PR番号>/head`)で
  確実に取れる。`git push origin <old-sha>:refs/heads/<親ブランチ名>` で復元 →
  `gh pr reopen <子>` → `gh pr edit <子> --base main` → 一時ブランチ削除
  (`git push origin --delete <親ブランチ名>`)→ 子を main に rebase して force-push。
- **そもそも積まない**選択肢も有効: 各マイルストーンPRを main ベースにして、前のPRが
  マージされ次第 `git rebase origin/main` で次を追従させる(依存が浅ければこちらが単純)。

## このスキルが守る不変条件

- **検証なしでコミットしない**(rust-test + clippy + TS版突き合わせ)。
- **PR作成までは確認不要で自動**、**mergeは明示指示まで待つ** ([[pr-flow-autonomy]])。
- **レビュー無しで push しない・マージしない**(2つのフックが拒否する。回避・偽装しない)。
  記録(`scripts/record-review.sh`)は「レビューを回した」という事実を残すためのもので、
  レビューを回さずに記録だけ作るのは偽装にあたる。
