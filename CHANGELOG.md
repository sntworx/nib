# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

While `nib` is pre-1.0, breaking changes to the language and to the host APIs
may land in any minor release.

## [Unreleased]

### Added

- `register_var` (`registerVar` in the PHP/TS bindings) — the value counterpart to `register_func`: binds a plain `Value` into the global scope instead of a native function, composing the same way (shadowable by the script's own `let`, visible from every `func` the script defines).

### Fixed

- `bindings-ts`: `new Nib()` now type-checks with no arguments — the generated `.d.ts` marks the constructor's `options` as `NibOptions?` instead of a required parameter.

## [0.1.0] - 2026-08-14

First release.
