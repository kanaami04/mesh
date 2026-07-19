# Task 04: Expression evaluator (tree)

> 機能軸: 木構造(自己参照する型)・分岐網羅・再帰(eval と render の2関数)。
> Meshは判別可能union+match+narrowing(F-6/自己参照unionの効果測定)、
> TSはタグ付きunion、Goはinterface or タグ付きstructとの対比になる。
> パーサは書かせない(木を直接構築)— 測りたいのは木構造の型設計と再帰処理のみ。

---

Write a complete, runnable program in {LANGUAGE}.

## Requirements

- Define a data type for arithmetic expressions with exactly these four forms:
  - **Num**: an integer literal (value)
  - **Add**: addition (left, right)
  - **Mul**: multiplication (left, right)
  - **Neg**: negation (operand)
- Implement two recursive functions:
  - **eval(expr)** → the integer value of the expression
  - **render(expr)** → a fully parenthesized string:
    - Num: just the number, e.g. `2`
    - Add: `(<left> + <right>)`
    - Mul: `(<left> * <right>)`
    - Neg: `(-<operand>)`
- In the entry point, construct this expression tree directly (no parsing):

  `Mul( Add( Num 2, Mul( Num 3, Num 4 ) ), Neg( Add( Num 1, Num 1 ) ) )`

  then print the rendered form on one line, and `= <evaluated value>` on the next.

## Expected stdout (byte-exact)

```
((2 + (3 * 4)) * (-(1 + 1)))
= -28
```

## Constraints

- One single file. No external dependencies, no third-party imports (standard library only).
- No user input; the tree above is hardcoded.
