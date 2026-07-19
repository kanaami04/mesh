# Task 03: Concurrent fetch + aggregate

> 機能軸: 並行起動・全完了待ち・結果の順序回復・並行下のエラー処理。
> Meshは spawn/`<-task`/構造化並行、Goは goroutine+channel/WaitGroup、TSは Promise.all系。
> スリープ時間を「idが小さいほど長い」逆順にしてあり、完了順で印字する誤実装は
> 期待出力と一致しない(順序回復が強制される)。
> 決定性の担保: 印字は全完了後に**リクエスト順**で行わせる。
> 並行性の担保: 逐次実行なら合計150msのスリープが並行なら約50ms —
> ハーネスは実行壁時間 <120ms を並行実行の判定に使う(逐次実装は「並行でない」として
> expected/got差分と同様にフィードバックする)。

---

Write a complete, runnable program in {LANGUAGE}.

## Requirements

- Implement `fetchData(id)`, which simulates fetching from a server:
  - It sleeps for `(6 - id) * 10` milliseconds (so id 1 sleeps 50ms, id 5 sleeps 10ms).
  - For id 3, it FAILS with the message `server error` (use your language's idiomatic
    failure mechanism).
  - For any other id, it returns the number `id * 100`.
- In the entry point:
  - Start fetches for ids 1, 2, 3, 4, 5 **concurrently** — all five must be started
    before waiting for any result. (A sequential implementation would take ~150ms of
    sleeping; a concurrent one ~50ms. The grader measures wall time.)
  - Wait for all five to finish.
  - Then print one line per id, **in id order 1 to 5**:
    - success: `worker <id>: <value>`
    - failure: `worker <id>: failed: <message>`
  - Finally print the sum of the successful values: `total: <sum>`

## Expected stdout (byte-exact)

```
worker 1: 100
worker 2: 200
worker 3: failed: server error
worker 4: 400
worker 5: 500
total: 1200
```

## Constraints

- One single file. No external dependencies, no third-party imports (standard library only).
- No user input.
