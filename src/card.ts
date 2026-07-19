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
    xs: int[] = []    // typed declaration: annotate the type explicitly (name: T = value)
    mut best: string | none = none   // start "absent", assign a real value later (needs mut)

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
    type Resp = { kind: "ok", user: User } | { kind: "notFound" }
                       // discriminated union: { field: Type, ... } ONLY valid inside a union
                       // (see "Discriminated unions" below — do not write it bare)

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
    if v is none {          // 1) narrow with \`is\` — accepts the SAME patterns as match:
                            //    none / error / closed, type names (is User, is int),
                            //    string literals (is "active"), partial shapes (is { kind: "ok" })
        return
    }
    print(v.name)           //    v is User from here on

    x := parse(s) or 0      // 2) fallback value on none/error

    y := parse(s)!          // 3) propagate none/error to the caller
                            //    (the enclosing fn's return type must include it)

    e := parse(s)           // an error's message: narrow to error, then interpolate it
    if e is error {
        print("failed: \${e}")   // \${error} renders its message; str(e) also works
    } else {
        print("ok: \${e}")       // if/else narrows BOTH branches, not just the early-return form —
    }                            // e is int here, no extra narrowing needed

Building an optional result imperatively (e.g. "the best so far"):

    mut best: string | none = none   // typed declaration lets you start absent
    for _, w := range words {
        best = w                     // string assigns fine into string | none
    }
    if best is none { return }       // narrow before using it

    msg := match v {        // 4) match — exhaustive: missing arms = compile error
        User => "hi \${v.name}"      // v is narrowed inside each arm
        none => "404"
    }

- match subjects must be union-typed. Patterns: type names (\`User\`, \`none\`, \`error\`, \`int\`, ...),
  string literals (\`"active"\`), or \`_\` (last arm only). Multiple patterns: \`"a", "b" => ...\`.
- \`x == none\` is a compile error — use \`is none\` (it narrows; \`==\` does not).

## Discriminated unions (tagged struct shapes)

    type GetUserResponse = { kind: "ok", user: User } | { kind: "notFound" } | { kind: "unauthorized" }

    fn getUser(id: string) GetUserResponse {
        u := findUser(id)
        if u is none { return GetUserResponse{ kind: "notFound" } }
        return GetUserResponse{ kind: "ok", user: u }
    }

    msg := match res {
        { kind: "ok" } => "found: \${res.user.name}"    // res.user only exists in this arm
        { kind: "notFound" } => "not found"
        { kind: "unauthorized" } => "unauthorized"
    }

- \`{ field: Type, ... }\` (an anonymous struct shape) is ONLY valid inside a \`type X = A | B\`
  union. Writing it alone (\`type X = { ... }\`) is a compile error — use \`struct X { ... }\`
  for a standalone shape.
- Build a value using the UNION's own name as the struct-literal name; the given field set
  picks which member you meant (no separate name needed per member).
- Narrow with \`match\` using a partial-shape pattern — name only the field(s) you need to pick
  the member (usually just the tag, e.g. \`kind\`). After narrowing, access the rest of that
  member's fields normally (\`res.user\`); accessing a field from a different member is a
  compile error, same as any other unnarrowed union access.
- \`is\` accepts the same partial-shape patterns, so guard-clause style works too:

      if res is { kind: "notFound" } { return "404" }
      return "found: \${res.user.name}"   // res narrowed to the remaining member(s) here
- Struct identity is STRUCTURAL, not by name: two \`struct\` declarations with the same fields
  (same names, same types) are interchangeable, and a named \`struct\` literal can be used
  wherever an anonymous \`{ ... }\` union member with the same fields is expected.
- Discriminated unions CAN be self-referential (trees, ASTs, linked structures) as long as the
  recursive reference sits inside a struct-shaped member's FIELD — the union's own name works
  as an ordinary type reference there, same as a recursive \`struct\`'s \`next: Node | none\`:

      type Tree = { kind: "leaf", value: int } | { kind: "node", left: Tree, right: Tree }

      fn leaf(v: int) Tree { return Tree{kind: "leaf", value: v} }
      fn node(l: Tree, r: Tree) Tree { return Tree{kind: "node", left: l, right: r} }

      fn sumTree(t: Tree) int {
          return match t {
              { kind: "leaf" } => t.value
              { kind: "node" } => sumTree(t.left) + sumTree(t.right)   // recursion works
          }
      }

  What's still NOT supported: two union types referencing each other DIRECTLY as bare members
  with nothing (no struct/array/map/chan) wrapping the reference, e.g. \`type A = B | none\`
  where \`type B = A | error\` — this reports \`type alias cycle\` (there's no struct field to
  "tie the knot" through). This is a narrow, rarely-needed shape; wrap the reference in a
  struct field instead, as the \`Tree\` example above does.
- Narrowing a field one \`is\` at a time still applies inside recursive/manual patterns too —
  combining checks like \`if l is none || r is none\` in a single condition is NOT currently
  narrowed (each variable needs its own \`is\`-only \`if\`). If you'd rather avoid a fixed set of
  \`kind\` values (e.g. a string tag checked manually, with no exhaustiveness checking), a plain
  recursive \`struct\` with \`T | none\` fields per variant works too — same recursion mechanism,
  just without the compiler verifying every \`kind\`/field combination for you.

## Structs, maps & methods

    u := User{name: "a", age: 1}   // ALL fields required (no zero values / defaults)
    u.age = 31                     // field writes are allowed (immutability is per-binding)
    m := map<string, int>{"a": 1}
    m["b"] = 2
    delete(m, "b")
    v := m["a"]                    // type is int | none — narrow it / use \`or\` / match
    n := len(m)                    // number of keys
    m[k] = (m[k] or 0) + 1         // count/accumulate idiom (no += , no comma-ok read)

- \`for k, v := range m\` iterates in INSERTION ORDER (deterministic). Reading a key you
  are sure exists still gives \`V | none\`, so use \`m[k] or <default>\` to get a plain value.

- Structs are reference values: a struct returned from \`find\` is the SAME object stored in
  the array, so writing \`u.age = 31\` to it updates the stored one. (Same for range loop vars.)

Methods use Go's syntax — a receiver clause right after \`fn\`, before the method name:

    fn (t: Todo) complete() Todo {
        return Todo{title: t.title, done: true}
    }
    fn (t: Todo) render() string {
        if t.done { return "[x] " + t.title }
        return "[ ] " + t.title
    }

    todos[0] = todos[0].complete()
    print(todos[0].render())
    print(Todo{title: "x", done: false}.complete().render())   // chains left-to-right

- Methods are declared at the TOP LEVEL only (never nested inside \`struct { ... }\`), one \`fn\`
  per method. The receiver type must be a \`struct\` (not int/string/array/etc).
- A method's name lives ONLY on its receiver type — \`render(t)\` does NOT work for a method
  declared as \`fn (t: Todo) render()\`; you must write \`t.render()\`. This means two different
  structs can each have their own \`describe()\` method with no collision (unlike free functions,
  which share one global name).
  Conversely, a plain function is ALWAYS called \`f(x)\`, never \`x.f()\` — there is exactly one
  call syntax per declaration, never a choice between two.
- Follow Go's convention for which to use: if an operation's natural first argument is one
  struct value and reads like "T does X", make it a method on T. Otherwise (multiple unrelated
  types, or a free-standing utility), use a plain function.

## Arrays

    xs := [1, 2, 3]        // non-empty literal — element type inferred
    ys := Todo[]{}         // empty typed array (literal form)
    zs: Todo[] = []        // empty typed array (typed declaration) — same result, more familiar
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

## Concurrency (structured — every task has an owner, leaks are impossible)

    task := spawn f(x)       // run concurrently; returns a receive port (chan<T>)
    v := <-task              // receive = wait for the result — type is T | closed (see below)
    ch := chan<string>()     // channel, unbounded buffer: ch <- v sends, <-ch receives
    wait {                   // wait EARLIER than the function exit: block until every
        spawn a()            // task spawned inside this block has finished
        spawn b()
    }
    detach logAccess(req)    // program-owned background task (see below)

Two ownership tiers — pick by how long the task should live:

- \`spawn f(x)\` is owned by the CURRENT FUNCTION: when the function returns (even via an
  early \`return\`), it implicitly waits for every task it spawned. You never need to
  clean up after spawn — a leaked/forgotten task cannot exist.
- \`detach f(x)\` is the escape hatch for work that must OUTLIVE the function (send email,
  write logs, notify): it is owned by the PROGRAM — the caller returns immediately, and
  the work finishes before the program exits. Same syntax/port as spawn. Use sparingly;
  reviewers grep for \`detach\` the way they grep for \`mut\`.
- A long-lived worker that loops forever receiving from a channel must be \`detach\`ed —
  \`spawn\` would make the enclosing function wait for it forever at its exit.

### Channel capacity — chan<T>() vs chan<T>(n)

    ch := chan<T>()     // no argument: UNBOUNDED buffer, send never blocks (the common case)
    ch := chan<T>(0)    // capacity 0: UNBUFFERED — send blocks until a receiver is ready right now
    ch := chan<T>(3)    // capacity 3: send blocks only once 3 unreceived values are already buffered

This is REAL Go-compatible blocking (not a panic-based approximation) — \`ch <- v\` on a full
bounded channel genuinely waits (\`await\`s) until space frees up.

### close — receiving is ALWAYS \`T | closed\`

Like map reads (\`V | none\`), a channel receive can ALWAYS observe "no more values", so its
type always includes \`closed\` — you must narrow before using the value, same as none/error:

    fn produce(ch: chan<int>) {
        for i := 1; i <= 3; i++ { ch <- i }
        close(ch)             // declares "no more sends" — required to end a receive loop cleanly
    }
    fn main() {
        ch := chan<int>()
        spawn produce(ch)
        for {
            v := <-ch          // type: int | closed
            if v is closed { break }
            print(v)
        }
    }

\`closed\` is its own type (like \`none\`/\`error\`) — narrow with \`is closed\` or a \`match\` arm
(\`closed => ...\`). It is NOT swept into \`!\`/\`or\` propagation (a closed channel isn't "this
function's own failure"). Sending to an already-closed channel, or closing twice, panics.

### select — wait on multiple channels, pick whichever is ready first

    result := select {
        v := <-ch1 => "from ch1: \${v}"    // v is bound to ch1's element type | closed
        v := <-ch2 => "from ch2: \${v}"
        _ => "nothing ready"               // OPTIONAL — makes the whole select non-blocking
    }

If multiple channels are ready simultaneously, one is picked pseudo-randomly (same as Go —
prevents one case from starving the others). Without a \`_\` arm, \`select\` blocks until at
least one channel is ready. \`select\`'s syntax deliberately echoes \`match\`'s \`pattern => body\`
shape, but its "patterns" are channel-receive expressions, not type patterns — it is NOT a
form of \`match\`.

## Builtins (complete list)

    print(...)  len(x)  push(arr, v)  str(x)  error(msg)  sleep(ms)  delete(m, k)
    contains(arr, v)  indexOf(arr, v)  keys(m)  values(m)  sort(arr)
    split(s, sep)  join(arr, sep)  trim(s)  upper(s)  lower(s)  toInt(s)
    filter(arr, pred)  transform(arr, f)  reduce(arr, f, init)  close(ch)

- \`print\` writes its args separated by spaces and appends a newline (one call = one line).
- push, not append. \`contains\`/\`indexOf\` work on arrays; \`indexOf\` returns \`int | none\`
  (narrow it, same as any other union). \`keys\`/\`values\` return arrays from a map (insertion
  order). \`sort(arr)\` is NON-mutating — it returns a NEW sorted array (\`int[]\`, \`float[]\` or
  \`string[]\` only, ascending); the argument is unchanged.
- \`split(s, sep)\` always returns \`string[]\` (never fails — no separator found means a
  one-element array). \`join(arr, sep)\` takes \`string[]\`. \`trim\`/\`upper\`/\`lower\` are
  string → string. \`toInt(s)\` DOES fail on non-numeric input, so it returns \`int | error\`
  — narrow it like any other failable call: \`n := toInt(s)!\` or \`n := toInt(s) or 0\`.
- Higher-order functions take a function VALUE as an argument — either a named \`fn\`, or an
  inline \`fn(...) ... { ... }\` closure (closures can capture outer variables, including
  \`mut\` ones):

      isEven := fn(n: int) bool { return n % 2 == 0 }
      evens := filter(nums, isEven)                        // T[]  (same element type)
      labels := transform(nums, fn(n: int) string { return "n\${n}" })  // can change element type
      total := reduce(nums, fn(acc: int, n: int) int { return acc + n }, 0)  // fold to Acc

  \`transform(arr, f)\` is Mesh's map-over-array (named \`transform\`, NOT \`map\` — \`map\` is
  already the \`map<K, V>\` type keyword, so \`map(arr, f)\` is a parse error; see below).
  \`reduce(arr, f, init)\` takes the callback before the initial value, matching JS's
  \`.reduce(callback, initialValue)\` order (with the array moved to the first argument).
- There are no methods on values other than struct fields, and nothing beyond the lists above:
  no regex, no string formatting/padding, no array flatten/zip/group. Write these by hand with
  \`for ... range\` until they land in the standard library.

## Does NOT exist in Mesh — never write these

null, undefined, nil / try, catch, throw, exceptions / panic(), recover /
class, inheritance, interfaces, generics / switch, while, do-while /
(T, error) multi-value returns / enum (use unions) / default args, overloads /
semicolons / backtick strings / comma-ok map reads (v, ok := m[k]) / ternary ?: (use match or if) /
methods on non-struct types (int/string/array — struct only) / function-type annotations
(a variable CAN hold a function value, e.g. \`f := fn(x: int) int {...}\`, but you cannot
write \`f: fn(int) int = ...\` — the type must be inferred from a \`:=\` declaration) /
Go's close/comma-ok idiom (\`v, ok := <-ch\`) — use \`v := <-ch\` then narrow with \`is closed\` /
send-case / default-send in select (select only reacts to RECEIVE readiness, not send readiness) /
two union types referencing each other directly as bare members with nothing wrapping the
reference (e.g. \`type A = B | none\` where \`type B = A | error\`) — wrap the reference in a
struct field instead (self-referential discriminated unions like a tree ARE supported, see above)

## Common compile errors → how to fix

    'x' is immutable — declare it with 'mut'        → change x := ... to mut x := ...
    'x' shadows an outer binding                    → rename it, or assign with = to a mut binding
    cannot access field or method on User | none    → add: if u is none { return }, then narrow it first
    undefined: 'render' (when render is a method)   → methods have no bare-name form; write t.render(), not render(t)
    'render' is a method — call it like render(...) → you wrote t.render (no parens); add ()
    match is not exhaustive — missing: ...          → add arms for the listed members, or a _ arm
    use 'is none' to test for none                  → replace == none with is none
    '!' propagates error, but this function returns int → add | error to the return type
    invalid operation: T + T | closed                → you used <-ch directly; narrow with 'is closed' first
    send on closed channel / close of closed channel → panic: don't send/close after close(ch) already ran
    range over an array needs two names             → for i, v := range arr (use _ to drop one)
    cannot use any[] as Todo[] / cannot return any[] → you wrote []; use Todo[]{} for an empty typed array
    this function has no return value (from push)   → push returns none; don't use it as a value
    panic: file:line:col: index N out of range      → check len() before indexing
    expected '<' after 'map', but got '('           → you wrote map(arr, f); use transform(arr, f)
                                                        ('map' is the map<K, V> type keyword)
    use 'struct X { ... }' to define a data shape   → you wrote type X = { ... } alone; either use
                                                        struct, or add a union: type X = {...} | {...}
    no member of 'X' matches the field(s) {...}     → the fields you wrote don't match any member of X;
                                                        check spelling and which fields that member needs
    ambiguous — multiple members of 'X' match       → add/change a field so only one member's shape fits
    type alias cycle involving 'X'                  → two unions reference each other as bare members
                                                        with nothing wrapping the reference; wrap it in
                                                        a struct field instead (self-reference through
                                                        a struct field, e.g. a tree, works fine)

## Verify your code (agents: do this after every edit)

    mesh check file.mesh --json    # {ok, diagnostics: [{file, line, col, severity, message}]}
    mesh run file.mesh             # compile and execute
`;
