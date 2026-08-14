---
name: drift-check
description: ADR・spec・CLAUDE.md間の矛盾(ドキュメントdrift)を検査する。フェーズ完了時、仕様の大きな変更後、または /drift-check で明示的に呼ばれたときに使う。
---

# drift検査スキル

ドキュメントdriftの検査を実行する:

1. docs/adr/ の全ADR(Status: superseded を除く)を読む。
2. docs/spec/・CLAUDE.md・docs/vision.md・docs/roadmap.md を読み比べ、以下を検出する:
   - ADRの決定と矛盾する記述
   - 覆されたはずの決定が残っている箇所
   - ADR化されていない暗黙の決定(文書に「決定済み」の顔で書かれているが根拠ADRが無いもの)
   - docs/adr/ASSUMPTIONS.md の検証状況が現状と食い違っている行
3. 指摘ごとに「場所・矛盾の内容・修正案」を報告する。問題がなければ「driftなし」と報告する。
4. 修正はユーザーの承認を得てから行う(勝手に直さない)。
