# @sntworx/nib

> **Active development.** APIs, syntax, and behavior may change without notice. Not recommended for production use yet.

TypeScript/WebAssembly bindings for `nib` — a small custom scripting language written in Rust, meant to be embedded inside a host application rather than run standalone.

Nothing is pre-bound by default: your app decides exactly which native functions a script is allowed to call (`registerFunc`), and can even strip specific keywords out of the language for a given script (`disableKeywords`) — e.g. dropping `while`/`for` to rule out unbounded loops. That opt-in-only surface makes it a fit for running untrusted or user-authored logic inside a larger app: plugin scripting, rules/workflow engines, user-defined formulas, that kind of thing.

For shared logic that's easier to write in `nib` itself than as JS functions, `include()` your own nib-authored library code — or the prepared `array`/`string`/`math` libraries in [`stdlib/`](https://github.com/sntworx/nib/tree/main/stdlib) in the main repo (this package doesn't bundle them itself, so fetch the `.nib` file you need and `include()` it, same as `math.nib` below).

Full language reference, syntax guide, and the PHP binding live in the [main repo](https://github.com/sntworx/nib).

## Installation

```sh
npm install @sntworx/nib
```

Works out of the box in Node (CommonJS `require` or ESM `import`) and via bundlers (webpack/vite/rollup). For direct browser use with no bundler, import the `@sntworx/nib/web` subpath instead — see below.

## Usage

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

`disableKeywords()` throws if given anything that isn't a nib keyword: a typo like `"Whlie"` is rejected rather than quietly ignored, since accepting it would leave you believing the language was restricted when it wasn't. A rejected call changes nothing — no name in it is applied.

Values crossing the boundary in either direction may nest at most 128 levels deep; anything deeper (including a cyclic object like `o.self = o`, which has no bottom) fails with `value nested deeper than 128 levels`. The conversion walks the structure recursively, so without that cap a cyclic value would overflow the wasm stack — and because that unwind skips Rust destructors, it poisons the whole module, not just the `Nib` instance that hit it.

`new Nib()` optionally takes an options object to override a handful of interpreter safety limits (recursion depth, parser nesting depth, total execution steps, max string/array/map size, max value nesting depth, max total values per tree) — omit it, or any of its keys, to use the defaults; see the [main repo](https://github.com/sntworx/nib#configuration) for what each option guards against:

```js
const nib = new Nib({
    maxCallDepth: 200,
    maxParseDepth: 32,
    maxSteps: 50000,
    maxStringLength: 10000,
    maxArrayLength: 10000,
    maxMapSize: 10000,
    maxValueDepth: 64,
    maxValueNodes: 100000,
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

Callbacks passed to `registerFunc` are plain JS functions and aren't arity-checked — JS itself doesn't error on a mismatched argument count, so `nib` just calls through and lets normal JS semantics apply (missing arguments become `undefined`, extra ones are ignored).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
