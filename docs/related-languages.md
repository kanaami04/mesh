# 類似言語調査

> Meshとコンセプトが近い言語の調査記録。設計討議の材料にする。
> 討議アジェンダへの反映は [design-agenda.md](design-agenda.md) の E 節。

## MoonBit(調査: 2026-07-18)

**AIネイティブを掲げる、マルチバックエンド(Wasm/JS/ネイティブ)の静的型付き言語。**
Meshの中核コンセプト①(AIが書きやすい)③(1言語でフロント/バック)とほぼ正面から被るが、
系譜(Rust/OCaml系・関数型寄り)とAIへの攻め方(重量級ツールチェーン統合)が対照的。

### 概要と現在地

- 開発元は深圳のIDEA研究院。リーダーは Hongbo Zhang(ReScript / OCaml / Flow のコア開発者)。
  2022年開始、2023年公開
- 2025年6月ベータ到達。2026年7月時点で **v0.10.0**、**1.0は2026年Q3目標**
- ターゲットは **Wasm(特にWasm GC)が第一級**。JSとネイティブ(LLVM)も出力可。
  JS出力はデッドコード除去が強く「npmライブラリとして現実的なサイズ」と評価されている
- ツールチェーンは `moon` コマンドに統合: check / fmt / test / bench / パッケージ管理(mooncakes.io)。
  **インラインスナップショットテストが言語レベルで組み込み**

### 「AIネイティブ」の中身(Meshとの最大の比較ポイント)

主張は「Pythonは人間との対話のために設計されたが、MoonBitはAIとの対話のために設計する」。
具体策は3層:

1. **言語設計**: トップレベル定義は**型注釈必須**、フラットな(ネストの浅い)構造を推奨。
   LLMが線形にコードを生成しやすく、KVキャッシュ効率も良いという理屈。
   interface は構造的実装(Goと同じ発想)でネストを排除
2. **デコーディング統合**: LLMのトークン生成時に構文の正しさを検証するローカルサンプリング+
   型エラーを検出するグローバルサンプリングを組み合わせ、不正トークンをバックトラックして再生成。
   約3%の速度低下でコンパイル成功率を大幅改善と報告
3. **ツールチェーン**: IDE組み込みAIアシスタント、静的解析で生成コードを検証

**Meshとの対比**: Meshの言語カード(圧縮仕様をコンテキストに入れて往復回数を測る実験)は
**モデル側に手を入れずプロンプトだけで解決する**軽量アプローチ。MoonBitの2は自前でLLM推論
インフラを持つ組織にしかできない重量級の策で、ここはMeshの独自性。一方で1の
「トップレベル型注釈必須・フラット構造」はMeshの既決定(全関数の型注釈必須)と同方向。

### 言語機能の比較

| 論点 | MoonBit | Mesh |
|---|---|---|
| 系譜・書き味 | Rust/OCaml風、関数型寄り、式指向(if/match/forが式) | TS+Go、手続き寄り |
| 不在の表現 | `Option[T]`(ADT) | `T \| none`(union) |
| エラー | `suberror` 宣言 + `raise`/`try`/`catch`/`noraise`。`Result` 型にも変換可、エラー多相 `raise?` | `T \| error` union + `!`/`or` |
| パターンマッチ | match式・網羅性検査・ADT。1.0で正規表現パターン予定 | match式・網羅性検査(実装済み) |
| 並行処理 | **構造化並行**(task group内の `spawn_bg` のみ、**channelなし**、キャンセル組み込み)。1.0でasyncランタイム正式化 | Go風 `spawn`/`wait`/channel |
| メモリ | GC(Wasm GC活用)。1.0で値型オプトイン予定 | JSランタイム任せ |
| defer | 1.0で導入予定 | TODO記載中 |

エラーは関数シグネチャに `raise DivError` と書くチェック例外風の設計。Javaのそれと違い
型推論とエラー多相で書き味を軽くしている。

並行処理は「新しいタスクは task group を通じてのみ生成でき、group は全タスク終了後にのみ
返る」という構造化並行。孤児タスク(goroutine泄漏)が設計上発生せず、キャンセルは
「ブロック地点でエラーとして現れる」ため明示的処理が不要。

### Meshにとっての示唆

1. **`mesh check --json` の方向性は正しい** — MoonBitも「AIが消費するツールチェーン」を核に
   据えており、同じ発想
2. **フォーマッタとスナップショットテストの優先度を上げる価値あり** — `moon fmt`(設定なし)と
   組み込みスナップショットテストは「AIが書いた差分をレビューしやすくする」効果が大きい。
   todo の `mesh fmt` と合致
3. **channelなし構造化並行という対抗案** — `select` 実装やチャネル容量指定の前に
   「そもそもchannelを露出すべきか」を一度討議する価値がある([design-agenda.md](design-agenda.md) E-1)
4. **差別化の軸**: MoonBitは「大組織による重量級AIツールチェーン統合」、Meshは
   「仕様の小ささ+言語カードでプロンプトだけで戦う」。言語カード実験(往復回数の計測)は
   MoonBitがやっていない定量アプローチ

### 出典

- [MoonBit公式](https://www.moonbitlang.com/) / [1.0ロードマップ](https://www.moonbitlang.com/blog/roadmap) / [v0.10.0リリース](https://www.moonbitlang.com/updates/2026/06/08/moonbit-0-10-0-release)
- [AI-Nativeツールチェーン設計ブログ](https://www.moonbitlang.com/blog/moonbit-ai) / [LLM時代の言語の未来](https://www.moonbitlang.com/blog/ai-coding) / [ACMワークショップ論文](https://dl.acm.org/doi/10.1145/3643795.3648376)
- [エラーハンドリング公式ドキュメント](https://docs.moonbitlang.com/en/latest/language/error-handling.html) / [async(実験的)ドキュメント](https://docs.moonbitlang.com/en/latest/language/async-experimental.html)
- [mizchi氏によるJS開発者視点レビュー](https://dev.to/mizchi/moonbit-a-modern-language-for-webassemblyjsnative-4p71)
- [Hongbo Zhang氏インタビュー](http://pldb.info/blog/hongboZhang) / [ベータ発表](https://www.moonbitlang.com/blog/beta-release)
