# git-worktree

「別のコミット/ブランチの状態を、いまの作業ツリーを壊さずに用意する」ための手順書。
**作ったら必ず消す**までがこのスキルの範囲。

## いつ使うか

- **変更前後の比較**: 「この誤検知はこのPRが入れたのか、前からあるのか」を確かめる。
  Rust移植では `mesh check` の出力をTS版と突き合わせるので、比較用に *別コミットの
  バイナリ* が要ることが頻繁にある。
- 別ブランチのCI失敗を手元で再現する。
- 並行して動くエージェントに、互いのファイルを踏ませずに作業させる。

## 使ってはいけない代替手段

- **`git stash` で「変更を一時的に外す」のは禁止**(2026-07-26に事故): 作業ツリーが
  既にクリーンだと `git stash` は**何も保存せず**、続く `git stash pop` が
  **スタックに元からあった無関係なstash**を展開して競合を起こす。実際に
  `full_checker.rs` が `UU` になった。そもそも比較したいのが「別のコミット」なら、
  未コミット変更を退避する stash では目的を達成できない。
- `git checkout <他のコミット>` も駄目——作業ツリーそのものが動くので、
  ビルド生成物・エディタ・並行して動いているエージェントを巻き込む。

## 手順

```sh
# 1. 作る（**リポジトリの外**に置く。中に作るとcargo/gitの無視設定と干渉する）
git worktree add /tmp/mesh-base <commit-or-branch>

# 2. 使う（例: 比較用バイナリを作って両者の出力を突き合わせる）
(cd /tmp/mesh-base/rust && cargo build)
/tmp/mesh-base/rust/target/debug/mesh check foo.mesh > /tmp/before.txt
rust/target/debug/mesh check foo.mesh > /tmp/after.txt
diff /tmp/before.txt /tmp/after.txt

# 3. **必ず消す**（このスキルの本題。消し忘れが一番起きやすい）
git worktree remove /tmp/mesh-base          # 未コミット変更があると拒否される
git worktree remove --force /tmp/mesh-base  # それでも消すとき

# 4. 消えたことを確認する
git worktree list   # 出力が本体のリポジトリ1行だけになっていること
```

## 後片付けの注意

- **`rm -rf` で消さない**。git側に管理情報が残り、次に同じパスを使おうとしたときに
  `already exists` で失敗する。消してしまった場合は `git worktree prune` で解消する。
- worktreeを消してもブランチは残る。比較用に `-b` で新しいブランチを作った場合は
  `git branch -D <名前>` も要る(`git worktree add /tmp/x <既存コミット>` のように
  detached HEAD で作れば、そもそもブランチは増えない——**比較用途ではこちらが既定**)。
- **作業の途中で失敗しても消す**。「あとで消す」は忘れる。作ったコマンドと消すコマンドを
  同じ手順の中に書いておくこと。

## cargoと併用するときのコツ

worktreeは `target/` を共有しないので、比較用ビルドはフルビルドになる(Meshの
Rust実装で数十秒)。`CARGO_TARGET_DIR` を共有すると速くなるが、**本体と同時にビルドすると
ロック待ちで直列化する**ので、比較のように短時間なら共有しない方が素直。

## このスキルが守る不変条件

- 作業ツリーを壊さずに別状態を用意する(`git stash`/`git checkout` で代用しない)。
- **作ったworktreeは必ず消す**。作業の最後に `git worktree list` で確認する。
