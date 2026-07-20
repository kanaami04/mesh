// 診断コード(F-13): 全診断メッセージを意味でグルーピングした安定な識別子。
// `mesh check --json` の code フィールドと `mesh explain <code>` の入力になる。
// diagnosticsToJson と同じ「安定した機械可読フォーマット、削除・改名しない」契約の一部。
// 新しい error() 呼び出しを足すときは、まず既存コードで表現できないか探すこと —
// 呼び出し箇所ごとに新設すると、コードが「意味の分類」ではなく「行番号のあだ名」になってしまう

import type { Pos } from "./token";

export type DiagnosticCode =
  // 名前・宣言
  | "reserved-word"
  | "builtin-redeclared"
  | "builtin-type-redeclared"
  | "name-conflicts-with-package"
  | "already-declared"
  | "shadowing"
  | "undefined-name"
  | "builtin-as-value"
  | "package-as-value"
  // 型・エイリアス
  | "unknown-type"
  | "unknown-package"
  | "unknown-package-type"
  | "package-symbol-is-a-type"
  | "unknown-package-function"
  | "not-exported"
  | "type-alias-cycle"
  | "error-type-must-be-struct"
  | "error-type-aliases-existing"
  // 代入可能性・演算
  | "type-mismatch"
  | "not-bool"
  | "invalid-operation"
  | "incomparable-types"
  | "use-is-none"
  | "division-by-zero"
  // 可変性
  | "immutable-assignment"
  | "compound-assign-on-map"
  // 関数・呼び出し
  | "argument-count"
  | "not-callable"
  | "builtin-arg-type"
  | "callback-signature-mismatch"
  | "missing-return-value"
  | "void-used-as-value"
  | "invalid-main-signature"
  | "missing-main"
  // struct・フィールド
  | "not-a-struct"
  | "unknown-field"
  | "method-not-called"
  | "duplicate-field"
  | "missing-fields"
  | "discriminated-union-no-match"
  | "discriminated-union-ambiguous"
  | "discriminated-union-tag-required"
  | "discriminated-union-tag-missing"
  | "method-field-conflict"
  | "duplicate-method"
  | "invalid-receiver-type"
  // union・narrowing・match・is
  | "narrow-required"
  | "union-required"
  | "impossible-pattern"
  | "unreachable-pattern"
  | "wildcard-not-alone"
  | "empty-match"
  | "mixed-void-arms"
  | "match-not-exhaustive"
  | "empty-select"
  // 伝播(?/or)
  | "prop-context-not-string"
  | "prop-requires-failure-union"
  | "prop-nothing-to-propagate"
  | "prop-context-structured-error"
  | "prop-return-type-mismatch"
  | "or-never-fails"
  | "or-requires-binding"
  | "or-no-success-value"
  | "or-fallback-type-mismatch"
  // ジェネリクス
  | "generic-type-param-conflict"
  | "generic-type-param-not-inferable"
  | "generic-inference-failed"
  // 添字・チャネル・range
  | "not-indexable"
  | "invalid-index-type"
  | "not-a-channel"
  | "not-rangeable"
  | "range-arity"
  // パッケージ・import(モジュールレベル)
  | "invalid-package-name"
  | "self-import"
  | "import-cycle"
  // 構文(パーサ)
  | "syntax-error"
  | "import-order"
  | "invalid-top-level-declaration"
  | "top-level-mut-not-allowed"
  | "chan-capacity-required"
  | "invalid-test-signature"
  | "invalid-import-path"
  | "bare-struct-shape"
  | "method-export-redundant"
  | "multiple-return-values-removed"
  | "interpolation-in-type"
  | "fn-type-with-param-names"
  | "invalid-assignment-target"
  | "misplaced-mut"
  | "invalid-spawn-target"
  | "multiple-select-defaults"
  | "postfix-bang-renamed"
  | "empty-typed-array-literal-removed"
  // 字句(レキサ)
  | "unterminated-string"
  | "unknown-escape"
  | "unterminated-interpolation"
  | "empty-interpolation"
  | "unexpected-character";

// 機械適用可能な単一range置換のfix(LSPのTextEditに近い最小形)。
// 複数編集が要る/安全に自動化できない診断は fix 無しのままにする(無理に作らない —
// 誤ったfixを機械的に適用させる方が、fix無しより害が大きい)
export interface Fix {
  description: string;
  range: { start: Pos; end: Pos };
  replacement: string;
}

export interface Diagnostic {
  pos: Pos;
  code: DiagnosticCode;
  message: string;
  file?: string; // 複数ファイルコンパイル時にどのファイルのエラーかを示す
  fix?: Fix;
}

// mesh explain <code> の中身。各コードにつき「この種類のエラーは何を意味するか」の一般論。
// 具体的な値(型名・変数名など)は message 側の役目なので、ここでは繰り返さない
export const DIAGNOSTIC_EXPLANATIONS: Record<DiagnosticCode, string> = {
  "reserved-word":
    "The name collides with a word that has special meaning in the JavaScript Mesh compiles to " +
    "(e.g. 'eval', 'class', 'await'). Pick a different name — there is no way to escape or quote it.",
  "builtin-redeclared":
    "The name is already used by a Mesh builtin function (print, len, push, ...). Builtins share " +
    "one namespace with your declarations so there is only one way to call something; pick another name.",
  "builtin-type-redeclared":
    "The name is one of Mesh's built-in type names (int, string, error, none, ...) and can't be " +
    "reused for a 'type' or 'struct' declaration.",
  "name-conflicts-with-package":
    "The name is already used as an import alias in this file. Import aliases and other declarations " +
    "share one namespace so qualified access (pkg.symbol) stays unambiguous.",
  "already-declared":
    "This name is already declared in the same scope (or, for types, in the same package). Pick a " +
    "different name, or if you meant to update the existing binding, use '=' instead of ':='.",
  "shadowing":
    "Reusing an outer name (including a function's own name) with ':=' is rejected — it is almost " +
    "always a bug where you meant to update the outer binding with '=' instead of creating a new one.",
  "undefined-name":
    "No variable, parameter, or function with this name is visible here. Check spelling and that the " +
    "declaration comes before this use (or, for another package, that it's imported and exported).",
  "builtin-as-value":
    "A builtin function was referenced without calling it (e.g. 'x := print'). Builtins can only be " +
    "called directly, like print(...); they can't be passed around as values.",
  "package-as-value":
    "An imported package name was used as a plain value. Package names only work as a qualifier " +
    "before '.', like pkg.symbol — they aren't values themselves.",
  "unknown-type":
    "No 'type' or 'struct' declaration with this name exists in the current package.",
  "unknown-package":
    "This package name isn't recognized — either it was never imported ('import \"path\"') or the " +
    "import path itself doesn't match any package directory.",
  "unknown-package-type":
    "The package exists and is imported, but it has no type declaration with this name.",
  "package-symbol-is-a-type":
    "This qualified name (pkg.Name) refers to a TYPE in that package, not a function — use it in a " +
    "type position, or as pkg.Name{...} to construct a value of it, not as a call pkg.Name(...).",
  "unknown-package-function":
    "The package exists and is imported, but it has no function declaration with this name.",
  "not-exported":
    "The function or type exists in that package, but wasn't declared with 'export' — add 'export' " +
    "to its declaration to make it visible outside the package.",
  "type-alias-cycle":
    "Two or more 'type' aliases reference each other with nothing (no struct/array/map/chan) wrapping " +
    "the reference, so there's no field to 'tie the knot' through. Wrap the recursive reference inside " +
    "a struct field instead (like a linked-list or tree node's 'next' field).",
  "error-type-must-be-struct":
    "Every member of an 'error type'/'error struct' declaration must be struct-shaped (like a " +
    "discriminated union) — a bare primitive type can't be marked as a structured error.",
  "error-type-aliases-existing":
    "'error type X = SomeOtherType' tries to tag a type that's already declared elsewhere by " +
    "reference, which would leak the error marker onto every other use of that type. Use an inline " +
    "struct shape ({ ... }) or declare a fresh 'error struct' instead.",
  "type-mismatch":
    "A value's type isn't assignable at this position (assignment, return, argument, struct field, " +
    "array/map element, channel send, ...). Check the expected type named in the message; narrowing " +
    "a union first (with 'is' or 'match') is the usual fix if the value is a union.",
  "not-bool":
    "This position requires a plain 'bool' value (an 'if'/'for' condition, '&&'/'||' operands, or '!') " +
    "but the expression has a different type.",
  "invalid-operation":
    "The operator isn't defined for these operand types. Arithmetic operators need numbers; '+' also " +
    "works for strings but not for mixing a string with a non-string (use str() to convert first).",
  "incomparable-types":
    "'==', '!=', '<', '<=', '>', '>=' require both sides to be a compatible type (or both numeric, or " +
    "both string-like for ordering). Narrow a union to a concrete type first if that's what's happening.",
  "use-is-none":
    "'== none' / '!= none' don't narrow the type (Mesh has no generic null), so they're rejected — use " +
    "'is none' instead, which both tests for absence and narrows the type in the branch that follows.",
  "division-by-zero":
    "Integer division or modulo by the literal 0 is caught at compile time — it would always panic at " +
    "runtime, so there's no reason to wait until then to report it.",
  "immutable-assignment":
    "This binding was declared without 'mut', so it can't be reassigned or have '++'/'--' applied to " +
    "it. Add 'mut' to the declaration if it should be mutable.",
  "compound-assign-on-map":
    "Compound assignment (+=, -=, *=, /=, %=) isn't allowed on a map entry because reading it can " +
    "return 'none' for a missing key — computing 'current op value' would silently produce garbage " +
    "for a key that doesn't exist yet. Read with an explicit fallback first: " +
    "'m[k] = (m[k] or fallback) op value'.",
  "argument-count":
    "The call passes a different number of arguments than the function/method/builtin expects.",
  "not-callable":
    "The expression being called isn't a function value (or a function type wasn't found for it).",
  "builtin-arg-type":
    "One of this builtin function's arguments has the wrong type or shape for what the builtin does " +
    "(e.g. push() needs an array, error() needs a string).",
  "callback-signature-mismatch":
    "A callback passed to filter/transform/reduce doesn't have the parameter count, parameter type, or " +
    "return type the builtin requires for that callback.",
  "missing-return-value":
    "The function's declared return type requires a value, but this 'return' has none (or the function " +
    "falls off the end without one).",
  "void-used-as-value":
    "A call to a function with no return type (void) was used somewhere a value is required — e.g. as " +
    "an argument, in an expression, or via 'return <void call>' from a function that returns void wasn't " +
    "expected to. Void-returning calls can only be used as statements.",
  "invalid-main-signature":
    "'fn main()' must take no parameters and return nothing — it's the program's entry point, not a " +
    "regular function other code calls.",
  "missing-main":
    "No 'fn main()' (with no receiver) was found in the entry package. Every Mesh program starts from " +
    "'fn main() { ... }'.",
  "not-a-struct":
    "This value or name isn't a struct type, so struct-only operations (field access, struct-literal " +
    "construction) don't apply to it.",
  "unknown-field":
    "The struct type has no field (or method) with this name. The message lists the fields that do exist.",
  "method-not-called":
    "A method name was referenced without calling it. Methods can only be used as recv.method(...), " +
    "not passed around as bare values.",
  "duplicate-field":
    "The same field name appears twice in one struct literal.",
  "missing-fields":
    "A struct literal must set every field of the struct (Mesh has no zero values / default field " +
    "values) — the message lists which ones are missing.",
  "discriminated-union-no-match":
    "For a union with 2+ anonymous '{...}' members (F-7): no member's tag value matches the one " +
    "written in this struct literal — check the tag value for a typo. For a union of independently " +
    "named structs (each constructed by its own name, e.g. 'Circle | Square'): the written field " +
    "set doesn't exactly match any member's fields — check for typos or a missing/extra field.",
  "discriminated-union-ambiguous":
    "Only possible for a union of independently named structs (each normally constructed by its own " +
    "name, e.g. 'Circle | Square') — a union with 2+ anonymous '{...}' members always resolves by a " +
    "required tag value instead (F-7), which can't be ambiguous. Here, the written field set matches " +
    "more than one member and the field values' types don't disambiguate further — add a more " +
    "specific field value, or rename fields so each member's shape is unique.",
  "discriminated-union-tag-required":
    "A union with 2+ anonymous '{...}' struct members (constructed via the union's own name, e.g. " +
    "'MyUnion{...}') must have a tag field: one field name present in every member, with a distinct " +
    "string-literal type in each (e.g. kind: \"ok\" / kind: \"notFound\") — F-7. Without one, " +
    "constructing a member by field set alone is fragile: adding a member elsewhere with an " +
    "overlapping field set could silently make an existing literal ambiguous or resolve to the wrong " +
    "member. Add a tag field, like the well-known 'kind' convention, with a unique literal value per " +
    "member. (This doesn't apply to a union of independently named structs, like 'Circle | Square' " +
    "— each already has its own unambiguous name to construct by.)",
  "discriminated-union-tag-missing":
    "This discriminated union's struct literal must set its tag field to a string-literal value so " +
    "the member can be identified (F-7) — e.g. write 'kind: \"ok\"', not a computed or non-literal " +
    "expression for that field.",
  "method-field-conflict":
    "A method declaration's name is already used by a field on the same struct — methods and fields " +
    "share one namespace on a given struct.",
  "duplicate-method":
    "A method with this name is already declared for this struct type.",
  "invalid-receiver-type":
    "A method's receiver ('fn (x: T) name(...)') must be a struct type; primitives, unions, arrays, " +
    "etc. can't have methods.",
  "narrow-required":
    "The value's type is still a union — you must narrow it first (with 'if x is ...', or 'match') " +
    "before accessing a field or calling a method on it, since not every member of the union has that " +
    "field.",
  "union-required":
    "'is' and 'match' only make sense on a union-typed value (that's what they narrow/decompose) — " +
    "the subject here already has a single concrete type.",
  "impossible-pattern":
    "This 'is'/match pattern can't match any member of the subject's union type — it's checking for a " +
    "shape or type that the union never contains.",
  "unreachable-pattern":
    "This pattern (or a '_' after it) is already fully covered by an earlier arm/check, so it can never " +
    "run — likely a leftover from editing, or a duplicate case.",
  "wildcard-not-alone":
    "'_' (the catch-all wildcard) must be the only pattern in its arm — it can't be combined with " +
    "other patterns using ','.",
  "empty-match":
    "A 'match' expression needs at least one arm.",
  "mixed-void-arms":
    "Some arms of this match/select return a value and others don't (are void) — every arm must " +
    "either return a value, or none of them should.",
  "match-not-exhaustive":
    "This 'match' doesn't cover every member of the subject's union type — add arms for the missing " +
    "ones (listed in the message), or a final '_' arm to cover the rest.",
  "empty-select":
    "A 'select' expression needs at least one channel arm, or a default ('_') arm.",
  "prop-context-not-string":
    "'?' with context (f() ? \"...\") requires the context to be a string literal (interpolation is " +
    "fine) — it becomes part of the propagated error message.",
  "prop-requires-failure-union":
    "'?' only works on a union that can contain none/error/a declared error type — the expression here " +
    "isn't such a union.",
  "prop-nothing-to-propagate":
    "The union has no none/error/error-type member, so there's nothing for '?' to propagate — every " +
    "member is already a success type.",
  "prop-context-structured-error":
    "'?' with context always converts the failure into a message string, but a structured error type " +
    "(declared with 'error type'/'error struct') doesn't have a message to convert. Use plain '?' (no " +
    "context) to propagate it as-is, or handle it with 'is'/'match' first.",
  "prop-return-type-mismatch":
    "'?' propagates a failure member up to the caller, but the enclosing function's declared return " +
    "type doesn't include that failure type — add it to the return type, or handle the failure locally " +
    "with 'is' instead of propagating it.",
  "or-never-fails":
    "The left side of 'or' has a type that can never be a failure (no none/error/error-type member), " +
    "so the fallback on the right can never run — 'or' isn't needed here.",
  "or-requires-binding":
    "The left side of 'or' can fail with something other than plain 'none' (an error or a structured " +
    "error type), so the bound form is required: 'or e => ...' to use the failure value, or " +
    "'or _ => ...' to discard it explicitly (so the discard is visible and greppable).",
  "or-no-success-value":
    "After removing every failure member, the left side of 'or' has no success type left — there's " +
    "nothing for the fallback to produce a value alongside. Handle it with 'is' instead.",
  "or-fallback-type-mismatch":
    "The 'or' fallback expression's type doesn't match the left side's success type.",
  "generic-type-param-conflict":
    "A generic function's type parameter name collides with a builtin type name, an existing 'type' " +
    "declaration, or another type parameter of the same function — each type parameter name must be " +
    "unique and unused elsewhere.",
  "generic-type-param-not-inferable":
    "A type parameter must appear in at least one parameter type (not just the return type), because " +
    "call sites infer type parameters purely from argument types — there's no explicit instantiation " +
    "syntax like 'f<int>(...)'.",
  "generic-inference-failed":
    "The compiler couldn't work out a concrete type for one or more of this generic function's type " +
    "parameters from the arguments actually passed at this call site.",
  "not-indexable":
    "The '[...]' indexing operator only works on arrays, maps, and strings — this value's type doesn't " +
    "support it.",
  "invalid-index-type":
    "An array or string index must be an 'int'.",
  "not-a-channel":
    "This operation (receive '<-', 'select' arm) requires a 'chan<T>' value, but the expression's type " +
    "isn't a channel.",
  "not-rangeable":
    "'for ... := range X' only works over an array, a map, or an int (counting 0..n) — this type isn't " +
    "one of those.",
  "range-arity":
    "The number of loop variable names doesn't match what this kind of range produces (two names for " +
    "array/map, one for an int range) — use '_' to explicitly ignore one if you don't need it.",
  "invalid-package-name":
    "The last segment of an import path must be a valid identifier, since it becomes the qualifier " +
    "used for pkg.symbol access — rename the package directory if it isn't.",
  "self-import":
    "A package can't import itself.",
  "import-cycle":
    "Two or more packages import each other, forming a cycle — Mesh requires imports to form a DAG so " +
    "packages can be checked in dependency order.",
  "syntax-error":
    "The parser expected a specific token or construct at this position and found something else — " +
    "check the surrounding syntax against the language card.",
  "import-order":
    "All 'import' declarations must appear before any other top-level declaration in the file.",
  "invalid-top-level-declaration":
    "Only 'import', 'fn', 'struct', 'type' (optionally preceded by 'export' and/or 'error'), and " +
    "top-level constants ('name := value', F-9c) are allowed at the top level of a file.",
  "top-level-mut-not-allowed":
    "Top-level bindings are always immutable, so 'mut' can't be used on one (F-9c). Mesh has no " +
    "mutable globals — if you need shared mutable state, pass it as a parameter instead.",
  "chan-capacity-required":
    "'chan<T>()' no longer defaults to an unbounded buffer (F-11) — an unbounded channel let a " +
    "detached background task leak memory forever with no syntactic warning sign, unlike a leaked " +
    "goroutine (which the 2-tier ownership design already makes impossible). Write 'chan<T>(none)' " +
    "to still choose an unbounded channel explicitly, or 'chan<T>(n)' for one that blocks sends " +
    "once n values are buffered.",
  "invalid-test-signature":
    "F-15: a function named 'test...' inside a '_test.mesh' file is treated as a test by 'mesh " +
    "test', and must take no parameters and return exactly 'none | error' — 'none' means it " +
    "passed, 'error' means it failed (reusing the existing absence/failure vocabulary instead of " +
    "adding a new pass/fail concept). Rename it if it's a helper function, not a test itself.",
  "invalid-import-path":
    "The import path string is invalid — it can't be empty, and it can't use string interpolation " +
    "(it must be a plain string literal).",
  "bare-struct-shape":
    "A bare '{ field: Type, ... }' shape was written outside of a union — that syntax is only valid as " +
    "a member of a 'type X = { ... } | { ... }' discriminated union. For a standalone data shape, use " +
    "'struct X { ... }' instead.",
  "method-export-redundant":
    "A method's visibility follows its struct's visibility, so 'export' on the method itself is " +
    "meaningless — export the struct instead (methods are then visible wherever the struct is).",
  "multiple-return-values-removed":
    "Go-style multiple return values ('return a, b') were removed from Mesh — return a single value, " +
    "using a union type (like 'T | error') if the function can fail or return one of several shapes.",
  "interpolation-in-type":
    "String interpolation ('${...}') can't be used inside a type annotation — type positions only " +
    "accept literal type syntax.",
  "fn-type-with-param-names":
    "A function type annotation ('fn(...) T') only lists parameter TYPES, not names — remove the " +
    "parameter name and colon, keeping just the type.",
  "invalid-assignment-target":
    "The left-hand side of ':=' or '=' isn't something that can be assigned to (':=' requires a plain " +
    "name; '=' allows a name, index expression, or field access).",
  "misplaced-mut":
    "'mut' can only be used directly on a ':=' short variable declaration, not anywhere else.",
  "invalid-spawn-target":
    "'spawn'/'detach' must be immediately followed by a function call expression — they start a call " +
    "concurrently, so there has to be a call there to start.",
  "multiple-select-defaults":
    "A 'select' expression can have at most one default ('_') arm.",
  "postfix-bang-renamed":
    "Postfix '!' for none/error propagation was renamed to '?' (to match Rust's identical operator, " +
    "and to stop colliding with force-unwrap/non-null-assertion meanings from other languages). Replace " +
    "'!' with '?' at the end of the expression.",
  "empty-typed-array-literal-removed":
    "Empty typed array literals ('Todo[]{}') were removed — they duplicated the plain empty array " +
    "'[]', which already becomes the right type wherever one is expected (a ':'-annotated declaration " +
    "or a function's declared return type). Write 'xs: Todo[] = []' instead (non-empty typed array " +
    "literals like 'Todo[]{a, b}' are unaffected).",
  "unterminated-string":
    "A string literal was opened with '\"' but the line ended (or the file ended) before a matching " +
    "closing '\"' was found.",
  "unknown-escape":
    "The backslash escape sequence inside this string isn't recognized.",
  "unterminated-interpolation":
    "A '${' interpolation inside a string was opened but never closed with a matching '}'.",
  "empty-interpolation":
    "A '${}' interpolation has nothing inside it — put an expression between the braces, or remove it.",
  "unexpected-character":
    "The lexer found a character that isn't part of any valid Mesh token.",
};
