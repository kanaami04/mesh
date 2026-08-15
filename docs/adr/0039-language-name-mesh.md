# ADR-0039: 言語名をMeshに変更(ADR-0004のMusubiを撤回)

- **Status**: accepted(ADR-0004 を supersede)
- **Date**: 2026-08-14
- **決定者**: ユーザー(愛着による指名) + Claude

## Context

ADR-0004でMusubiを採用し、仕様1〜7章まで執筆した段階で、ユーザーから「Meshという名前が気に入っているので変えたい」と申し出があった。Meshは同名のTS実装プロジェクト(旧Mesh。本プロジェクトの前身にあたる別言語)で使っていた名前で、GitHubの同名リポジトリが存在していた。

## Decision

**言語名をMeshにする。** CLI: `mesh` / 拡張子: `.mesh` / panic表現: `MeshPanic`。

- 旧リポジトリ `kanaami04/mesh`(TS実装)は**削除**し、本リポジトリ `kanaami04/musubi` を `kanaami04/mesh` にリネームした(ユーザーが実施)。
- ADR-0004は歴史として本文を保持し、Statusのみsupersededにした(追記専用ルール)。

## 検討した代替案と捨てた理由

- **Musubiを維持**: ADR-0004で挙げた「独自性・検索性」の利点は残るが、作者の愛着という判断基準に劣後する。言語名は作者が長期間唱え続けるものであり、納得感が実用上の検索性に優先する。
- **旧リポジトリを残して別名(mesh-lang等)にする**: 名前が濁る。旧TS実装はローカル(`~/kanaami/mesh`)に残っており、設計資産の参照には支障がない。

## Assumptions

- A-17: 「Mesh」は一般語(網・メッシュ)であり、検索性はMusubiより低い。この不利を承知の上で採用した(将来「検索できない」問題が実害化しても、それは既知のトレードオフであり名前変更の理由にはしない)。

## 既存ADRとの相互作用

- ADR-0004: supersede。命名候補10案の検討記録は歴史として残す。
- 全仕様章・言語カード・CLAUDE.md・README: 名称・CLI・拡張子を一括置換済み。
- 旧Mesh(TS実装)への言及は「前実装」「前プロジェクト」と呼び分け、新言語名との衝突を避ける。

## Consequences

- リポジトリURLは https://github.com/kanaami04/mesh(旧musubi URLは自動リダイレクト)。
- 実装フェーズのcrate名は `mesh`、ソース拡張子は `.mesh`(当初は `crates/mesh` に置く想定だったが、2026-08-15にリポジトリ直下の単一crateへ変更。crate名自体は不変)。
- 旧Mesh(TS実装)はGitHubから削除済み。参照が必要な場合はローカルの `~/kanaami/mesh` を使う。
