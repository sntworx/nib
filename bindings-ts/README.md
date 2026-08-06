# @sntworx/nib

> **Active development.** APIs, syntax, and behavior may change without notice. Not recommended for production use yet.

TypeScript/WebAssembly bindings for `nib` — a small custom scripting language meant to be embedded inside a host application rather than run standalone.

It has no standard library and nothing pre-bound by default: your app decides exactly which native functions a script is allowed to call (`registerFunc`), and can even strip specific keywords out of the language for a given script (`disableKeywords`) — e.g. dropping `while`/`for` to rule out unbounded loops. That opt-in-only surface makes it a fit for running untrusted or user-authored logic inside a larger app: plugin scripting, rules/workflow engines, user-defined formulas, that kind of thing.

Full language reference, syntax guide, and the PHP binding live in the [main repo](https://github.com/sntworx/nib).

## Installation

```sh
npm install @sntworx/nib
```

Works out of the box in Node (CommonJS `require` or ESM `import`) and via bundlers (webpack/vite/rollup). For direct browser use with no bundler, import the `@sntworx/nib/web` subpath instead — see below.

## Usage

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

Callbacks passed to `registerFunc` are plain JS functions and aren't arity-checked — JS itself doesn't error on a mismatched argument count, so `nib` just calls through and lets normal JS semantics apply (missing arguments become `undefined`, extra ones are ignored).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
