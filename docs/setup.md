# 開発環境のセットアップ

> 環境構築の**一次情報源**。他のドキュメント(README.md / docs/handoff.md)は
> 手順を重複させず、ここへ案内すること。

## 1. ツールチェーン(mise)

ツールのバージョンは [mise](https://mise.jdx.dev/) で固定しています(`mise.toml` 参照)。

```sh
mise install     # bun / node / rust が mise.toml のバージョンで入る
```

mise 自体が入っていない場合は `curl -fsSL https://mise.run | sh` で `~/.local/bin/mise` に入ります。
`eval "$(mise activate bash)"` を `~/.bashrc` に書いておけば、以降 PATH を手で通す必要はありません。

## 2. system パッケージ

mise で管理できないものだけを OS のパッケージマネージャで入れます。

| パッケージ | 必要な理由 | 無いとどうなるか |
|---|---|---|
| `gcc`(または clang 等の C コンパイラ) | Rust のリンカが `cc` を呼ぶ | `cargo build` が ``error: linker `cc` not found`` で失敗 |
| `jq` | `.claude/hooks/*.sh` が入力 JSON の解析に使う | フックが無言で素通りし、レビュー必須の強制が効かなくなる(squashの強制はリポジトリ設定側でサーバ側にかかるため無関係) |
| `gh` | PR の作成・CI 確認・マージ | 開発フロー(下記)が回せない。mise でも入る(`mise use -g gh@latest`) |

Ubuntu なら `sudo apt-get install -y gcc jq` です。

## 3. 依存パッケージ

```sh
bun install      # typescript など(bunx tsc --noEmit に必要)
```

## 4. GitHub 認証

```sh
gh auth login    # 対話式。HTTPS + Git 認証も gh に任せるのが楽
```

必要なスコープは `repo` `workflow` `read:org` です(`workflow` が無いと
`.github/workflows/` を含む PR を push できません)。

## 5. `/code-review` プラグイン

開発フローは PR ごとの `/code-review` を必須にしており、`.claude/hooks/enforce-code-review.sh` が
`### Code review` 見出しのコメントが無い状態でのマージを拒否します。

**注意**: `.claude/settings.json` の `enabledPlugins` は
`code-review@claude-code-plugins` という**名前を有効化するだけ**で、プラグインの実体は
`~/.claude/plugins/` 配下(git 管理外・マシンごと)にあります。新しいマシンでは
Claude Code の `/plugin` から取得してください。

これを忘れると「フックはマージを拒否するが、レビューを投稿する手段が無い」という
詰みになります。

## 6. 動作確認

```sh
mise run test           # bun test
mise run check          # bunx tsc --noEmit
mise run rust-test      # cd rust && cargo test
mise run rust-check     # cd rust && cargo clippy --all-targets
mise run run-examples   # examples/*.mesh を全部実行
```

## 環境ごとの注意

### メモリの小さいマシン(2GB 未満)

`cargo` の並列ビルドがメモリを食い切ることがあります。`CARGO_BUILD_JOBS=1` を付けるか、
`~/.cargo/config.toml` に `[build] jobs = 1` を書いてください。rustup が
「low memory のため single-threaded 展開」と警告することがありますが、これは実害ありません。

参考: AWS Lightsail の 512MB インスタンス(2 vCPU / swap 2GB)で
`bun test` と `cargo test` が両方通ることを確認済みです(2026-07-21)。

### 作業ディレクトリ

固定していません。過去の記録には `/Users/kanayama/kanaami/language`(Mac)や
`/home/ubuntu/development/mesh`(Lightsail)が登場しますが、どこに clone しても構いません。
