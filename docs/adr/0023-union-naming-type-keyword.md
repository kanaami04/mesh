# ADR-0023: unionの命名は `type Name = A | B`(enumキーワードは置かない)

- **Status**: accepted
- **Date**: 2026-08-14
- **決定者**: ユーザー + Claude

## Context

Phase 1ラウンド5のQ19。インラインunion(`User | none`)は決定済み(ADR-0005)で、unionに名前を付ける宣言の形を決める必要があった。

## Decision

```
type Shape = Circle | Square | Triangle
```

- `struct` は形(フィールドの集まり)の定義、`type` はunion・別名の命名、と役割を分離する。
- インライン記法と字面が一致する(`=` の右がそのまま型式)。

## 検討した代替案と捨てた理由

- **enumキーワード(Rust風)**: バリアントを囲う宣言。まとまりはあるが宣言種が増え、union記法と意味が被る。
- **両方置く**: 同じことの書き方が2つになる。

## Assumptions

- (特になし)

## Consequences

- メンバーの実行時判別はA-7(型タグ)の設計に依存する。タグ無し名前付きstruct同士のunionが静かに誤った前例があるため、`type` unionのmatchはconformanceテストの最重点にする。
- ジェネリクスと組み合わせ可(`type Result<T> = T | error` のような別名も書ける。ADR-0020)。
