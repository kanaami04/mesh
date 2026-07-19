# Task 01: TODO manager (CLI)

> 機能軸: struct・配列・変更操作・文字列比較・反復。カード実験第1〜4回と同系の題材。
> 以下の英語仕様を(言語名だけ差し替えて)そのまま被験体に渡す。

---

Write a complete, runnable program in {LANGUAGE}.

## Requirements

- Model a todo item with a title (string) and a done flag (boolean).
- Implement three operations:
  - **add(title)**: append a new todo (not done) to the list
  - **complete(title)**: mark the FIRST todo whose title matches as done.
    If no todo matches, print exactly: `no such todo: <title>`
  - **render**: print each todo on its own line, in insertion order:
    `[x] <title>` if done, `[ ] <title>` if not
- In the program entry point, perform exactly this scenario:
  1. add "buy milk"
  2. add "write report"
  3. add "call bob"
  4. complete "write report"
  5. complete "pay rent"
  6. render the list

## Expected stdout (byte-exact)

```
no such todo: pay rent
[ ] buy milk
[x] write report
[ ] call bob
```

## Constraints

- One single file. No external dependencies, no third-party imports.
- No user input; the scenario above is hardcoded.
