// 言語カード: AIエージェントのコンテキストに貼る前提で設計された圧縮仕様書。
// `mesh card` で出力する。実装と乖離させないこと(カードの主張はテストで検証される)。
// 英語なのは意図的 — どのモデルの学習データとも噛み合う共通語で、トークン効率も良い。

export const LANGUAGE_CARD = `# Mesh Language Card

Mesh is a statically-typed language that compiles to JavaScript (runs in browsers and Node/Bun).
Think "TypeScript's types × Go's simplicity". This card is the COMPLETE reference —
Mesh has no features beyond what is listed here. When unsure, prefer the patterns shown.

## Program structure

- Top level allows only \`import\`, \`fn\`, \`struct\`, \`type\` declarations, and top-level constants
  (\`name := value\` / \`name: T = value\`, F-9c — imports first). Entry point: \`fn main()\`
  (required, no params, no return type).
- No semicolons (statements end at newline). Blocks always use braces. Comments: \`//\`.

## Bindings (immutable by default)

    x := 10           // immutable binding (default). Type is inferred.
    mut n := 0        // mutable — only mut bindings can be reassigned or ++/--
    n = n + 1
    n += 1            // compound assignment: += -= *= /= %= (F-9b). Not on a map entry — the
                       // key may not exist, so read with a fallback first: m[k] = (m[k] or 0) + 1
    xs: int[] = []    // typed declaration: annotate the type explicitly (name: T = value)
    mut best: string | none = none   // start "absent", assign a real value later (needs mut)

- No \`var\` / \`let\` / \`const\`. No uninitialized declarations.
- No shadowing: reusing an outer name (incl. function names) in \`:=\` is a compile error.
- JS reserved words can NOT be used as variable or function names (Mesh compiles to JS):
  \`await async function const let var class new this typeof instanceof in of yield delete
  void switch case default do while with export import extends super null undefined try
  catch finally throw eval arguments\`. Naming a function \`eval\` is a compile error —
  pick another name (e.g. \`evaluate\`).
- Function parameters are always immutable (you cannot reassign the parameter itself).
- \`:=\` widens string-literal types to \`string\`, so \`mut s := "a"\` allows \`s = "b"\` later.
  (A literal type like \`"a"\` only appears where you write it explicitly, e.g. in a union.)

Top-level bindings are always immutable — \`mut\` is not allowed at the top level, so there is
no mutable global state (F-9c). \`export\` makes a top-level constant visible as \`pkg.name\`
from other packages, same as \`fn\`/\`struct\`/\`type\`. To share MUTABLE state, pass it as a
parameter — arrays, maps, structs, and channels are reference values, so a function mutating
one (e.g. \`push(items, x)\`) is visible to the caller.

    maxRetries := 3                  // top-level constant — visible to every fn in the package
    export basePath: string = "/api" // exported — other packages use it as pkg.basePath

## Types

    int  float  string  bool  error  none  any
    T[]                // array:   nums := [1, 2, 3]  /  empty: items: Todo[] = []
    map<K, V>          // map:     ages := map<string, int>{"a": 1}  /  empty: map<string, int>{}
    chan<T>            // channel: ch := chan<string>(none)
    A | B              // union
    "active"           // string literal type (subtype of string)
    fn(int, string) bool   // function type — types only, no parameter names.
                       // Reads like declarations: in fn(int) int | error the union is the
                       // RETURN type; to put the function itself in a union, parenthesize:
                       // (fn(int) int) | none

    struct User {      // data shape (one field per line, no commas)
        name: string
        age: int
    }
    type Status = "active" | "banned"   // union/alias naming (NOT for data shapes)
    type Resp = { kind: "ok", user: User } | { kind: "notFound" }
                       // discriminated union: { field: Type, ... } ONLY valid inside a union
                       // (see "Discriminated unions" below — do not write it bare)

## Generic functions

    fn first<T>(arr: T[], pred: fn(T) bool) T | none {
        for _, v := range arr {
            if pred(v) { return v }
        }
        return none
    }

    nums := [1, 2, 3, 4]
    r := first(nums, fn(n: int) bool { return n > 2 })   // T=int inferred from the arguments —
                                                          // call it like any other function, no <int>

- Only top-level \`fn\` declarations can have type parameters (\`fn name<T, U>(...)\`) — no generic
  \`struct\`/\`type\`, no generic methods, no generic \`fn(...){...}\` closures.
- \`T\` is fully abstract inside the body: you can move values of type \`T\` around (store them,
  pass them to other functions, return them, put them in \`T[]\`/\`T | none\`, compare two \`T\`s
  with \`==\`/\`!=\`) but NOT do type-specific operations on them (\`T + 1\`, \`t.field\`, \`t < 0\`).
  This is why \`sort\` stays a built-in — it needs \`<\`, which a bare \`T\` doesn't support.
- Every type parameter MUST appear in at least one parameter type (\`T[]\`, \`fn(T) bool\`, ...,
  not just the return type) — the compiler infers \`T\` from the call's arguments, there is no
  \`first<int>(...)\` explicit-instantiation syntax. \`fn zero<T>() T\` is a compile error for
  exactly this reason (nothing in the call \`zero()\` tells the compiler what \`T\` is).
- Call it by name directly — \`first(nums, pred)\`. Assigning a generic function to a variable
  first (\`f := first\`) and calling that isn't supported; \`T\` stays unresolved and the call fails.

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

    v2 := find(2) or defaultUser()   // 2) fallback — the plain form is for none ONLY.
    x := parse(s) or _ => 0          //    a union containing error REQUIRES a binding:
    x2 := parse(s) or e => report(e) //    \`or e => expr\` (use it) or \`or _ => expr\`
                                     //    (discard it — the discard is visible & greppable)

    y := parse(s)?          // 3) propagate none/error to the caller
                            //    (the enclosing fn's return type must include it)
    y2 := parse(s) ? "config port"   // 3b) propagate WITH context: on failure, propagates
                            //    error("config port: <original message>") — none is upgraded
                            //    to error("config port") too, so the return type needs error.
                            //    The context must be a string literal (interpolation is fine)

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
- A match arm's body is a single EXPRESSION — there is no block body and no "do nothing" arm
  (\`none => {}\` is a compile error). When some cases should do nothing, use \`if x is ...\`
  instead of match.
- \`x == none\` is a compile error — use \`is none\` (it narrows; \`==\` does not).
- \`is\` narrowing composes: \`a is Foo && a.value > 0\` narrows \`a\` for the right side of \`&&\`
  AND inside the \`then\` block; \`a is none || b is none { return }\` narrows both \`a\` and \`b\`
  (to non-none) in the code after; \`!(a is none)\` narrows its \`then\` block the same way the
  \`else\` of \`a is none\` would. It also narrows FIELD PATHS directly — \`if n.next is none { ... }\`
  lets you use \`n.next\` as its narrowed type afterward, no need to copy it into a local
  variable first (this needs an immutable root, e.g. \`n := ...\`, not \`mut n := ...\`).

## Discriminated unions (tagged struct shapes)

    type GetUserResponse = { kind: "ok", user: User }
        | { kind: "notFound" }
        | { kind: "unauthorized" }

Long union declarations can be split across lines — continuation lines start with \`|\`
(as above), or the line can end with \`|\`; both work in \`type\` declarations.

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
- Build a value using the UNION's own name as the struct-literal name. A REQUIRED tag field
  (F-7) — one field name present in every member, with a distinct string-literal value in
  each (\`kind\` by convention, but any name works) — picks which member you meant, by its
  VALUE alone. This is declared once and enforced at the \`type\` declaration: a union with 2+
  anonymous members that has no such field is a compile error right there (\`discriminated
  union 'X' needs a tag field\`), before anyone tries to construct one. This also means adding
  a member elsewhere can never silently break an existing literal — resolution never compares
  against the OTHER members' field sets, only the tag value written right there.
- Narrow with \`match\` using a partial-shape pattern — name only the field(s) you need to pick
  the member (usually just the tag, e.g. \`kind\`). After narrowing, access the rest of that
  member's fields normally (\`res.user\`); accessing a field from a different member is a
  compile error, same as any other unnarrowed union access.
- \`is\` accepts the same partial-shape patterns, so guard-clause style works too:

      if res is { kind: "notFound" } { return "404" }
      return "found: \${res.user.name}"   // res narrowed to the remaining member(s) here
- Struct identity is NOMINAL (by name): two \`struct\` declarations with the same fields are
  DIFFERENT types — \`struct Meters { value: float }\` is rejected where \`Dollars\` is expected,
  so unit/ID wrapper structs actually protect you. The one place comparison is structural is
  where no name exists: anonymous \`{ ... }\` union members — a NAMED struct value can still be
  used where an anonymous member with the same fields is expected.
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
- Narrowing composes here too: \`if l is none || r is none { return }\` narrows both \`l\` and \`r\`
  afterward, and field paths narrow directly (\`if t.left is none { ... }\`, no local copy
  needed). If you'd rather avoid a fixed set of \`kind\` values (e.g. a string tag checked
  manually, with no exhaustiveness checking), a plain recursive \`struct\` with \`T | none\` fields
  per variant works too — same recursion mechanism, just without the compiler verifying every
  \`kind\`/field combination for you.

## Structured errors (discriminated unions that '?'/'or' can propagate)

Plain \`error\` is a message string — fine until you need to branch on WHAT failed (retry a
timeout, but 404 a not-found). Mark a discriminated union (or a plain struct) with \`error\` and
\`?\`/\`or\` treat it as a failure, exactly like \`none\`/\`error\`:

    error type DbError = { kind: "notFound", table: string } | { kind: "timeout", ms: int }

    fn find(id: int) User | DbError {
        if id == 1 { return User{name: "a", age: 1} }
        return DbError{kind: "notFound", table: "users"}
    }

    fn useIt(id: int) string | DbError {
        u := find(id)?               // DbError propagates because DbError was declared 'error type'
        return u.name
    }

    x := find(2) or e => match e {   // bound form: e is DbError, branch on its kind
        { kind: "notFound" } => defaultUser()
        { kind: "timeout" } => retryUser()
    }

- \`error type X = { ... } | { ... }\` or the single-shape form \`error struct X { field: T }\` —
  put \`error\` right before \`type\`/\`struct\`. Members must be struct-shaped (inline \`{ ... }\`,
  or a fresh \`error struct\`) — you can't retroactively tag an existing named type
  (\`error type X = SomeOtherStruct\` is a compile error; declare \`error struct SomeOtherStruct\`
  instead, or use an inline shape).
- Plain \`?\` (no context) propagates a structured error as-is. The context form
  (\`f() ? "line \${i}"\`) does NOT work with structured errors — it always converts to a message
  string, and a structured error has no message to convert. Use plain \`?\`, or narrow it with
  \`is\`/\`match\` first, and build your own \`error("...")\` if you need a string.
- \`or\` follows the same explicitness rule as plain \`error\`: a union containing a structured
  error type requires the bound form (\`or e => ...\` / \`or _ => ...\`); only \`none\`-only unions
  can use the plain fallback form.
- This is purely additive — plain \`error\`/\`none\` and existing \`?\`/\`or\` code are unaffected.

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
- Writing to a field (\`n.next = ...\`) drops any narrowing you'd done on that field/path — the
  compiler forgets what it was and you narrow it again with \`is\` before using it.

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
    zs: Todo[] = []        // empty typed array — always use a typed declaration (there is no
                           // 'Todo[]{}' literal form; a plain '[]' becomes the right type
                           // wherever one is expected, so a literal form would just duplicate this)
    ws := Todo[]{a, b}     // non-empty typed array literal — still useful to force an explicit
                           // element type instead of inferring one from the first element
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

Note: postfix \`x?\` is error/none propagation (like Rust's \`?\`); prefix \`!x\` is boolean NOT.
There is no ternary \`?:\` and no optional chaining \`?.\` — use \`if\` or \`match\`.

## Strings

    s := "hello \${name}"    // interpolation is always on in "..."; escape \\$ for a literal $
    t := "a" + "b"           // + concatenates strings only — use \${x} or str(x) for other types

## Concurrency (structured — every task has an owner, leaks are impossible)

    task := spawn f(x)       // run concurrently; returns a receive port (chan<T>)
    v := <-task              // receive = wait for the result — type is T | closed (see below)
    ch := chan<string>(none) // channel, unbounded buffer: ch <- v sends, <-ch receives
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

### Channel capacity — always explicit (F-11): chan<T>(none) vs chan<T>(n)

Capacity is a REQUIRED argument — \`chan<T>()\` is a compile error. An unbounded channel lets a
long-lived \`detach\`ed worker's inbox grow forever if nothing drains it (a resource leak with
no syntactic warning sign), so choosing "unbounded" must be written out, not left as a default:

    ch := chan<T>(none) // explicitly UNBOUNDED: send never blocks (only when you've reasoned
                         // about why unbounded growth here is fine)
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
        ch := chan<int>(none)
        spawn produce(ch)
        for {
            v := <-ch          // type: int | closed
            if v is closed { break }
            print(v)
        }
    }

\`closed\` is its own type (like \`none\`/\`error\`) — narrow with \`is closed\` or a \`match\` arm
(\`closed => ...\`). It is NOT swept into \`?\`/\`or\` propagation (a closed channel isn't "this
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
    contains(arr, v)  indexOf(arr, v)  get(arr, i)  keys(m)  values(m)  sort(arr)
    split(s, sep)  join(arr, sep)  trim(s)  upper(s)  lower(s)  toInt(s)
    filter(arr, pred)  map(arr, f)  reduce(arr, f, init)  close(ch)

- \`print\` writes its args separated by spaces and appends a newline (one call = one line).
- push, not append. \`contains\`/\`indexOf\` work on arrays; \`indexOf\` returns \`int | none\`
  (narrow it, same as any other union). \`get(arr, i)\` is a type-safe read — returns
  \`T | none\` instead of panicking on an out-of-range index (F-9d). Use \`arr[i]\` when an
  out-of-range index is a bug you want to panic on (e.g. a loop bound you control yourself);
  use \`get(arr, i)\` when the index comes from outside (user input, another data source) and
  out-of-range is an expected case to handle, not a bug:

      first := get(xs, 0) or 0            // safe read with a fallback
      v := get(xs, i)
      if v is none { return error("no element at \${i}") }

  \`keys\`/\`values\` return arrays from a map (insertion order). \`sort(arr)\` is NON-mutating —
  it returns a NEW sorted array (\`int[]\`, \`float[]\` or \`string[]\` only, ascending); the
  argument is unchanged.
- \`split(s, sep)\` always returns \`string[]\` (never fails — no separator found means a
  one-element array). \`join(arr, sep)\` takes \`string[]\`. \`trim\`/\`upper\`/\`lower\` are
  string → string. \`toInt(s)\` DOES fail on non-numeric input, so it returns \`int | error\`
  — narrow it like any other failable call: \`n := toInt(s)?\` or \`n := toInt(s) or _ => 0\`.
- Higher-order functions take a function VALUE as an argument — either a named \`fn\`, or an
  inline \`fn(...) ... { ... }\` closure (closures can capture outer variables, including
  \`mut\` ones):

      isEven := fn(n: int) bool { return n % 2 == 0 }
      evens := filter(nums, isEven)                        // T[]  (same element type)
      labels := map(nums, fn(n: int) string { return "n\${n}" })  // can change element type
      total := reduce(nums, fn(acc: int, n: int) int { return acc + n }, 0)  // fold to Acc

  \`map(arr, f)\` is Mesh's map-over-array (F-8: \`map\` is a context-dependent keyword — \`map<K, V>\`
  in a type position is still the map TYPE, \`map(arr, f)\` in an expression position is this
  function; the two never collide because one needs \`<\` next and the other needs \`(\`).
  \`reduce(arr, f, init)\` takes the callback before the initial value, matching JS's
  \`.reduce(callback, initialValue)\` order (with the array moved to the first argument).
- You can write your OWN higher-order functions with function types (\`fn(T) U\` — see Types):

      fn apply(f: fn(int) int, x: int) int { return f(x) }
      fn retryInt(f: fn() int | error, tries: int) int | error {
          for i := 1; i <= tries; i++ {
              v := f()
              if v is error { continue }
              return v
          }
          return error("gave up after \${tries}")
      }
      fn makeAdder(n: int) fn(int) int { return fn(x: int) int { return x + n } }

  Function types also work as struct fields (\`onHit: fn(int) int\`), in typed declarations
  (\`double: fn(int) int = fn(x: int) int { return x * 2 }\`), and in channels
  (\`chan<fn(int) int>\`). They are still MONOMORPHIC — no generics yet, so a helper that
  should work for any element type must be written per-type (or use the builtins above).
- There are no methods on values other than struct fields, and nothing beyond the lists above:
  no regex, no string formatting/padding, no array flatten/zip/group. Write these by hand with
  \`for ... range\` until they land in the standard library.

## Modules (import / export)

A package is a DIRECTORY under the project root (= the entry file's directory); the directory
name is the package name. All .mesh files inside one package share one flat namespace — no
import between them. The entry program is the single entry .mesh file; to split code, put it
in a package directory and import it:

    // mathutil/ops.mesh
    export fn add(a: int, b: int) int { return a + b }   // export = visible to importers
    fn helper(n: int) int { return n * 2 }               // no export = package-private

    // app.mesh (the entry file)
    import "mathutil"                     // imports go at the top, before all declarations
    fn main() {
        print(mathutil.add(1, 2))         // always qualified: pkg.symbol (one way to call)
        p := mathutil.Point{x: 1, y: 2}   // exported structs construct with the qualifier
        q: mathutil.Point = mathutil.origin()   // qualify in type positions too
    }

- \`export\` goes on top-level \`fn\` / \`struct\` / \`type\` / constant (F-9c). Methods are NOT
  exported individually — they work wherever their struct is usable (export the struct).
- Accessing an unexported symbol, importing an unknown package, and import cycles are all
  compile errors. A package cannot import itself.
- v1 limits: package paths are single directory names (no \`"a/b"\` nesting). \`"mesh/..."\` is
  reserved for the standard library — \`mesh/io\` and \`mesh/json\` exist (see next section);
  any other \`mesh/...\` path is not implemented yet and is an \`unknown package\` error.

## Standard library: mesh/io, mesh/json (F-14)

Only builtin package with NO .mesh source — it needs native file/JSON access, so it's wired
directly into the compiler instead of being a directory of Mesh files.

    import "mesh/io"
    import "mesh/json"

    fn main() {
        text := io.readFile(io.args()[0])     // string | error
        if text is error { print("read failed: \${text}"); return }

        v := json.parse(text)                 // json.Value | error
        if v is error { print("bad json: \${v}"); return }
        if v is { kind: "obj" } {
            print(len(v.entries))             // v.entries is map<string, json.Value>
        }
        print(json.stringify(v))
    }

- \`io.args() string[]\` — the program's own CLI arguments (\`mesh run file.mesh a b\` → \`["a","b"]\`,
  not including the file path itself). Empty outside a CLI context (e.g. the browser playground).
- \`io.readFile(path: string) string | error\` — reads a whole file as UTF-8 text. A missing
  file or any other read failure is \`error\`, never a panic (this is an EXPECTED failure —
  the caller doesn't control whether the file exists).
- \`json.Value\` is a discriminated union (F-7: has a required \`kind\` tag, like any other) —
  the self-referential-type showcase:

      type Value = { kind: "str", s: string } | { kind: "num", n: float }
                 | { kind: "bool", b: bool } | { kind: "null" }
                 | { kind: "arr", items: Value[] } | { kind: "obj", entries: map<string, Value> }

  Build one directly with \`json.Value{kind: "str", s: "hi"}\`, same as any discriminated union
  construction. Narrow with \`match\`/\`is\` partial-shape patterns exactly like a user-declared one.
- \`json.parse(text: string) json.Value | error\` — malformed JSON is \`error\`, not a panic.
  \`json.stringify(v: json.Value) string\` — the inverse; never fails (a well-formed \`Value\`
  tree always serializes).

## Does NOT exist in Mesh — never write these

null, undefined, nil / try, catch, throw, exceptions / panic(), recover /
class, inheritance, interfaces, generics (monomorphic function types like \`fn(int) int\` DO exist — see Types) / switch, while, do-while /
(T, error) multi-value returns / enum (use unions) / default args, overloads /
semicolons / backtick strings / comma-ok map reads (v, ok := m[k]) / ternary ?: (use match or if) /
methods on non-struct types (int/string/array — struct only) /
Go's close/comma-ok idiom (\`v, ok := <-ch\`) — use \`v := <-ch\` then narrow with \`is closed\` /
send-case / default-send in select (select only reacts to RECEIVE readiness, not send readiness) /
two union types referencing each other directly as bare members with nothing wrapping the
reference (e.g. \`type A = B | none\` where \`type B = A | error\`) — wrap the reference in a
struct field instead (self-referential discriminated unions like a tree ARE supported, see above)

## Diagnostic codes, explanations, and machine-applicable fixes

\`mesh check <file> --json\` gives every diagnostic a stable \`code\` (e.g. \`"use-is-none"\`), and,
when the fix is safe and mechanical, a \`fix\` field: \`{ description, range: {start, end}, replacement }\`
— a single text replacement you can apply directly, no parsing required. Diagnostics without an
obvious one-token fix omit the \`fix\` field entirely (don't invent one). Run \`mesh explain <code>\`
for a longer explanation of what a code means and how to think about it; run it with no code to
list every known code. The table below is the same information in prose form, keyed by message
instead of by code — use whichever is more convenient.

## Common compile errors → how to fix

    'x' is immutable — declare it with 'mut'        → change x := ... to mut x := ...
    top-level bindings are always immutable         → 'mut' isn't allowed at the top level (F-9c);
                                                        pass mutable state as a parameter instead
    'x' shadows an outer binding                    → rename it, or assign with = to a mut binding
    cannot access field or method on User | none    → add: if u is none { return }, then narrow it first
    undefined: 'render' (when render is a method)   → methods have no bare-name form; write t.render(), not render(t)
    'render' is a method — call it like render(...) → you wrote t.render (no parens); add ()
    match is not exhaustive — missing: ...          → add arms for the listed members, or a _ arm
    use 'is none' to test for none                  → replace == none with is none
    '?' propagates error, but this function returns int → add | error to the return type
    postfix '!' was renamed — use '?'               → propagation is now x? (Rust-style); prefix !x
                                                        is still boolean NOT
    'or' would silently discard an error            → bind it ('or e => expr') or discard it
                                                        explicitly ('or _ => expr'); plain 'or expr'
                                                        is only for none-typed fallbacks
    invalid operation: T + T | closed                → you used <-ch directly; narrow with 'is closed' first
    chan<T>() no longer defaults to an unbounded buffer → write chan<T>(none) for unbounded,
                                                        or chan<T>(n) for a bounded buffer
    send on closed channel / close of closed channel → panic: don't send/close after close(ch) already ran
    range over an array needs two names             → for i, v := range arr (use _ to drop one)
    cannot use any[] as Todo[] / cannot return any[] → the [] has no type context here; add one
                                                        (xs: Todo[] = [] or a declared return type)
    empty typed array literal 'T[]{}' was removed    → write 'xs: T[] = []' instead
    cannot use '+=' on a map entry                   → read with a fallback first:
                                                        m[k] = (m[k] or 0) + 1
    this function has no return value (from push)   → push returns none; don't use it as a value
    panic: file:line:col: index N out of range      → check len() before indexing
    use 'struct X { ... }' to define a data shape   → you wrote type X = { ... } alone; either use
                                                        struct, or add a union: type X = {...} | {...}
    discriminated union 'X' needs a tag field       → give every anonymous member a shared field
                                                        with a distinct string-literal value (F-7),
                                                        e.g. add kind: "ok" / kind: "notFound"
    'X{...}' needs its tag field 'kind' set         → you left out the tag (or gave a non-literal
                                                        value); write kind: "..." to select a member
    no member of 'X' has kind: "..."                → check the tag value for a typo (F-7)
    no member of 'X' matches the field(s) {...}     → (union of named structs only) the fields you
                                                        wrote don't match any member; check spelling
    ambiguous — multiple members of 'X' match       → (union of named structs only) add/change a
                                                        field so only one member's shape fits
    type alias cycle involving 'X'                  → two unions reference each other as bare members
                                                        with nothing wrapping the reference; wrap it in
                                                        a struct field instead (self-reference through
                                                        a struct field, e.g. a tree, works fine)
    'x' is not exported by package 'y'              → add 'export' to its declaration in y/
    unknown package 'x'                             → the entry file's directory needs an x/ folder
                                                        with .mesh files (and: import "x" at the top)
    imports must come before all declarations       → move every import to the very top of the file
    'x' is a package — use it as a qualifier        → write x.something; a package name alone isn't a value

## Verify your code (agents: do this after every edit)

    mesh check file.mesh --json    # {ok, diagnostics: [{file, line, col, severity, code, message, fix?}]}
    mesh explain <code>            # longer explanation of a diagnostic code (no code = list them all)
    mesh run file.mesh             # compile and execute
`;
