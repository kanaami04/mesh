# TS実装(`src/`)の撤去計画

2026-07-27 に整理。**撤去条件(1)(2)を満たした時点**で作った、ここから先の段取り。
条件そのものの定義と経緯は `todo.md`「TS実装(`src/`)の撤去条件」が一次情報源で、
このファイルは**残りの作業をどの順で・何を確かめながらやるか**だけを書く。

## 前提: いま何が終わっていて、何が残っているか

| 条件 | 状態 |
|---|---|
| (1) Rust CLIが全サブコマンドを持ち、full_checkerがrun/buildのゲートに入る | ✅ milestone 35〜38 |
| (2) 診断カバレッジ | ✅ milestone 62〜65(107/107・parity検出漏れ0・誤検知0) |
| (3) TSのテスト資産をRust側へ移植 | ⬜ 481テスト・6146行が残っている |
| (4) `playground/`と`editors/vscode`の移行 | ⬜ playgroundはwasmビルドが要る(**一番重い**) |
| (5) CIで両実装を並走させ差分ゼロを確認 | 🔶 段階1で着手 |

生成JSも examples 全24本でバイト一致し、`mise run parity` は156ファイルで
検出漏れ0・誤検知0・生成JS差0。**言語実装としてはRust版で足りている。**

## 撤去を阻む本当のブロッカーは3つ

### A. ~~Rustのビルドが `src/` の3ファイルに依存している~~ ✅ 段階2で解消

`src/` を消すと **Rust版がコンパイルできなくなる**。単なるオラクル依存ではない。

| ファイル | 参照元 | 何のため |
|---|---|---|
| `src/runtime.ts` | `rust/src/codegen.rs` | 生成JSへ埋め込むランタイム本体。二重管理を避けるため共有 |
| `src/card.ts` | `rust/src/card.rs` | `mesh card` の本文 |
| `src/diagnostic-codes.ts` | `rust/src/explain.rs` | `mesh explain` の説明文 |

いずれも `include_str!` で埋め込んでいた。**段階2で `rust/embedded/` へ複製し、
Rust側を自己完結させた**(TS版を完全に廃止する方針のため、`shared/`のような中間形態は置かない)。

複製で失うはずだったものは2つの番人で埋めてある:

- `rust/tests/embedded_sync.rs` — **複製が原本とバイト一致するかを強制する**。
  複製は放っておけば必ず腐るため。原本(`src/`)が消えたら自動的にskipし、
  そのときこのファイルごと消してよい旨をコメントに書いてある
- `rust/tests/card_completeness.rs` — `tests/card-completeness.test.ts` の移植。
  **TS版はTS実装のリストを見ていたが、こちらはRust実装の`BUILTINS`/`RESERVED`を見る**
  ——TS撤去後もカード完全性の検査が残る。「節が取れずに全部通る」空振りを検出するテストも付けた

**`src/`を丸ごと消した状態で`cargo build`と全テストが通ることをworktreeで実測済み**
(`mesh card`/`mesh explain`/`mesh run`の動作確認込み)。

### B. TS版は「移植の検証装置(オラクル)」でもある

`mise run parity` / `mise run sweep` はどちらもTS版を正解として動く。
TS版を消すと**この2つが動かなくなる**——回帰を検出する主力を失う。

対策の方向は「消す前に、TS版に依存しない形で同じ検証をできるようにする」:
`tests/parity/*/expected.txt` のスナップショットは既にRust単独で回せる
(`rust/tests/parity.rs`)ので、**スナップショットを撤去時点の正解として凍結する**のが素直。

### C. playgroundがブラウザで `src/compiler.ts` を直接importしている

`playground/main.ts` が `import { compile } from "../src/compiler"`。
Rust版でこれをやるにはwasmビルドが要る(現状 `rust/` にwasm対応は無い)。

なお `editors/vscode` は **TextMate文法とエディタ設定だけ**でTS実装に依存していない
——移行対象として挙がっていたが、実際にはブロッカーではない(実測で確認)。
`bench/README.md` は手順書の中で `bun src/cli.ts` を案内しているだけ、
`demo/todo-api` は両実装の実行手順を併記しているだけで、どちらも差し替えるだけで済む。

## 段取り

### 段階1: CIで両実装を並走させる(条件5)— **本PRで実施**

いまのCIは `bun test` と `cargo test` を**別々に**走らせるだけで、
**TS版とRust版を突き合わせていない**。`mise run parity` / `mise run sweep` は
手元でしか回っておらず、「並走で差分ゼロ」を機械的に保証できていない。

`parity` 26秒 + `sweep` 10秒(実測)なのでCIに十分載る。両方が要るjobとして追加する。

### 段階2: `include_str!` 3ファイルをRust側へ複製(ブロッカーA)— ✅ 完了

kanayamaの判断で**選択肢2(Rustへ複製)**を採った——TS版を完全に廃止したいので、
`shared/`のような「TS構文のファイルが残り続ける」中間形態は目的に合わない。
複製で失う検証は上記2つの番人で埋めた。

**これで `src/` はRustのビルドに要らなくなった**。残る依存はオラクル(parity/sweep)としてだけ。

### 段階3: TSのテスト資産の移植(条件3)— ✅ 完了

481テスト・6146行あるが、**領域別に数えるとRust側の方が多い**ものがほとんどだった:

| 領域 | TS | Rust | 判断 |
|---|---|---|---|
| checker | 202 | 309(`full_checker` 211 + `checker` 98) | 移植不要 |
| e2e | 177 | 184(`codegen` 157 + `cli` 27) | 移植不要 |
| parser | 45 | 77 | 移植不要 |
| lexer | 15 | 15 | 移植不要 |
| card-subset | 16 | 7 | **数は少ないが同等**——Rust側は検出条件を1テストへ集約している |
| formatter | 16 | 13 | **コーパス全体の性質テストだけ無かった** → 移植 |
| http | 6 | **0** | **実行時の検証が丸ごと無かった** → 移植 |
| card-completeness | 4 | 3 | 段階2で移植済み |

**数の差より「観点が丸ごと無い」ものを探すのが本質**だった。見つかったのは2つ:

- `rust/tests/http.rs` — `mesh/http` の実行時テスト6件。それまでRust側は
  `tests/parity/56-http-listen-signature/`(`mesh check`を見るだけ)しか無く、
  **サーバーが実際に動くかを一度も確かめていなかった**。
  HTTPクライアントは`std::net`で素書きしてある(この移植は依存クレート0で来ており、
  テストのためだけに依存を増やしたくないため)
- `rust/tests/fmt_corpus.rs` — `mesh fmt` がexamples全体で**べき等**かつ**意味を変えない**か。
  個別の整形規則のテストは13件あったが、実プログラムを通す性質テストが無かった。
  意味保存は整形前後を実行して標準出力を比べる(AST比較より信頼できる)

### 段階4: playgroundのwasm化(条件4)— **一番重い**

`rust/` に `wasm32-unknown-unknown` ターゲットと `wasm-bindgen` を足し、
`playground/main.ts` の import を差し替える。ここだけ独立して先行できる。

### 段階5: 撤去の実行

1. `bench/README.md`・`demo/todo-api/README.md` の手順をRust版へ差し替え
2. `tests/parity/*/expected.txt` を「撤去時点の正解」として凍結し、
   `scripts/parity.sh` をスナップショット比較専用へ縮退(TS呼び出しを落とす)
3. CIから `bun test` / `tsc --noEmit` を外す
4. `src/` と `tests/*.test.ts` を削除。**`tests/parity/` は消さない**
   ——TS実装のテストと同じ`tests/`配下に居るが、こちらはRust側のコーパス
   (段階2のworktree検証で、`tests/`ごと消して`corpus_coverage`が落ちて気づいた)
5. `rust/tests/embedded_sync.rs` を削除(原本が無くなり何も守らなくなるため)

**この順序は動かさない**。特に2を先にやらないと、撤去した瞬間に回帰検出手段が消える。

## 判断の原則

- **「Rust版が本番として十分」と「オラクルとしての役目も終わった」の両方が揃うまで消さない**
  (2026-07-25にkanayamaと整理した方針をそのまま踏襲)
- 段階1〜4は**どれも撤去を伴わない**ので、途中で止めても損失が無い
- 段階5だけが不可逆。ここは明示的な合意を取ってから実行する
