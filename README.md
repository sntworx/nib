# NIB

![nib](.github/assets/nib-logo.svg)

> **Active development.** APIs, syntax, and behavior may change without notice. Not recommended for production use yet.

## What's nib

`nib` is a small custom scripting language written in Rust, meant to be embedded inside a host application rather than run standalone. C-like syntax (`var`, `if`/`else`, `while`, `for`, top-level `func`s with no closures, arrays, etc.).

Nothing is pre-bound by default: the host decides exactly which native functions a script is allowed to call (`register_func`), and can even strip specific keywords out of the language for a given script (`disable_keywords`) — e.g. dropping `while`/`for` to rule out unbounded loops. That opt-in-only surface makes it a fit for running untrusted or user-authored logic inside a larger app: plugin scripting, rules/workflow engines, user-defined formulas, that kind of thing — where you want scripts to only ever touch what you explicitly exposed.

For shared logic that's easier to write in `nib` itself than as native Rust/PHP/JS functions, a host can also `include()` its own nib-authored library code — or the prepared `array`/`string`/`math` libraries in [`stdlib/`](stdlib/) — before running the main script; top-level `func`s from an include land in the same global scope the main script runs in, so it can call them directly. This is separate from `register_func`: it doesn't need the host to touch its own language at all, and included code stays subject to the same "no closures, top-level `func` only" rules as any other `nib` script. `include()` itself can't fail — it just queues the source — so a host never needs error handling around the call; parsing and running happen together inside `run()`, so any problem with included code (or the main script) surfaces from that one call.

The same language also reaches multiple host runtimes: a PHP extension (`bindings-php`) and TypeScript/WebAssembly bindings (`bindings-ts`) sit on top of the same core interpreter, so identical `nib` scripts and host-defined behavior can run in a PHP backend and a browser/Node frontend alike.

## Table of contents

- [What's nib](#whats-nib)
- [Syntax](#syntax)
  - [Comments](#comments)
  - [Literals](#literals)
  - [Variables & assignment](#variables--assignment)
  - [Operators](#operators)
  - [Control flow](#control-flow)
  - [Functions](#functions)
  - [Arrays](#arrays)
  - [Maps](#maps)
  - [Strings](#strings)
  - [Numbers](#numbers)
  - [What's not there](#whats-not-there)
- [Workspace layout](#workspace-layout)
- [Standard library](#standard-library)
- [Configuration](#configuration)
- [PHP](#php)
  - [Installation](#installation)
  - [Usage](#usage)
- [JS/TS](#jsts)
  - [Installation](#installation-1)
  - [Usage](#usage-1)
- [License](#license)

## Syntax

### Comments

```
// line comments

/* and
   block comments */
```

### Literals

```
var i = 42;
var f = 3.14;
var s = "line 1\nline 2\ttabbed\\backslash \"quoted\"";
var b = true;
var n = null;
var a = [1, 2, 3];
var m = {name: "Bob", "favorite number": 7};
```

### Variables & assignment

```
var x = 1;
x = 2;
x += 3;   // also -= *= /= %=
x++;      // also x-- and prefix ++x/--x
```

### Operators

```
1 + 2 * 3;
7 % 3;               // remainder, sign follows the dividend (like C/JS, not Python)
(1 + 2) * 3;         // parens are just a grouping expression
"count: " + 5;       // + also stringifies numbers for concatenation
a == b && c != d;    // && and || short-circuit
!done || -x <= 0;    // unary ! and -
```

### Control flow

```
if x > 1 {
    // ...
} else if x == 1 {
    // ...
} else {
    // ...
}
```

```
while x < 10 {
    x++;
}
```

```
for (var i = 0; i < 10; i += 1) {
    if i == 2 { continue; }
    if i == 5 { break; }
}
```

```
for x in [10, 20, 30] {
    print(x);
}
```

```
match x {
    case 1 {
        // ...
    }
    case 2 {
        // ...
    }
    else {
        // ...
    }
}
```

`if`/`while`/`match` conditions don't need parens; C-style `for`'s three clauses do, and each of them is optional (`for (;;) { }` loops forever). `for x in arr { }` never has parens — that's how it's told apart from C-style `for`. `break`/`continue` are only valid inside a loop.

`for x in arr` iterates a value-type array by value: `arr` is evaluated once up front (reassigning it mid-loop doesn't change what's iterated), and `x` is a fresh binding each iteration that doesn't alias back into the array. It's array-only — no direct string or map iteration; use `for c in s.chars() { }` for strings and `for k in m.keys() { }`/`for v in m.values() { }` for maps.

Each `match` arm is `case` followed by a pattern expression (any expression, not just a literal) and its block; arms are tried top-to-bottom and the first whose pattern equals the subject (same equality as `==`) runs, with no fallthrough. `else` is optional and, if present, must be the last arm.

A bare `{ ... }` also works as its own statement — its own scope, not attached to any `if`/`while`/`for`/`func`/`match`.

### Functions

```
func add(a, b) {
    return a + b;
}

add(1, 2);

func noop() {
    return;   // the expression is optional
}
```

Functions are **top-level only** (no nested `func`) and have **no closures** — a function only ever sees globals plus its own params/locals, never the caller's.

### Arrays

```
var matrix = [[1, 2], [3, 4]];
matrix[0][1] = 9;
matrix[0][1] += 1;
```

Arrays are a value type: `var b = a; b[0] = 1;` does **not** change `a`, unlike JS/Python/Ruby.

```
var arr = [1, 2, 3];
arr.push(4);      // -> [1, 2, 3, 4], and writes it back to `arr`
var last = arr.pop();  // -> 4, and writes the shrunk array back to `arr`
arr.len();        // -> 3
```

`.method()` is a small, fixed set of built-in pseudo-methods on arrays, maps, and strings — not general member access or user-extensible dispatch. `push`/`pop` write their result back to wherever the receiver came from (a variable or a nested index, e.g. `matrix[0].push(x)`), same as `arr[i] = x` does; calling one on something that isn't a variable or index (like a bare function call's return value) fails the same way index-assignment into a temporary already does.

### Maps

```
var user = {name: "Bob", age: 30};
user["age"] += 1;
user["email"] = "bob@example.com";  // new key: just inserts, no push() needed
```

Maps are string-keyed and, like arrays, a value type: `var b = user; b["age"] = 0;` does **not** change `user`. Insertion order is preserved, so printing/`keys()`/`values()` are always deterministic — a map is not a `HashMap`.

```
var m = {a: 1, b: 2};
m.len();          // -> 2
m.has("a");       // -> true
m.get("z");       // -> null (never errors, unlike m["z"])
m.remove("a");    // -> 1, and writes the shrunk map back to `m`
m.keys();         // -> ["b"]
m.values();       // -> [2]
```

`m[key] = value` always upserts — inserts a new key or overwrites an existing one, unlike arrays where index-assignment is bounds-checked and can't grow. `m[key]` on a missing key is a runtime error (use `.get(key)`/`.has(key)` to check first); compound assignment (`m[key] += value`) also requires the key to already exist. Two maps compare equal (`==`) if they have the same keys and values, regardless of insertion order.

### Strings

```
var s = "  Hello World  ";
s.trim();       // -> "Hello World"
s.trim().upper();  // -> "HELLO WORLD"
s.trim().lower();  // -> "hello world"
s.len();        // -> 15 (character count, not byte count)
```

Strings have no `.` for growing them — use `+` (`s = s + "!";`), same as always. There's also no direct indexing (`s[0]`) or iteration (`for c in s`); instead, `.chars()` splits a string into an `Array` of single-character strings, which already has both:

```
for c in "abc".chars() {
    print(c);
}
"abc".chars()[1];  // -> "b"
```

### Numbers

```
(3.7).floor();   // -> 3
(3.2).ceil();    // -> 4
(3.5).round();   // -> 4
```

`floor`/`ceil`/`round` are `Float`-only and return an `Int` (not a `Float`) — the usual reason to want this conversion is to use the result as an array index, which needs a real `Int`. `Int` has no such methods (nothing to convert). Watch operator precedence: unary `-` binds looser than `.method()`, so `-3.7.floor()` means `-(3.7.floor())` (`-3`), not `(-3.7).floor()` (`-4`) — parenthesize the receiver if the sign needs to apply first.

### What's not there

Nothing pre-bound by default (the host opts scripts into native functions via `register_func`, or into the `stdlib/` libraries via `include()`, see above), no closures, no general `.` member access (only the fixed set of array/map/string pseudo-methods above) — so a map has no `Math.floor()`-style dotted namespacing either. `++`/`--` (either prefix or postfix) only work as a whole statement, e.g. `x++;` or a `for` loop's clauses — not embeddable mid-expression like `1 + x++`.

## Workspace layout

- `core/` — the language implementation (package `nib_core`): lexer, parser, interpreter.
- `nib/` — a CLI that runs `.nib` scripts, or prints their parsed AST.
- `bindings-php/` — a PHP extension (via `ext-php-rs`) exposing `nib` as a `Nib` class.
- `bindings-ts/` — TypeScript/WebAssembly bindings (via `wasm-bindgen`), published as the `@sntworx/nib` npm package.
- `stdlib/` — prepared libraries written in `nib` itself (`array.nib`, `string.nib`, `math.nib`), meant to be loaded with `include()` — see below.

## Standard library

Three small libraries in [`stdlib/`](stdlib/), written in `nib` itself, on top of the built-in Array/String/Float methods above. Not pre-bound — load whichever files you need with `include()`. Flat function names, no namespacing (see "What's not there" above), so this is exactly what a host or user could write and `include()` themselves.

- **`stdlib/array.nib`**: `array_contains(arr, value)`, `array_index_of(arr, value)`, `array_join(arr, sep)`, `array_reverse(arr)`, `array_slice(arr, start, end)`, `array_sum(arr)`, `array_min(arr)`, `array_max(arr)`, `array_map(arr, fn)`, `array_filter(arr, fn)`, `array_reduce(arr, fn, initial)`.
- **`stdlib/string.nib`**: `str_index_of(s, needle)`, `str_contains(s, needle)`, `str_starts_with(s, prefix)`, `str_ends_with(s, suffix)`, `str_slice(s, start, end)`, `str_repeat(s, count)`, `str_split(s, sep)`, `str_replace(s, search, replacement)`.
- **`stdlib/math.nib`**: `math_abs(x)`, `math_min(a, b)`, `math_max(a, b)`, `math_clamp(x, lo, hi)`, `math_pow(base, exponent)` — integer exponents; a negative exponent returns a `Float`.

`array_map`/`array_filter`/`array_reduce` take a `nib` function by name (e.g. `array_map(arr, double)`) — functions are ordinary values, so this needs no closures or special support.

## Configuration

Both host bindings accept an optional config when constructing a `Nib` instance (see the code examples in the [PHP](#php)/[JS/TS](#jsts) usage sections below). Every setting is optional and falls back to its default when omitted; settings are fixed for the lifetime of the instance — there's no way to change them after construction.

| Setting | Default | Description |
| --- | --- | --- |
| `maxCallDepth` | `1000` | Caps how deeply `nib` function calls can recurse. Exceeding it fails the script with a normal runtime error (`"stack overflow: exceeded maximum call depth of N"`) instead of overflowing the real native stack and crashing the host process — matters for any script that recurses, whether intentionally or from a bug. |
| `maxParseDepth` | `128` | Caps how deeply nested a script's expressions/blocks can be while parsing (e.g. deeply nested `((((1))))` grouping, or nested `if`/`while`/`{ }`). Exceeding it fails to parse with `"expression or block nested too deeply"` instead of overflowing the parser's own recursive descent — the default is deliberately much lower than `maxCallDepth` since a single level of syntax nesting burns several real stack frames during parsing, not one, and is verified safe with margin even on a constrained ~1MiB stack (e.g. a small worker thread, not just an ~8MiB main thread). |
| `maxSteps` | `1000000` | Caps total interpreter work per `run()` call — one "step" per statement executed and per loop iteration (so a non-empty loop body ticks more than once per iteration; this is a work budget, not a precise iteration count). Exceeding it fails with `"exceeded maximum execution steps of N"` instead of letting a script loop forever (e.g. `while true { }`) and hang the host process. Resets to zero at the start of every `run()` call — it's a per-run budget, not a lifetime total on a `Nib` instance reused across multiple scripts. |
| `maxStringLength` | `1000000` | Caps a single string's length (character count, not byte count), checked wherever a string is built or grows — literals, concatenation (`+`/`+=`), and methods that return a string. Exceeding it fails with `"string exceeds maximum length of N characters"`. |
| `maxArrayLength` | `1000000` | Caps a single array's element count, checked wherever an array is built or grows — literals, `push()`, and methods that return an array (`chars()`, `keys()`, `values()`). Exceeding it fails with `"array exceeds maximum length of N elements"`. |
| `maxMapSize` | `1000000` | Caps a single map's entry count, checked whenever a *new* key is inserted — literals and `m[newKey] = x`. Overwriting an existing key never grows the map, so it's never rejected regardless of this limit. Exceeding it fails with `"map exceeds maximum size of N entries"`. |

## PHP

### Installation

1. Download the extension build for your platform from the [Releases page](https://github.com/sntworx/nib/releases). Release builds are always packaged as `.so`, even the macOS one (PHP looks for a `.so` file regardless of platform, even though Rust itself produces a `.dylib` there).
2. Copy it into your PHP install's `extension_dir` (find that path with `php -i | grep extension_dir`).
3. Enable it in `php.ini`:
   ```ini
   extension=php_nib.so
   ```
4. Confirm it loaded: `php -m | grep -i nib`.

### Usage

```php
<?php

$nib = new Nib();

$nib->registerFunc("print", function (...$args) {
    echo implode(" ", $args), "\n";
});

$nib->disableKeywords(["while"]); // optional: restrict the language surface

$nib->include(file_get_contents(__DIR__ . "/lib/math.nib"));

$nib->parse('
    var x = 1 + 2;
    print("x =", double(x));
');

$nib->run();
```

`new Nib()` optionally takes an associative array to override the [config](#configuration) defaults — omit it, or omit either key, to use the defaults:

```php
$nib = new Nib([
    "maxCallDepth" => 200,
    "maxParseDepth" => 32,
    "maxSteps" => 50000,
    "maxStringLength" => 10000,
    "maxArrayLength" => 10000,
    "maxMapSize" => 10000,
]);
```

`lib/math.nib`:

```
func double(x) { return x * 2; }
```

`include()` just queues the source and can't fail on its own — no try/catch needed around it. `parse()` and `run()` throw on error (a bad script raises a PHP exception rather than returning an error code), so wrap them in `try`/`catch` when running untrusted scripts. `run()` is also where a problem in included code would surface (labeled `(in included code)` so it's not confused with a main-script error):

```php
try {
    $nib->parse($untrustedScript);
    $nib->run();
} catch (\Throwable $e) {
    // ...
}
```

Callbacks passed to `registerFunc` accept any PHP callable (closure, named function, `[$obj, "method"]`, etc.) and are arity-checked via reflection, so calling one with the wrong number of arguments from a `nib` script fails with a clear error instead of a PHP-level warning.

## JS/TS

### Installation

```sh
npm install @sntworx/nib
```

Works out of the box in Node (CommonJS `require` or ESM `import`) and via bundlers (webpack/vite/rollup). For direct browser use with no bundler, import the `@sntworx/nib/web` subpath instead — see below.

### Usage

Node or a bundler (auto-initializes, no manual setup step):

```js
import { readFileSync } from "node:fs";
import { Nib } from "@sntworx/nib";
// or: const { Nib } = require("@sntworx/nib");

const nib = new Nib();

nib.registerFunc("print", (...args) => {
    console.log(...args);
});

nib.disableKeywords(["while"]); // optional: restrict the language surface

nib.include(readFileSync("./lib/math.nib", "utf8"));

nib.parse(`
    var x = 1 + 2;
    print("x =", double(x));
`);

nib.run();
```

`lib/math.nib`:

```
func double(x) { return x * 2; }
```

`include()` just queues the source and can't fail on its own — no try/catch needed around it. A problem in included code surfaces from `run()` instead (labeled `(in included code)` so it's not confused with a main-script error).

`new Nib()` optionally takes an options object to override the [config](#configuration) defaults — omit it, or omit either key, to use the defaults:

```js
const nib = new Nib({
    maxCallDepth: 200,
    maxParseDepth: 32,
    maxSteps: 50000,
    maxStringLength: 10000,
    maxArrayLength: 10000,
    maxMapSize: 10000,
});
```

Direct browser, no bundler — needs an explicit async init first, and `include()`'s source has to be `fetch()`ed rather than read from disk. Browsers can't resolve a bare specifier like `@sntworx/nib/web` on their own (that's what bundlers/Node do), so map it to a real URL with an import map first:

```html
<script type="importmap">
{
    "imports": {
        "@sntworx/nib/web": "https://cdn.jsdelivr.net/npm/@sntworx/nib/pkg/web/nib_ts.js"
    }
}
</script>
<script type="module">
    import init, { Nib } from "@sntworx/nib/web";

    await init(); // fetches and instantiates the .wasm

    const nib = new Nib();
    nib.registerFunc("print", (...args) => console.log(...args));
    nib.include(await (await fetch("./lib/math.nib")).text());
    nib.parse('print("x =", double(2));');
    nib.run();
</script>
```

`parse()`/`run()` throw a JS `Error` on failure, so wrap them in `try`/`catch` when running untrusted scripts:

```js
try {
    nib.parse(untrustedScript);
    nib.run();
} catch (e) {
    // ...
}
```

Callbacks passed to `registerFunc` are plain JS functions and, unlike the PHP binding, aren't arity-checked — JS itself doesn't error on a mismatched argument count, so `nib` just calls through and lets normal JS semantics apply (missing arguments become `undefined`, extra ones are ignored).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
