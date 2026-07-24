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

### 3. featureブランチ → commit → push → PR作成

デフォルトブランチ(main)にいるなら **必ず先にブランチを切る**。

```sh
git checkout -b <topic-branch>
git status --short && git diff --stat   # ステージ前に確認(意図しないファイルを混ぜない)
git add -A
git commit -F <message-file>   # 決定の経緯・却下した代替案もメッセージに書く
git push -u origin <topic-branch>
gh pr create --base main --title "..." --body "..."
```

`git add -A` は自動フローでも**必ず直前に `git status`/`git diff --stat` を目視**してから
使う——無関係なローカル生成物・別作業の変更を巻き込まないため(CLAUDE.md「無関係な変更は
別コミットに分ける」)。

コミットメッセージ末尾には環境指定のトレーラ(`Co-Authored-By:` / `Claude-Session:`)、
PR本文末尾には `🤖 Generated with [Claude Code]...` を付ける(ハーネス既定に従う)。

**無関係な変更は同じPRに混ぜない**(例: スキル追加とマイルストーン実装は別PR)。

### 4. CI と code review(mergeの前提)

PR作成後、次の2つを **並行** で(順不同):

```sh
gh pr checks <番号> --watch   # CI green を確認
```

コードレビューを回す。**`code-review` プラグインが入っていて `code-review:code-review`
スキルとしてモデルから起動できる環境なら、Claude自身がSkillツールで実行できる**
(2026-07-24に`/plugin`で導入して確認済み)。プラグインが無い環境の組み込み `/code-review`
スラッシュコマンドは `disable-model-invocation` でユーザーしか起動できないので、その場合は
ユーザーに `/code-review <番号> --comment` の実行を依頼する。いずれにせよ結果は
`### Code review` 見出しのコメントとしてPRに投稿される。
`.claude/hooks/enforce-code-review.sh` が、そのコメントが無い状態での `gh pr merge` を
機械的に拒否する。レビュー指摘(80点以上)があれば対応コミットを足し、CIとレビューをやり直す。

### 5. merge(=ここでいったん止まる)

**自動でやらない。** CI green + code review コメントが揃っていても、kanayamaさんが
「マージして」と明示するまで待つ。指示が出たら:

```sh
gh pr merge <番号> --squash --delete-branch
git checkout main && git fetch --prune origin && git merge --ff-only origin/main
```

squash merge のみ(リポジトリ設定でmerge commit/rebase mergeは無効)。

## このスキルが守る不変条件

- **検証なしでコミットしない**(rust-test + clippy + TS版突き合わせ)。
- **PR作成までは確認不要で自動**、**mergeは明示指示まで待つ** ([[pr-flow-autonomy]])。
- **レビュー無しでマージしない**(フックが拒否する。回避・偽装しない)。
