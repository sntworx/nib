# run nib cli
nib *args:
  cargo run -p nib -- {{args}}

# build cli (musl)
cli-build-musl:
  #!/usr/bin/env bash
  set -euo pipefail
  version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' nib/Cargo.toml | head -n1)
  target=x86_64-unknown-linux-musl
  cargo build -p nib --release --target "$target"
  mkdir -p dist/cli
  cp "target/$target/release/nib" "dist/cli/nib-v${version}-${target}"
  echo "dist/cli/nib-v${version}-${target}"

# build cli (macos)
cli-build-macos:
  #!/usr/bin/env bash
  set -euo pipefail
  version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' nib/Cargo.toml | head -n1)
  # host triple, not a bare "macos" - this only ever builds for the host arch,
  # so the name has to say which one (Intel vs Apple Silicon)
  target=$(rustc -vV | sed -n 's/^host: //p')
  cargo build -p nib --release
  mkdir -p dist/cli
  cp target/release/nib "dist/cli/nib-v${version}-${target}"
  echo "dist/cli/nib-v${version}-${target}"

# build php bindings for local architecture
bindings-php-build:
  cargo build -p bindings-php --release

# package php extension (dist/php-nib)
bindings-php-package:
  mkdir -p dist/php-nib
  ./bindings-php/scripts/package.sh

# install php extension in local php instance
php-extension-install:
  cd bindings-php && cargo php install --release --yes

# remove php extension from local php instance
php-extension-remove:
  cd bindings-php && cargo php remove --yes

# update (remove and install) php extension in local php instance
php-extension-update:
  cd bindings-php && cargo php remove --yes && cargo php install --release --yes

# build TS bindings for web (bindings-ts/pkg/web)
bindings-ts-build-web:
  cd bindings-ts && wasm-pack build --release --target web --out-dir pkg/web

# build TS bindings for bundler (bindings-ts/pkg/bundler)
bindings-ts-build-bundler:
  cd bindings-ts && wasm-pack build --release --target bundler --out-dir pkg/bundler

# build TS bindings for node (bindings-ts/pkg/node)
bindings-ts-build-node:
  cd bindings-ts && wasm-pack build --release --target nodejs --out-dir pkg/node

# build all TS bindings (bindings-ts/pkg/*)
bindings-ts-build-all: bindings-ts-build-web bindings-ts-build-bundler bindings-ts-build-node
  rm -f bindings-ts/pkg/*/.gitignore

# pack TS bindings into npm tarball
bindings-ts-pack: bindings-ts-build-all
  mkdir -p dist/ts-nib
  cd bindings-ts && npm pack --pack-destination ../dist/ts-nib

# run the test suite (rust, all crates)
test:
  cargo test

# run the wasm binding tests (needs wasm-pack)
test-ts:
  cd bindings-ts && wasm-pack test --node

# run release-only tests (stack-size guarantees, skipped by default)
test-release:
  cargo test --release -- --ignored

# test coverage for the language crate (needs: cargo install cargo-llvm-cov)
coverage:
  cargo llvm-cov --package nib-lang --summary-only

# test coverage as a browsable HTML report
coverage-html:
  cargo llvm-cov --package nib-lang --html --open

# build and open the language crate's rustdoc
docs:
  cargo doc -p nib-lang --no-deps --open

# run cargo FMT
cargo-fmt:
    cargo +nightly fmt