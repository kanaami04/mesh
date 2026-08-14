---
name: adr
description: 設計上の決定をADRとして記録する。新しい決定・方針転換・技術選定が確定したとき、または /adr で明示的に呼ばれたときに使う。採番・索引・前提台帳・spec修正まで一括で行う。
---

# ADR作成スキル

対象の決定(引数または直前の会話で確定した決定)をADRとして記録する。

手順(docs/adr/README.md の運用ルールに従う):

1. docs/adr/ の既存ファイルを確認し、次の番号を採番する。
2. docs/adr/template.md の形式でADRを書く(Context / Decision / 検討した代替案と捨てた理由 / Assumptions / Consequences)。結論を先頭に、短く、機械が拾いやすい構造で書く。
3. この決定が覆す旧ADRがあれば、旧ADRのStatusを `superseded by ADR-XXXX` に更新する。
4. docs/adr/ASSUMPTIONS.md に新しい前提を追記し、影響を受ける既存前提を更新する。
5. docs/adr/README.md の「現在のADR一覧」に追加する。
6. 影響を受ける docs/spec/・CLAUDE.md の記述を修正する(正文書ルール: ADRが正)。
7. 最後に、決定の要点と更新したファイルの一覧を日本語で報告する。
