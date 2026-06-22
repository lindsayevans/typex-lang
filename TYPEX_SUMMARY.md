# TypeX Language — Project Summary

## Overview

TypeX is a strictly typed programming language loosely based on TypeScript, with Rust-inspired error handling and semantics. It supports both direct script execution via an interpreter and compilation to native binaries.

- **File extension:** `.tx`
- **Goals:** General systems programming, scripting, eventual self-hosting
- **Toolchain:** `tx run`, `tx build`, `tx check`, `tx ast`, `tx repl`

---

## Language Spec — v1

### Primitive Types

- `char` — 32-bit Unicode Scalar Value (as per Rust)
- `string` — UTF-8, no index access, iteration via `for (let {index, offset, value} in str)`
- Numerics: `int8`/`uint8` through `int64`/`uint64`, `float32`/`float64`
  - Aliases: `int`=`int64`, `uint`=`uint64`, `float`=`float64`
- `boolean`, `null`
- No `Number`, no `undefined`

### Built-in Types

- `Array<T>` — with `filter`, `map`, `equals` etc.
- `Result<T, E>` — for error handling, no try/catch
- `Date`, `Time`, `DateTime` — UTC-only in v1 (Temporal-inspired)
- `Record<K, V>` — typed key-value map

### Error Handling

```tx
function divide(a: int, b: int): Result<int, string> {
    if (b == 0) {
        return Err("division by zero");
    }
    return Ok(a / b);
}

function main(): int {
    match divide(10, 2) {
        Ok(n) => println("result: {}", n),
        Err(e) => println("error: {}", e),
    }
    return 0;
}
```

### Enums

```tx
// Auto-numbered
enum Flags { Read, Write }         // 0, 1

// Sparse numeric
enum Options {
    Optimised,
    Strict = 7,
    Debug,                          // 8
    DryRun = 77,
}

// String-valued (bidirectional lookup)
enum Colour {
    Red = "#f00",
    Green = "#0f0",
    Blue = "#00f",
}
```

### Variables

```tx
let x: int = 0;        // mutable
const y: int = 42;     // immutable
x = x + 1;            // reassignment
```

### Functions

```tx
// Named
function add(a: int, b: int): int { return a + b; }

// Arrow - block body
const add = (a: int, b: int): int => { return a + b; };

// Arrow - expression body
const add = (a: int, b: int): int => a + b;
```

### Control Flow

```tx
// if/else
if (x == 0) { ... } else if (x == 1) { ... } else { ... }

// switch (primitive dispatch)
switch (n) {
    case 1:
        return 100;
    case 2:
        return 200;
    default:
        return 0;
}

// match (Result/enum exhaustive matching)
match result {
    Ok(val) => val,
    Err(e)  => panic("error: {}", e),
}

// ternary
const status: string = age >= 18 ? "Adult" : "Minor";
```

### Loops

```tx
// Array iteration
for (let {index, value} in array) { ... }
for (let {value} in array) { ... }

// Object iteration (order not guaranteed)
for (let {key, value} of object) { ... }

// String iteration
for (let {index, offset, value} in str) { ... }
```

### Modules

```tx
// Named exports only, no default export
export function fibonacci(n: int): int { ... }

// File imports
import { fibonacci } from "./fibonacci.tx";

// Stdlib imports
import { readFile, writeFile } from "tx:fs";
import { sqrt, abs } from "tx:math";
import { exec } from "tx:process";
```

### Generics

```tx
function identity<T>(x: T): T { return x; }
type Pair<A, B> = { first: A, second: B };
```

### Union Types

```tx
type Status = 'pending' | 'approved' | 'rejected';
let x: string | null = null;
```

### Destructuring

```tx
let [first, second] = getThings();
let {foo, bar} = getThing();
```

### Builtins

```tx
print("hello {} {}", 1, "world");
println("hello {name}", name);   // named interpolation
panic("something went wrong: {}", e.message);
```

### Entrypoint

```tx
function main(argv: Array<string>): int {
    // argv[0] = script path
    return 0;
}
```

### Semicolons

Required to end expressions.

### Equality

Strict only (`==`), reference equality for complex types. Use `Array.equals()` for value comparison.

---

## Language Spec — v2 (Planned)

- Full OOP: classes, public/private, extends, interfaces/implements
- async/await
- Nullish coalescing (`??`), optional chaining (`?.`)
- Network stdlib: `tx:net`, DNS, HTTP
- Cryptography, advanced math, RegExp
- Testing, linting, formatting, package management
- `tx fmt`, `tx lint`, `tx test`, `tx new`, `tx add`
- LSP (`typex_lsp`)

---

## Crate Architecture

```
typex_span        — source locations, spans, diagnostics, error rendering
typex_ast         — AST node types (no logic)
typex_lexer       — tokenizer
typex_parser      — recursive descent parser
typex_resolve     — name resolution, scope analysis
typex_typecheck   — explicit type checking (Option A: no inference)
typex_runtime     — Value enum, format_string, RuntimeError
typex_vm          — tree-walking interpreter, module loader
typex_std         — standard library (tx:fs, tx:io, tx:math, tx:process)
typex_codegen     — Cranelift native compiler
typex_hir         — (planned) typed HIR
typex_ir          — (planned) compiler IR
typex_cli         — tx binary (ast/check/run/build/repl commands)
```

### Dependency Graph

```
typex_span
  └─ typex_ast
       └─ typex_lexer
       └─ typex_parser
            └─ typex_resolve
                 └─ typex_typecheck
                      └─ typex_hir (planned)
                           └─ typex_ir (planned)
                                └─ typex_vm
                                └─ typex_codegen
typex_runtime  (used by typex_vm + typex_codegen)
typex_std      (built on typex_runtime)
typex_cli      (depends on everything)
```

---

## Implementation Status

### typex_span ✅

- `Pos` — line, col, byte offset
- `Span` — start/end pos + FileId
- `SourceMap` — file registry, source text, snippet extraction
- `Diagnostic` — error/warning/note with span
- `render_diagnostic()` — Rust-style error output with `^^^` pointer

### typex_ast ✅

- `Module`, `Item`, `FunctionDef` (with `exported: bool`)
- `TypeExpr` — Named, Generic, Union, Nullable
- `EnumDef`, `EnumVariant`, `EnumValue`
- `Import`, `Export`
- `Stmt` — Let, Const, Return, If, Switch, For, Match, Expr
- `Expr` — Lit, Ident, BinOp, UnaryOp, Call, Field, Index, Ternary, Match, Arrow, Array, Record, Destructure, Assign
- `ForStmt` — Array, Object, Str variants
- `Pattern` — Ok, Err, EnumVariant, Wildcard

### typex_lexer ✅

- All TypeX tokens including keywords, operators, literals
- UTF-8 aware, tracks line/col/offset
- Line comments (`//`)
- Error recovery with diagnostics

### typex_parser ✅

- Full recursive descent parser
- Operator precedence: ternary → or → and → equality → comparison → additive → multiplicative → unary → postfix → primary
- `export function` syntax
- File imports (`./foo.tx`) and stdlib imports (`tx:fs`)
- Switch without braces per case
- Match expressions (inline and statement)
- Arrow functions (block and expression body)
- Destructuring

### typex_resolve ✅

- Two-pass: hoist declarations, then resolve bodies
- Builtin types and functions pre-registered
- Scope stack, undefined variable detection
- Duplicate declaration detection
- Export validation
- Match arm binding scoping

### typex_typecheck ✅

- Explicit types only (Option A — no inference)
- `Ty` enum mirrors TypeX type system
- Function signature hoisting
- `Result<T, E>` Ok/Err constructor type inference from return context
- Numeric widening (int8 assignable to int64)
- Boolean condition enforcement
- Match arm type consistency
- Warning for always-unequal comparisons

### typex_runtime ✅

- `Value` enum: Int, Uint, Float, Bool, Char, Str, Null, Void, Array, Record, Ok, Err, Fn
- `format_string()` — positional `{}`, indexed `{42}`, named `{ident}`
- `RuntimeError` with `RuntimeErrorKind`: Return(Value), Panic(String), Error(String)
- `RuntimeResult<T>` type alias

### typex_vm ✅

- Tree-walking AST interpreter
- Scoped environment with save/restore for function calls (fixes recursive variable pollution)
- Stdlib dispatch via `StdRegistry`
- File-based module loading (relative path resolution)
- Builtins: print, println, panic, Ok, Err
- Method calls: Array.length, Array.equals, Str.length
- String concatenation with `+`
- Full control flow: if/else, switch, for (array/object/string), match
- Variable reassignment via `Env::set()`

### typex_std ✅

- `tx:fs` — readFile, writeFile, exists, deleteFile
- `tx:io` — readLine, readLines
- `tx:math` — sqrt, abs, pow, floor, ceil, round, min, max, clamp
- `tx:process` — exec, exit

### typex_codegen ✅ (ARM64 macOS)

- Cranelift-based native compiler
- Object file emission + `cc` linking
- `txruntime.c` shim for non-variadic printf (`tx_print_int`, `tx_puts`)
- Supported: int/float/bool literals, arithmetic, comparisons, logical ops
- Supported: function calls, recursion, if/else, switch/case, for...in arrays
- Supported: variable declaration (let/const), variable reassignment
- Supported: ternary expressions, array literals (stack-allocated int arrays)
- Not yet: match/Result, string variables, multi-arg println, for...of objects
- PIC enabled (`is_pic = true`), `AppleAarch64` calling convention

### typex_cli ✅

- `tx ast <file>` — parse and pretty-print AST
- `tx check <file>` — parse + resolve + typecheck, Rust-style errors
- `tx run <file> [args]` — interpreter with full pipeline
- `tx build <file>` — native compiler, outputs `file` (no extension on macOS/Linux)
- `tx repl` — interactive session with persistent state, `:help`, `:history`, `:clear`
- `tx fmt/lint/test/new/add` — planned v2

---

## Key Design Decisions

**Error handling:** `Result<T, E>` + `match`, no try/catch. `panic()` for unrecoverable errors.

**Null safety:** Non-nullable by default. Opt-in via `string | null`.

**No `undefined`:** Dropped in favour of `null` only.

**Type inference:** None in v1 — every `let`/`const` requires explicit annotation.

**Equality:** Strict only (`==`). Reference equality for complex types.

**String iteration:** Yields `{index: uint, offset: uint, value: string}` where index = codepoint index, offset = byte offset.

**Enum reverse lookup:** String enums only (`Colour["#f00"] == Colour.Red`).

**Return signal:** `RuntimeErrorKind::Return(Value)` — avoids string serialization of return values, correctly handles `Ok`/`Err` compound values across recursive calls.

**Function call isolation:** Each function call saves/restores the entire `Env` to prevent recursive calls polluting outer scope variables.

**Codegen printf:** Uses `tx_print_int(fmt, n)` C shim instead of variadic `printf` — ARM64 macOS ABI treats variadic args differently, causing pointer values to be passed instead of integers.

**Module system:** File imports resolved relative to the calling file's directory. `exported: bool` on `FunctionDef`.

---

## Working Example Programs

```tx
// fibonacci.tx
function fibonacci(n: int): int {
    if (n <= 0) { return 0; }
    if (n == 1) { return 1; }
    const a: int = fibonacci(n - 1);
    const b: int = fibonacci(n - 2);
    return a + b;
}

// ping.tx
import { exec } from "tx:process";

function main(argv: Array<string>): int {
    const host: string = argv[1];
    const cmd: string = "ping -c 4 " + host;
    const result: string = match exec(cmd) {
        Ok(output) => output,
        Err(e) => "error",
    };
    println(result);
    return 0;
}
```

---

## Toolchain Commands

```sh
tx ast <file>              # print AST
tx check <file>            # typecheck only
tx run <file> [args]       # interpret
tx build <file>            # compile to native binary
tx build <file> --target=<triple>  # cross-compile (planned)
tx repl                    # interactive session
```

### Target Triples (v1)

| Target Triple               | Platform            | Output    |
| --------------------------- | ------------------- | --------- |
| `x86_64-unknown-linux-musl` | Linux x86_64        | `foo`     |
| `x86_64-apple-darwin`       | macOS Intel         | `foo`     |
| `aarch64-apple-darwin`      | macOS Apple Silicon | `foo`     |
| `x86_64-pc-windows-msvc`    | Windows x86_64      | `foo.exe` |

---

## VSCode Extension

Located at `vscode-typex/`. Install via:

```bash
cp -r vscode-typex ~/.vscode/extensions/vscode-typex
```

Provides syntax highlighting for `.tx` files including keywords, types, strings with `{}` interpolation highlighting, operators, and comments.

---

## What's Next

**High priority:**

- `match`/`Result` in codegen
- String support in codegen (pointer-based, `txruntime.c` helpers)
- Multi-arg `println` in codegen
- Cross-compilation target support

**Medium priority:**

- `typex_hir` — typed HIR layer between AST and codegen
- `typex_ir` — compiler IR
- More stdlib: `tx:env`, `tx:time`
- `for...of` objects in VM
- Better REPL (multi-line input, arrow key history)

**v2:**

- OOP (classes, interfaces, extends)
- async/await
- Network stdlib
- Package management
- LSP
- Self-hosting
