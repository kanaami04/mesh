# 先行言語・競合調査(2026-08時点)

Meshの立ち位置を定めるための調査。結論: **「FE+BE両対応」の実現方式は業界的に「JSへコンパイル」か「WASMへコンパイル」の実質二択**であり、各言語はその路線上で差別化している。

## MoonBit — 最も近い競合

- 中国発の「AIネイティブ」言語。wasm / wasm-gc / **JS** / native の4バックエンド。2026年に1.0予定のベータ段階。
- WASM第一設計で、出力サイズ・速度は既存言語より優秀。JSバックエンドも高速を主張。
- Rust風の構文+GC付き。ツールチェーン(moon CLI, IDE)一体型。
- **Meshとの差別化ポイント**: MoonBitは「クラウド・エッジ計算向け+汎用」でありUIは主戦場ではない。Meshは**UI構文を言語に組み込み、フルスタックWebアプリに全振り**する。また日本語話者の作者による、読みやすさ最優先の設計。
- ユーザー所感: 近いが想像しているものではない(→どこが違うかをPhase 1で言語化するとMeshの輪郭が明確になる)。

## Topcoat — Rustフルスタックの新星(2026年7月登場)

- Tokioチーム製のRustフルスタックWebフレームワーク。0.1.0が2026-07-16、公式発表2026-07-22。
- **注目すべき設計**: WASMを使わず、**型チェック済みRust式をJavaScriptに変換**してブラウザ側の状態更新を行う「No-WASM」方式。コンポーネントがサーバ側でHTMLを組み立て、DBも直接読めるため、FEとBEの間のAPI層に毎回書く定型コード(ボイラープレート)が不要になる。Laravel/Rails/Next.js的な体験をRustで、という思想。
- **Meshへの示唆**: ①Tokioチームですら「ブラウザ=WASMよりJS変換」を選んだ(MeshのJSターゲット決定を裏付ける)。②「FE/BE境界のAPIボイラープレート排除」はMeshのフルスタック統合(Phase 7)の重要な参考例。③ただしTopcoatは既存言語Rustの上のフレームワークであり、言語ごと設計できるMeshの方が構文の自由度は高い。

## Gleam — 複数ターゲットの先行成功例

- 静的型付け言語。**Erlang と JavaScript の両方にコンパイル**し、BE(BEAM)とFE(JS)を1言語で書ける。
- JSターゲットでは追加ランタイムをほぼ載せず、生成コードはJS/TSから普通に呼べる。ターゲットごとに並行モデルを変える(BEAM=アクター、JS=Promise)という現実的な割り切り。
- **Meshへの示唆**: 「1言語・複数ターゲット・FE/BEコード共有」が実際に成立する証拠。またターゲット間で無理に意味論を統一しない割り切り方が参考になる。

## その他の参考(いいとこ取りの元ネタ)

- **TypeScript**: 型システムの表現力とJSトランスパイルの成功例。ただし`any`と歴史的経緯が「抜け穴」。ソースマップ・エディタ体験は手本。
- **Elm**: FE専用だが「親切なエラーメッセージ」の金字塔。MeshのエラーメッセージのUX目標。ただしFE専用に閉じたことがエコシステム的な限界にもなった(→BE軸を持つMeshの逆張り根拠)。
- **Dart/Flutter**: 「言語+UIフレームワーク一体開発」の成功例。UIを言語と共進化させる価値の証拠。
- **ReScript**: JSターゲットのML系。生成JSの読みやすさを売りにしている(生成コードが読める=デバッグしやすい、はMeshも重視したい)。
- **Kotlin**: マルチターゲット(JVM/JS/Native)だがJS側は主流になれなかった。「後付けターゲット」の難しさの教訓。

## Sources

- https://tokio.rs/blog/2026-07-22-announcing-topcoat
- https://github.com/tokio-rs/topcoat
- https://xenospectrum.com/en/topcoat-rust-fullstack/
- https://docs.moonbitlang.com/en/stable/language/
- https://thenewstack.io/moonbit-wasm-optimized-language-creates-less-code-than-rust/
- https://gleam.run/news/v0.16-gleam-compiles-to-javascript/
- https://gleam.run/
