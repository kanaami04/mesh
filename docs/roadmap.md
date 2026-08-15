# ロードマップ

各フェーズに「完了条件」を置く。順番は原則だが、前提が覆ったら(ADR+ASSUMPTIONS.md更新の上で)組み替えてよい。

## Phase 0: 資料整備 ✅ 完了(2026-08-13)

- [x] ビジョン・要件定義(vision.md)
- [x] 学習教材(learning/how-languages-work.md)
- [x] 先行言語調査(research/landscape.md)
- [x] ADR運用・前提台帳の整備(adr/)
- [x] テスト戦略(testing-strategy.md)
- [x] 仕様書の骨子に対するユーザーレビュー(全章をPRレビュー・マージで実施)

## Phase 1: コア言語仕様の決定 ✅ 完了(2026-08-15、drift-check全量検査ゲート通過)

- [x] 型システムの基本方針(設計ADR 30本超で確定。null/エラー=union路線、局所推論、名前的、値意味論など)
- [x] 仕様1〜7章を執筆(字句・型・式・文・関数・エラー処理・モジュール。EARS約150規則+conformanceテストID約200本。各章spec-reviewで敵対的レビュー2〜4ラウンド)
- [x] 言語カードv0.1(docs/language-card.md)

## Phase 2: 最小コンパイラ(Rust)← いまここ

- cargoワークスペース作成、`mesh build file.mesh` でJSを出力
- 字句解析器(手書き)→ 構文解析器(手書き再帰下降)→ AST → JS生成(まず型検査なし)
- スナップショットテスト基盤(insta)+ Node実行E2Eテスト基盤

**完了条件**: Phase 1で決めた範囲の.meshがJSになりNodeで正しく動く。テストがCIとして回る。

## Phase 3: 型検査

- 型注釈+局所型推論、union型、narrowing、matchの網羅性検査
- エラーメッセージの構造化出力(JSON)= AI自己修正ループの土台

**完了条件**: 型エラーの負例テスト群が期待通り落ち、メッセージ品質をレビュー済み。

## Phase 4: データ型とモジュール

- struct+メソッド、判別可能union(タグで種類を見分けられるunion型)、モジュールシステム、JS FFI(MeshからJSの関数やnpmライブラリを呼ぶ仕組み)

## Phase 5: 標準ライブラリ第1弾 + BE基盤

- core(string/list/map)、json(FE/BE型共有を実質化するため検証つきデコードを最初から)、http server

**完了条件**: MeshだけでJSON APIサーバが書ける。

## Phase 6: UI構文とリアクティビティ(FE)

- 組み込みUI構文(JSX風)の仕様策定 → 画面更新方式の決定(全体を仮想DOMで差分更新するか、変化した箇所だけ直接更新するfine-grained方式かは、ここでADRにする)
- ブラウザ向けランタイム(最小)

**完了条件**: カウンターとTODOリストがブラウザで動く。

## Phase 7: フルスタック統合

- FE/BE型共有、開発サーバ(`mesh dev`)、バンドル
- **完了条件**: TODOアプリ(FE+API+永続化)がMeshのみで動く = vision成功指標1

## Phase 8: ツールチェーン

- `mesh fmt`、LSP(エディタ補完・エラー表示)、VSCode拡張

## Phase 9: ドッグフーディングとAI検証

- 白紙AI+言語カード実験で「AIに書きやすい」を計測、仕様へフィードバック
