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

### A. Rustのビルドが `src/` の3ファイルに依存している

`src/` を消すと **Rust版がコンパイルできなくなる**。単なるオラクル依存ではない。

| ファイル | 参照元 | 何のため |
|---|---|---|
| `src/runtime.ts` | `rust/src/codegen.rs` | 生成JSへ埋め込むランタイム本体。二重管理を避けるため共有 |
| `src/card.ts` | `rust/src/card.rs` | `mesh card` の本文 |
| `src/diagnostic-codes.ts` | `rust/src/explain.rs` | `mesh explain` の説明文 |

いずれも `include_str!` で埋め込んでいる。**撤去前に移送先を決める必要がある**。
本文をRustへ複製すると `tests/card-completeness.test.ts`(card.tsの主張と実装の乖離を
CIで検出している唯一の仕組み)の検証から外れる点に注意。

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

### 段階2: `include_str!` 3ファイルの移送先を決める(ブロッカーA)

**撤去そのものより先に片付ける**。ここが決まらないと撤去の見通しが立たない。
選択肢と、それぞれで失うもの:

1. **`shared/` のような実装非依存のディレクトリへ移す** — TS/Rust両方から参照できる。
   `runtime.ts` は「JSソースであること」に意味があるので移すだけで済む。
   `card.ts`/`diagnostic-codes.ts` はTS構文のデータ定義なので、
   移送先でもパーサ(Rust側の既存の抜き出しロジック)がそのまま使える
2. **Rustへ本文を複製する** — `card-completeness.test.ts` の検証から外れる。**非推奨**
3. **データをTS構文から別形式(TOML/JSON)へ移す** — 一番きれいだが、
   `card-completeness.test.ts` と `card-subset.test.ts` の書き換えが要る

### 段階3: TSのテスト資産の移植(条件3)

481テスト・6146行。**全部移植する必要は無い**——Rust側は既に629テストあり、
重複が多い。先に「Rustに無い観点」を機械的に洗い出してから決める。

固有の価値がはっきりしているのは `tests/card-completeness.test.ts`(4テスト)
——`src/card.ts` の主張と実装の乖離を検出する唯一の仕組みで、段階2の移送先次第で形が変わる。

### 段階4: playgroundのwasm化(条件4)— **一番重い**

`rust/` に `wasm32-unknown-unknown` ターゲットと `wasm-bindgen` を足し、
`playground/main.ts` の import を差し替える。ここだけ独立して先行できる。

### 段階5: 撤去の実行

1. `bench/README.md`・`demo/todo-api/README.md` の手順をRust版へ差し替え
2. `tests/parity/*/expected.txt` を「撤去時点の正解」として凍結し、
   `scripts/parity.sh` をスナップショット比較専用へ縮退(TS呼び出しを落とす)
3. CIから `bun test` / `tsc --noEmit` を外す
4. `src/` と `tests/*.test.ts` を削除

**この順序は動かさない**。特に2を先にやらないと、撤去した瞬間に回帰検出手段が消える。

## 判断の原則

- **「Rust版が本番として十分」と「オラクルとしての役目も終わった」の両方が揃うまで消さない**
  (2026-07-25にkanayamaと整理した方針をそのまま踏襲)
- 段階1〜4は**どれも撤去を伴わない**ので、途中で止めても損失が無い
- 段階5だけが不可逆。ここは明示的な合意を取ってから実行する
