# Task 02: Config parser + validation

> 機能軸: 文字列分解・intパース・エラー値の生成と伝播・検証。
> Meshでは `toInt`(`int | error`)・narrowing・`?`/`or`・エラーメッセージ構築が試される。
> TSは例外 or Result自作、Goは`(T, error)`戻りとの対比になる。

---

Write a complete, runnable program in {LANGUAGE}.

## Requirements

The program processes a hardcoded list of configuration lines (strings), in order:

```
port=8080
host=localhost
retries=three
debug
color=blue
port=99999
```

Parsing and validation rules, applied to each line (1-indexed):

- A line must have the form `key=value` (exactly one `=`). If not, print exactly:
  `line <n>: malformed line`
- Known keys and their rules:
  - `port`: value must be an integer between 1 and 65535 (inclusive)
  - `host`: value must be a non-empty string
  - `retries`: value must be an integer >= 0
- A line with an unknown key prints exactly: `line <n>: unknown key: <key>`
- A line whose value violates the key's rule prints exactly: `line <n>: invalid value for <key>: <value>`
- A valid line stores the value. (In this scenario no key is validly set twice; you do not need to define overwrite behavior.)

After processing all lines, print the successfully stored values, one per line, in this
fixed order — `port`, then `host`, then `retries` — skipping keys that were never
successfully set, in the form `<key> = <value>`.

## Expected stdout (byte-exact)

```
line 3: invalid value for retries: three
line 4: malformed line
line 5: unknown key: color
line 6: invalid value for port: 99999
port = 8080
host = localhost
```

## Constraints

- One single file. No external dependencies, no third-party imports (standard library only).
- No user input; the lines above are hardcoded.
