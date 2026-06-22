# TypeX language

> Vibe coding my own interpreter/compiler, because YOLO

A strictly typed language loosely based on TypeScript, with some nice bits from Rust

Goals:
v1: safe native scripting/application language
v2: async, OOP, richer standard library
v3: systems layer: FFI, explicit resource management, unsafe blocks, low-level memory/control primitives
v4: self-hosting

File extension: .tx

## v1

- primitive types
  - `char` (32-bit Unicode Scalar Value as per Rust)
  - `string` - UTF-8
    - no index access on strings, access/modification functions in standard library
    - built in iteration: `for (let {index, value} in str) { println("[{index}]: {value}");}`
      - can also access the byte offset: `for (let {index, offset, value} in str) { ... }`
      - index: uint
      - offset: uint
      - value: string
  - numerics: `int8`/`uint8` ... 16, 32, ... `int64`/`uint64`, `float32`/`float64`
    - aliases: uint=uint64, int=int64, float=float64
  - `boolean`
  - `null` ((non-nullable by default, explicit opt-in)) - e.g. `let x: string | null = null`
  - no Number, undefined
- proper `Date`, `Time`, `DateTime` types ala JS Temporal
  - Simplified UTC only API in v1, more comprehensive API with timezone support etc. in later versions
- `Array` type - builtin methods for filter, map, etc.
- exception handling - Result<T, E> return types - no try/catch/etc.
  - `match` keyword branching as per Rust:

  ```tx
    const greetingFile = fs.read("hello.txt");
    const greeting = match greetingFile {
        Ok(file) => file.content,
        Err(e) => panic("Problem opening the file: {}", e.message),
    };
  ```

  or inlined:

  ```tx
    const greeting = match fs.read("hello.txt") {
        Ok(file) => file.content,
        Err(error) => panic("Problem opening the file: {}", error.message),
    };
  ```

  - full example:

  ```tx
  function divide(numerator: int, denominator: int): Result<int, string> {
    if (denominator == 0) {
        return Err("Cannot divide by zero!");
    }

    return Ok(numerator / denominator);
  }

  function main() {
    match divide(10, 0) {
        Ok(result) => println("Success! The answer is: {result}"),
        Err(error) => println("Error occurred: {error}"),
    }
  }
  ```

- Record type: `Record<K, V>`
  - K must be a hashable primitive type (string, numeric, enum)
  - V can be any type
  - e.g.
  ```tx
  const thing: Record<string, string | uint> = {
      name: "Zaphod Beeblebrox",
      age: 42,
  }
  ```
- enums

* Rust style with associated data (numeric, char, string)
  - numeric enums auto-increment from 0 by default, or from the last explicit value
  - reverse lookup via [] is supported (on string enums only)
  - e.g.

  ```tx
  enum Flags {
      Read, Write
  }

  Flags.Read // 0
  Flags.Write // 1

  enum Options {
      Optimised,
      Strict = 7,
      Debug,
      DryRun = 77,
  }
  Options.Optimised // 0
  Options.Strict // 7
  Options.Debug // 8
  Options.DryRun // 77

  enum Colours {
      Red = "#f00",
      Green = "#0f0",
      Blue = "#00f",
  }

  print("red: {}", Colours.Red); // Prints: "red: #f00"
  Colours["#0f0"] == Colours.Green // true
  Colours["#00f"] == Colours.Red // false

  ```

- union types - `type Status = 'pending' | 'approved' | 'rejected';`, `string | null;`
- same `type Foo = {...}` as TS
- ternary operator: `const status = age >= 18 ? "Adult" : "Minor";`
- semicolons are required to end expressions
- let (mutable), const (immutable) - no var
- named exports/imports, no default export
- named functions: `function foo(n: int32): boolean {return true;}`
- anonymous/arrow functions: `const foo = (n: int32): boolean => {return true;}`
  - also return type when body is an expression: `const foo = (n: int32): boolean => n == 42;`
- basic generics
- basic array & object destructuring:
  - `let [first, second] = getThings()`
  - `let {foo, bar} = getThing()`
- strict equality only (==)
  - reference equality for more complex types - standard library may include a deep equality method at some point
  - arrays use reference equality; use Array.equals() for value comparison
- basic control flow - if/else/else if, switch/case/etc.
  - match for Result/enum exhaustive matching; switch for primitive value dispatch.
- basic looping - `for (let {index, value} in array)`, `for (let {key, value} of object){...}`, `for (let {key, value} of record){...}`,
  - key/index/value destructuring - not everything is required - e.g. `for (let {value} in array)`, `for (let {index} in array)`, `for (let x of object){ print(x.key); }`
  - for object loops: iteration order is not guaranteed, undefined fields are skipped
- builtins inspired by Rust:
  - print with formatting: `print("hi {} {} {}, {name}", 1, "two", 3.1415)` + `println`
    - String formatting can use positional or named interpolation, or mix both: `print("Hello {name}{}", '!')`
  - panic: `panic("message: {err.message}")`
  - `quit()`, `quit(1)` - REPL only: exits to shell
- entrypoint: if there is a `function main(argv: Array<string>): int {...}`, it gets called with arguments
  - e.g. `txs foo.tx 1 2 3` could equate to `main(["foo.tx", "1", "2", "3"])`
  - if there is no return, assume success
  - if there is a panic, exit with a failure exit code

## v2

- Full OOP: classes, proper usage of public/private, extends, interfaces/implements
- async/await
- nullish coalescing, Optional Chaining Operator

# TypeX implementations

## Core Libraries (separate Rust crates)

- Rust based, shared libraries for lexer/parser/interpreter

## Standard Libraries

- Rust based standard libraries for use in TypeX scripts
- v1: IO, Filesystem, basic math
- v2: network, DNS, HTTP, etc.; cryptography, advanced math, RegExp, JSON serialise/deserialise, testing/linting/formatting/package mgmnt

## Interpreter/REPL

- Rust based, main command line interface uses shared libraries for lexer/parser/interpreter
- Focused on fast developer iteration, small utility scripts etc. - cross platform
  e.g.

```sh
tx run foo.tx # executes `foo.tx` script
# or use txs alias:
txs foo.tx # executes `foo.tx` script
```

## Compiler

- Rust based, main command line interface uses shared libraries for lexer/parser
- Focused on native performance, outputs native binaries for a select group of platforms (linux ELF, windows exe, MacOS - x86_64, aarch64) using Cranelift
  e.g.

```sh
tx build foo.tx # compiles foo.tx to binary `foo[.exe]` for current platform
# or use txc alias:
txc foo.tx
```

## TypeX toolchain

```sh
tx run # starts REPL session
tx run foo.tx # executes script foo.tx
tx build foo.tx # compiles native binary
# v2:
tx fmt # formats .tx files according to (TBD)
tx lint # lints .tx files according to (TBD)
tx test # runs tests
tx new foo # scaffold a new project
tx add <pkg> # package management
```

## Targets

v1: host platform only

Cross compilation in future versions

| Target Triple                | Platform | Architecture          | OS/ABI        | Output    |
| ---------------------------- | -------- | --------------------- | ------------- | --------- |
| `x86_64-unknown-linux-gnu`   | Linux    | x86_64                | glibc         | `foo`     |
| `x86_64-unknown-linux-musl`  | Linux    | x86_64                | musl (static) | `foo`     |
| `aarch64-unknown-linux-gnu`  | Linux    | ARM64                 | glibc         | `foo`     |
| `aarch64-unknown-linux-musl` | Linux    | ARM64                 | musl (static) | `foo`     |
| `x86_64-pc-windows-msvc`     | Windows  | x86_64                | MSVC          | `foo.exe` |
| `aarch64-pc-windows-msvc`    | Windows  | ARM64                 | MSVC          | `foo.exe` |
| `x86_64-apple-darwin`        | macOS    | x86_64 (Intel)        | Darwin        | `foo`     |
| `aarch64-apple-darwin`       | macOS    | ARM64 (Apple Silicon) | Darwin        | `foo`     |

# Rust Crates

typex_lexer - lexing
typex_parser - parsing
typex_ast - abstract syntax tree
typex_span - source locations, diagnostics
typex_resolve - modules, imports, names
typex_typecheck - type checking
typex_hir - lowered typed representation
typex_ir - compiler/interpreter IR
typex_vm - bytecode interpreter, execution loop, stack
typex_runtime - memory model, built-in types (Array, string etc.), GC/ref-counting
typex_std - standard library implementation
typex_cli - CLI tools: tx, txs, txc
typex_codegen - binary code generation

v2:
typex_lsp - language server
