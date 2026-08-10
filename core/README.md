# nib-lang

> **Active development.** APIs, syntax, and behavior may change without notice. Not recommended for production use yet.

`nib` is a small, embeddable scripting language with C-like syntax (`var`, `if`/`else`, `while`, `for`, top-level `func`s with no closures, arrays, maps) and a tree-walking interpreter. This crate, `nib-lang`, is the language implementation: lexer, parser, and interpreter, with no dependencies of its own.

Nothing is pre-bound by default: a host opts a script into native functions via `register_func`, and can strip specific keywords out of the language for a given script via `disable_keywords` (e.g. dropping `while`/`for` to rule out unbounded loops). That makes it a fit for running untrusted or user-authored logic inside a larger Rust application — plugin scripting, rules/workflow engines, user-defined formulas — where a script should only ever touch what was explicitly exposed to it.

## Quick start

```rust
use nib_lang::{Nib, Value};

let mut nib = Nib::new();

nib.register_func("double", |args: &[Value]| match args {
    [Value::Int(n)] => Ok(Value::Int(n * 2)),
    _ => Err("double() expects one int argument".to_string()),
});

nib.parse("double(21);")?;
nib.run()?;
# Ok::<(), nib_lang::Error>(())
```

`Nib` owns a persistent interpreter, so registered functions and script-defined globals survive across repeated `parse()`/`run()` calls. See the [`Nib`](https://docs.rs/nib-lang/latest/nib_lang/struct.Nib.html) docs for the full method-by-method reference, including `include()` for loading shared `nib`-authored library code before the main script.

## Syntax at a glance

```
var x = 1 + 2;
if x > 1 { } else { }
while x < 10 { x++; }
for (var i = 0; i < 10; i += 1) { }
for x in [1, 2, 3] { }
match x { case 1 { } default { } }
try { throw "boom"; } catch e { }

func add(a, b) { return a + b; }

var arr = [1, 2, 3];
arr.push(4);

var m = {name: "Bob", age: 30};
m["age"] += 1;
```

Arrays and maps are value types (assigning or passing one copies it, cheaply, via copy-on-write), functions are top-level only with no closures, and `.method()` is a small closed set of built-in pseudo-methods rather than general member access. For the full language reference — every operator, control-flow form, and built-in method — see the [project README](https://github.com/sntworx/nib#syntax).

## Sandbox limits

Every `Nib` is bounded by a [`Config`](https://docs.rs/nib-lang/latest/nib_lang/struct.Config.html), passed to `Nib::with_config`. Defaults are sized for an embedded sandbox rather than a maximum-plausible script:

| Field | Default | Bounds |
| --- | --- | --- |
| `max_call_depth` | `200` | Recursive `nib` function-call depth. |
| `max_parse_depth` | `128` | Nested expression/block depth while parsing. |
| `max_steps` | `100_000` | Total interpreter work per `run()` call. |
| `max_string_length` | `65_536` | A single string's character count. |
| `max_array_length` | `10_000` | A single array's element count. |
| `max_map_size` | `10_000` | A single map's entry count. |
| `max_value_depth` | `64` | How deeply arrays/maps may nest inside each other. |
| `max_value_nodes` | `100_000` | Total values in one array/map tree, counting nested ones. |

Exceeding any of these fails the script with a catchable runtime error rather than hanging or crashing the host process — see each field's own documentation on [`Config`](https://docs.rs/nib-lang/latest/nib_lang/struct.Config.html) for why it's shaped the way it is.

## More

The broader project also ships prepared `nib`-authored libraries (array/string/math helpers), a CLI, and host bindings for PHP and TypeScript/WebAssembly built on top of this crate — see the [project repository](https://github.com/sntworx/nib) for those.

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/sntworx/nib/blob/main/LICENSE-APACHE) or [MIT license](https://github.com/sntworx/nib/blob/main/LICENSE-MIT) at your option.
