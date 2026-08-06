# NIB

![nib](.github/assets/nib-logo.svg)

> **Active development.** APIs, syntax, and behavior may change without notice. Not recommended for production use yet.

### What's nib

`nib` is a small custom scripting language, meant to be embedded inside a host application rather than run standalone. C-like syntax (`var`, `if`/`else`, `while`, `for`, top-level `func`s with no closures, arrays, etc.).

It has no standard library and nothing pre-bound by default: the host decides exactly which native functions a script is allowed to call (`register_func`), and can even strip specific keywords out of the language for a given script (`disable_keywords`) — e.g. dropping `while`/`for` to rule out unbounded loops. That opt-in-only surface makes it a fit for running untrusted or user-authored logic inside a larger app: plugin scripting, rules/workflow engines, user-defined formulas, that kind of thing — where you want scripts to only ever touch what you explicitly exposed.

The same language also reaches multiple host runtimes: a PHP extension (`bindings-php`) and TypeScript/WebAssembly bindings (`bindings-ts`) sit on top of the same core interpreter, so identical `nib` scripts and host-defined behavior can run in a PHP backend and a browser/Node frontend alike.

## Table of contents

- [What's nib](#whats-nib)
- [Syntax](#syntax)
  - [Comments](#comments)
  - [Literals](#literals)
  - [Variables & assignment](#variables--assignment)
  - [Control flow](#control-flow)
  - [Functions](#functions)
  - [Arrays](#arrays)
  - [Operators](#operators)
  - [What's not there](#whats-not-there)
- [Workspace layout](#workspace-layout)
- [PHP](#php)
  - [Installation](#installation)
  - [Usage](#usage)
- [JS/TS](#jsts)
  - [Installation](#installation-1)
  - [Usage](#usage-1)
- [License](#license)

### Syntax

#### Comments

```
// line comments

/* and
   block comments */
```

#### Literals

```
var i = 42;
var f = 3.14;
var s = "line 1\nline 2\ttabbed\\backslash \"quoted\"";
var b = true;
var n = null;
var a = [1, 2, 3];
```

#### Variables & assignment

```
var x = 1;
x = 2;
x += 3;   // also -= *= /=
x++;      // also x-- and prefix ++x/--x
```

#### Control flow

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

`if`/`while` conditions don't need parens; `for`'s three clauses do, and each of them is optional (`for (;;) { }` loops forever). `break`/`continue` are only valid inside a loop.

A bare `{ ... }` also works as its own statement — its own scope, not attached to any `if`/`while`/`for`/`func`.

#### Functions

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

#### Arrays

```
var matrix = [[1, 2], [3, 4]];
matrix[0][1] = 9;
matrix[0][1] += 1;
```

Arrays are a value type: `var b = a; b[0] = 1;` does **not** change `a`, unlike JS/Python/Ruby.

#### Operators

```
1 + 2 * 3;
(1 + 2) * 3;         // parens are just a grouping expression
"count: " + 5;       // + also stringifies numbers for concatenation
a == b && c != d;    // && and || short-circuit
!done || -x <= 0;    // unary ! and -
```

#### What's not there

No standard library/builtins by default (the host opts scripts into native functions via `register_func`, see above), no closures, no `.` member access. `++`/`--` (either prefix or postfix) only work as a whole statement, e.g. `x++;` or a `for` loop's clauses — not embeddable mid-expression like `1 + x++`.

### Workspace layout
- `core/` — the language implementation (package `nib_core`): lexer, parser, interpreter.
- `nib/` — a CLI that runs `.nib` scripts, or prints their parsed AST.
- `bindings-php/` — a PHP extension (via `ext-php-rs`) exposing `nib` as a `Nib` class.
- `bindings-ts/` — TypeScript/WebAssembly bindings (via `wasm-bindgen`), published as the `@sntworx/nib` npm package.

## PHP

### Installation

1. Download the extension build for your platform from the [Releases page](https://github.com/sntworx/nib/releases).
2. Copy it into your PHP install's `extension_dir` (find that path with `php -i | grep extension_dir`). **On macOS**, PHP looks for a `.so` file even though Rust produces a `.dylib` — rename it to end in `.so` after copying it over, or PHP won't find it.
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

$nib->parse('
    var x = 1 + 2;
    print("x =", x);
');

$nib->run();
```

`parse()` and `run()` throw on error (a bad script raises a PHP exception rather than returning an error code), so wrap them in `try`/`catch` when running untrusted scripts:

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
import { Nib } from "@sntworx/nib";
// or: const { Nib } = require("@sntworx/nib");

const nib = new Nib();

nib.registerFunc("print", (...args) => {
    console.log(...args);
});

nib.disableKeywords(["while"]); // optional: restrict the language surface

nib.parse(`
    var x = 1 + 2;
    print("x =", x);
`);

nib.run();
```

Direct browser, no bundler — needs an explicit async init first:

```html
<script type="module">
    import init, { Nib } from "@sntworx/nib/web";

    await init(); // fetches and instantiates the .wasm

    const nib = new Nib();
    nib.registerFunc("print", (...args) => console.log(...args));
    nib.parse('print("hello from the browser");');
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
