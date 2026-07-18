// 言語カード: AIエージェントのコンテキストに貼る前提で設計された圧縮仕様書。
// `mesh card` で出力する。実装と乖離させないこと(カードの主張はテストで検証される)。
// 英語なのは意図的 — どのモデルの学習データとも噛み合う共通語で、トークン効率も良い。

export const LANGUAGE_CARD = `# Mesh Language Card

Mesh is a statically-typed language that compiles to JavaScript (runs in browsers and Node/Bun).
Think "TypeScript's types × Go's simplicity". This card is the COMPLETE reference —
Mesh has no features beyond what is listed here. When unsure, prefer the patterns shown.

## Program structure

- Top level allows only \`fn\`, \`struct\`, \`type\` declarations. Entry point: \`fn main()\` (required, no params, no return type).
- No semicolons (statements end at newline). Blocks always use braces. Comments: \`//\`.

## Bindings (immutable by default)

    x := 10           // immutable binding (default). Type is inferred.
    mut n := 0        // mutable — only mut bindings can be reassigned or ++/--
    n = n + 1

- No \`var\` / \`let\` / \`const\`. No uninitialized declarations.
- No shadowing: reusing an outer name (incl. function names) in \`:=\` is a compile error.
- Function parameters are always immutable (you cannot reassign the parameter itself).
- \`:=\` widens string-literal types to \`string\`, so \`mut s := "a"\` allows \`s = "b"\` later.
  (A literal type like \`"a"\` only appears where you write it explicitly, e.g. in a union.)

There are NO global/module-level variables (top level is only fn/struct/type). To share
mutable state, pass it as a parameter — arrays, maps, structs, and channels are reference
values, so a function mutating one (e.g. \`push(items, x)\`) is visible to the caller.

## Types

    int  float  string  bool  error  none  any
    T[]                // array:   nums := [1, 2, 3]  /  empty: items := Todo[]{}
    map<K, V>          // map:     ages := map<string, int>{"a": 1}  /  empty: map<string, int>{}
    chan<T>            // channel: ch := chan<string>()
    A | B              // union
    "active"           // string literal type (subtype of string)

    struct User {      // data shape (one field per line, no commas)
        name: string
        age: int
    }
    type Status = "active" | "banned"   // union/alias naming (NOT for data shapes)

## Absence & failure — THE core pattern (no null, no exceptions)

Failable functions return unions. You CANNOT use the value before narrowing it.

    fn find(id: int) User | none {
        if id == 1 { return User{name: "a", age: 1} }
        return none
    }
    fn parse(s: string) int | error {
        return error("bad input: \${s}")     // or return the int
    }

Four ways to consume a union:

    v := find(1)
    if v is none {          // 1) narrow with \`is\` (only \`is none\` / \`is error\`)
        return
    }
    print(v.name)           //    v is User from here on

    x := parse(s) or 0      // 2) fallback value on none/error

    y := parse(s)!          // 3) propagate none/error to the caller
                            //    (the enclosing fn's return type must include it)

    e := parse(s)           // an error's message: narrow to error, then interpolate it
    if e is error {
        print("failed: \${e}")   // \${error} renders its message; str(e) also works
    }

    msg := match v {        // 4) match — exhaustive: missing arms = compile error
        User => "hi \${v.name}"      // v is narrowed inside each arm
        none => "404"
    }

- match subjects must be union-typed. Patterns: type names (\`User\`, \`none\`, \`error\`, \`int\`, ...),
  string literals (\`"active"\`), or \`_\` (last arm only). Multiple patterns: \`"a", "b" => ...\`.
- \`x == none\` is a compile error — use \`is none\` (it narrows; \`==\` does not).

## Structs & maps

    u := User{name: "a", age: 1}   // ALL fields required (no zero values / defaults)
    u.age = 31                     // field writes are allowed (immutability is per-binding)
    m := map<string, int>{"a": 1}
    m["b"] = 2
    delete(m, "b")
    v := m["a"]                    // type is int | none — narrow it / use \`or\` / match
    n := len(m)                    // number of keys
    m[k] = (m[k] or 0) + 1         // count/accumulate idiom (no += , no comma-ok read)

- Structs are reference values: a struct returned from \`find\` is the SAME object stored in
  the array, so writing \`u.age = 31\` to it updates the stored one. (Same for range loop vars.)

## Arrays

    xs := [1, 2, 3]        // non-empty literal — element type inferred
    ys := Todo[]{}         // EMPTY typed array. A bare [] is any[] and won't coerce to Todo[]
    push(xs, 4)            // append in place — mutates xs, returns none (not usable as a value)
    n := len(xs)
    for i, v := range xs { }

## Control flow

    if cond { } else if cond2 { } else { }   // no parens; cond must be bool
    for i := 0; i < n; i++ { }               // C-style (header var is implicitly mutable)
    for cond { }                             // while-style
    for { break }                            // infinite
    for i, v := range arr { }                // arrays/maps ALWAYS take two names
    for _, v := range arr { }                //   use _ to drop one
    for k, v := range m { }
    for i := range 10 { }                    // 0..9 — int range takes exactly one name

\`if\` and \`match\` are the only branching. There is no switch and no while keyword.

## Operators

    + - * / %            arithmetic (int/int stays int; / by 0 panics; + also concatenates strings)
    == != < <= > >=      comparison → bool
    && || !              logical: && and ||, and PREFIX ! for negation, e.g. \`if !t.done { }\`
    -x                   numeric negation

Note: postfix \`x!\` is error/none propagation (see below); prefix \`!x\` is boolean NOT. Different things.
There is no ternary \`?:\` — use \`if\` or \`match\`.

## Strings

    s := "hello \${name}"    // interpolation is always on in "..."; escape \\$ for a literal $
    t := "a" + "b"           // + concatenates strings only — use \${x} or str(x) for other types

## Concurrency

    task := spawn f(x)       // run concurrently; returns a receive port (chan<T>)
    v := <-task              // receive = wait for the result
    ch := chan<string>()     // channel: ch <- v sends, <-ch receives
    wait {                   // block until every task spawned inside has finished
        spawn a()
        spawn b()
    }

## Builtins (complete list)

    print(...)  len(x)  push(arr, v)  str(x)  error(msg)  sleep(ms)  delete(m, k)

- \`print\` writes its args separated by spaces and appends a newline (one call = one line).
- push, not append. There are no methods on values other than struct fields, and no
  standard library yet: array find/filter/map, string split/join, parseInt, sort etc.
  must be written by hand (loop with \`for ... range\`).

## Does NOT exist in Mesh — never write these

null, undefined, nil / try, catch, throw, exceptions / panic(), recover /
class, inheritance, methods / switch, while, do-while / (T, error) multi-value returns /
enum (use unions) / default args, overloads / semicolons / backtick strings /
comma-ok map reads (v, ok := m[k]) / ternary ?: (use match or if)

## Common compile errors → how to fix

    'x' is immutable — declare it with 'mut'        → change x := ... to mut x := ...
    'x' shadows an outer binding                    → rename it, or assign with = to a mut binding
    cannot access field on User | none — narrow it  → add: if u is none { return }
    match is not exhaustive — missing: ...          → add arms for the listed members, or a _ arm
    use 'is none' to test for none                  → replace == none with is none
    '!' propagates error, but this function returns int → add | error to the return type
    range over an array needs two names             → for i, v := range arr (use _ to drop one)
    cannot use any[] as Todo[] / cannot return any[] → you wrote []; use Todo[]{} for an empty typed array
    this function has no return value (from push)   → push returns none; don't use it as a value
    panic: file:line:col: index N out of range      → check len() before indexing

## Verify your code (agents: do this after every edit)

    mesh check file.mesh --json    # {ok, diagnostics: [{file, line, col, severity, message}]}
    mesh run file.mesh             # compile and execute
`;
