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

# Builds + names the release cdylib as dist/php-nib/php_nib-v<version>-php<major.minor>-<target>.<ext>.
# Target defaults to the host triple; PHP version comes from whichever `php` is on PATH.
package-bindings-php target='':
  ./bindings-php/scripts/package.sh {{target}}

package-bindings-php-macos-arm64:
  ./bindings-php/scripts/package.sh aarch64-apple-darwin

package-bindings-php-linux-x64-gnu:
  ./bindings-php/scripts/package.sh x86_64-unknown-linux-gnu

package-bindings-php-linux-x64-musl:
  ./bindings-php/scripts/package.sh x86_64-unknown-linux-musl

# Generates dist/php-nib/SHA256SUMS over whatever packages are currently there.
# Uses sha256sum on Linux, shasum -a 256 on macOS (whichever is on PATH).
checksum-bindings-php:
  ./bindings-php/scripts/checksum.sh

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
  mkdir -p dist/ts-nib
  cd bindings-ts && npm pack --pack-destination ../dist/ts-nib

# Cargo

cargo-fmt:
    cargo +nightly fmt