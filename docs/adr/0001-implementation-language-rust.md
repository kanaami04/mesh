# ADR-0001: コンパイラの実装言語をRustにする

- **Status**: accepted
- **Date**: 2026-08-13
- **決定者**: ユーザー + Claude

## Context

コンパイラ・ツールチェーン一式の実装言語を決める必要があった。ユーザーの明確な希望としてRustが指定されている。

## Decision

コンパイラ・ツールチェーン一式を**Rust**で実装する。

## 検討した代替案と捨てた理由

- **TypeScript**: 試行錯誤は速いが、コンパイル速度・配布(単一バイナリ)・LSPの応答性でRustが勝る。
- **Go**: 配布とシンプルさは良いが、enum+matchがなくAST処理(コンパイラの中核作業)の快適さでRustに劣る。

## Assumptions

- A-3: Rust実装をClaude主導で保守・拡張し続けられる(ユーザーはコードを書かない前提)。

## Consequences

- cargoワークスペース、insta(スナップショット)、tower-lsp(LSP)等のRustエコシステムを利用する。
