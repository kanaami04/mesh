# ADR-0004: 言語名はMusubi(結び)

- **Status**: accepted
- **Date**: 2026-08-13
- **決定者**: ユーザー + Claude

## Context

リポジトリ構成・ドキュメント・CLI名を固定するため言語名が必要だった。10候補(Kanaami, Ami, Ori, Tsumugi, Musubi, Weft, Loom, Itomaki, Hata, Uni)をブレストした。

## Decision

**Musubi(結び)**。FEとBEを1言語で「結ぶ」という中核コンセプトを直接表す。

- CLI: `musubi` / 拡張子: `.msb`

## 検討した代替案と捨てた理由

- Kanaami: 物語性はあるが長い。Ori: 短いがゲーム名と衝突。Weft/Loom: 既存プロダクトと衝突。

## Assumptions

- (特になし。名前変更のコストは初期なら小さい)

## Consequences

- GitHubリポジトリ名は musubi 系で取る(公開時に空きを確認)。
