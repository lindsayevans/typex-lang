# TypeX Standard Library Reference

The TypeX standard library is imported using the `tx:` prefix:

```tx
import { readFile, writeFile } from "tx:fs";
import { sqrt, abs } from "tx:math";
import { readLine } from "tx:io";
import { exec, exit } from "tx:process";
```

All fallible functions return `Result<T, string>` and should be handled with `match`.

---

## tx:fs

Filesystem operations.

```tx
import { readFile, writeFile, exists, deleteFile } from "tx:fs";
```

### readFile

Reads the contents of a file as a string.

```tx
readFile(path: string): Result<string, string>
```

```tx
const content: string = match readFile("hello.txt") {
    Ok(s)  => s,
    Err(e) => panic("failed to read file: {}", e),
};
println("{}", content);
```

---

### writeFile

Writes a string to a file, creating it if it doesn't exist.

```tx
writeFile(path: string, content: string): Result<null, string>
```

```tx
match writeFile("output.txt", "Hello, TypeX!") {
    Ok(n)  => println("wrote file ok"),
    Err(e) => println("write failed: {}", e),
}
```

---

### exists

Returns `true` if a file or directory exists at the given path.

```tx
exists(path: string): boolean
```

```tx
const found: boolean = exists("config.txt");
if (found) {
    println("config found");
} else {
    println("config missing");
}
```

---

### deleteFile

Deletes a file at the given path.

```tx
deleteFile(path: string): Result<null, string>
```

```tx
match deleteFile("temp.txt") {
    Ok(n)  => println("deleted"),
    Err(e) => println("delete failed: {}", e),
}
```

---

## tx:io

Input/output operations.

```tx
import { readLine, readLines } from "tx:io";
```

### readLine

Reads a single line from stdin. Accepts an optional prompt string.

```tx
readLine(): Result<string, string>
readLine(prompt: string): Result<string, string>
```

```tx
const name: string = match readLine("Enter your name: ") {
    Ok(s)  => s,
    Err(e) => panic("read error: {}", e),
};
println("Hello, {}!", name);
```

---

### readLines

Reads all lines from stdin until EOF, returning them as an array of strings.

```tx
readLines(): Array<string>
```

```tx
const lines: Array<string> = readLines();
println("read {} lines", lines.length);
```

---

## tx:math

Mathematical functions.

```tx
import { sqrt, abs, pow, floor, ceil, round, min, max, clamp } from "tx:math";
```

### sqrt

Returns the square root of a number. Returns `Err` for negative inputs.

```tx
sqrt(n: float): Result<float, string>
```

```tx
const root: float = match sqrt(144.0) {
    Ok(n)  => n,
    Err(e) => panic("sqrt error: {}", e),
};
println("sqrt(144) = {}", root); // 12
```

---

### abs

Returns the absolute value of a number.

```tx
abs(n: int): int
abs(n: float): float
```

```tx
println("{}", abs(-42));    // 42
println("{}", abs(-3.14));  // 3.14
```

---

### pow

Returns `base` raised to the power of `exp`.

```tx
pow(base: float, exp: float): float
```

```tx
println("{}", pow(2.0, 10.0)); // 1024
```

---

### floor

Returns the largest integer less than or equal to `n`.

```tx
floor(n: float): int
```

```tx
println("{}", floor(3.9)); // 3
println("{}", floor(-1.1)); // -2
```

---

### ceil

Returns the smallest integer greater than or equal to `n`.

```tx
ceil(n: float): int
```

```tx
println("{}", ceil(3.1)); // 4
println("{}", ceil(-1.9)); // -1
```

---

### round

Returns `n` rounded to the nearest integer. Rounds half up.

```tx
round(n: float): int
```

```tx
println("{}", round(3.5)); // 4
println("{}", round(3.4)); // 3
```

---

### min

Returns the smaller of two values.

```tx
min(a: int, b: int): int
min(a: float, b: float): float
```

```tx
println("{}", min(10, 20)); // 10
println("{}", min(3.14, 2.71)); // 2.71
```

---

### max

Returns the larger of two values.

```tx
max(a: int, b: int): int
max(a: float, b: float): float
```

```tx
println("{}", max(10, 20)); // 20
println("{}", max(3.14, 2.71)); // 3.14
```

---

### clamp

Clamps a value between a minimum and maximum.

```tx
clamp(n: int, lo: int, hi: int): int
clamp(n: float, lo: float, hi: float): float
```

```tx
println("{}", clamp(15, 0, 10));  // 10
println("{}", clamp(-5, 0, 10));  // 0
println("{}", clamp(5, 0, 10));   // 5
```

---

## tx:process

Process and system operations.

```tx
import { exec, exit } from "tx:process";
```

### exec

Executes a shell command and returns its combined stdout/stderr output.
Returns `Err` if the command fails or returns a non-zero exit code.

```tx
exec(cmd: string): Result<string, string>
```

```tx
const output: string = match exec("ls -la") {
    Ok(s)  => s,
    Err(e) => panic("exec failed: {}", e),
};
println("{}", output);
```

```tx
// ping example
const result: string = match exec("ping -c 4 google.com") {
    Ok(s)  => s,
    Err(e) => "ping failed",
};
println("{}", result);
```

---

### exit

Exits the process with the given exit code.

```tx
exit(code: int): void
```

```tx
const ok: boolean = exists("required_file.txt");
if (!ok) {
    println("required_file.txt not found");
    exit(1);
}
```

---

## Planned (v2)

The following modules are planned for v2:

### tx:net

DNS lookups, TCP/UDP sockets, HTTP client.

### tx:time

Current time, sleep, duration arithmetic, timezone support.

### tx:env

Environment variable access (`getenv`, `setenv`).

### tx:crypto

Hashing (SHA-256, MD5), HMAC, base64 encode/decode.

### tx:math (extended)

Trigonometric functions, logarithms, constants (`PI`, `E`).

### tx:re

Regular expression matching and substitution.
