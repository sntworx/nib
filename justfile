# Nib

nib *args:
  cargo run -p nib -- {{args}}

# PHP Bindings

build-bindings-php:
  cargo build -p bindings-php --release

install-php-extension:
  cd bindings-php && cargo php install --release --yes

remove-php-extension:
  cd bindings-php && cargo php remove --yes

update-php-extension:
  cd bindings-php && cargo php remove --yes && cargo php install --release --yes

# TS Bindings

build-bindings-ts-web:
  cd bindings-ts && wasm-pack build --release --target web --out-dir pkg/web

build-bindings-ts-bundler:
  cd bindings-ts && wasm-pack build --release --target bundler --out-dir pkg/bundler

build-bindings-ts-node:
  cd bindings-ts && wasm-pack build --release --target nodejs --out-dir pkg/node

build-bindings-ts: build-bindings-ts-web build-bindings-ts-bundler build-bindings-ts-node
  rm -f bindings-ts/pkg/*/.gitignore

pack-bindings-ts: build-bindings-ts
  cd bindings-ts && npm pack

# Cargo

cargo-fmt:
    cargo +nightly fmt