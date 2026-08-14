# ADR-0012: struct宣言+レシーバ付きfnメソッド(classは置かない)

- **Status**: accepted
- **Date**: 2026-08-13
- **決定者**: ユーザー + Claude

## Context

Phase 1ラウンド3のQ9。データ型の宣言と、それに紐づく振る舞い(メソッド)の記法を決める必要があった。FE/BE両対応はどの案でも成立する(structは実行環境に依存しない)ことを確認済み。

## Decision

```
struct User {
  name: string
  age:  int
}

fn (u: User) greet() string {
  return "こんにちは、" + u.name
}

User{name: "alice", age: 30}.greet()   // 呼び出しはドット記法
```

- データ(`struct`)と振る舞い(レシーバ付き `fn`)を分離する(Go流)。
- 生成はstructリテラル `User{field: 値}`。
- **class・継承は言語に置かない。**

## 検討した代替案と捨てた理由

- **メソッドをstruct内に書く(TS class風)**: 1箇所にまとまるが、データ定義とロジックが混ざり、関数の書き方が2種類になる。
- **implブロック(Rust流)**: 宣言種が1つ増え、シンプルさ方針と合わない。

## Assumptions

- (特になし。Goと前実装で実証済みの形)

## Consequences

- 関数の書き方が「`fn`(レシーバ有無の差だけ)」の1本に統一される。
- FE/BEで同じstruct定義をimportして共有できる(型共有の実体)。
- FEの画面部品は `component`(ADR-0003)が担い、structと役割が混ざらない。
