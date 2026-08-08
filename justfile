# run nib cli
nib *args:
  cargo run -p nib -- {{args}}

# build cli (musl)
cli-build-musl:
  cargo build -p nib --release --target x86_64-unknown-linux-musl
  mkdir -p dist/cli
  cp target/x86_64-unknown-linux-musl/release/nib dist/cli/nib-cli-linux

# build cli (macos)
cli-build-macos:
  cargo build -p nib --release
  mkdir -p dist/cli
  cp target/release/nib dist/cli/nib-cli-macos

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

# run cargo FMT
cargo-fmt:
    cargo +nightly fmt