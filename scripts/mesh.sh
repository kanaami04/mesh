#!/usr/bin/env bash
# `mesh` CLIの入口(TS撤去 段階5)。以前は `bun src/cli.ts` だったが、TS実装の撤去に伴い
# Rust版のバイナリを呼ぶ。**無ければビルドする**——`bun run mesh` を叩いた人が
# 「バイナリが無い」で止まらないように(初回だけ数十秒かかる)。
set -euo pipefail
cd "$(dirname "$0")/.."

BIN=rust/target/debug/mesh
if [ ! -x "$BIN" ]; then
	echo "mesh: Rust版CLIをビルドする(初回のみ)..." >&2
	cargo build --manifest-path rust/Cargo.toml >&2
fi
exec "$BIN" "$@"
