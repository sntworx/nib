# bump the version everywhere it's duplicated: the Cargo workspace (core, nib,
# bindings-php, bindings-ts all inherit from `[workspace.package]`) and
# bindings-ts/package.json - npm is a separate registry with no shared
# version field, so that one file still needs its own edit.
bump-version version:
  #!/usr/bin/env bash
  set -euo pipefail
  sed -i.bak -E "/^\[workspace\.package\]/,/^\[/ s/^version = \".*\"/version = \"{{version}}\"/" Cargo.toml
  rm -f Cargo.toml.bak
  sed -i.bak -E "s/\"version\": \"[^\"]*\"/\"version\": \"{{version}}\"/" bindings-ts/package.json
  rm -f bindings-ts/package.json.bak
  cargo check --workspace --quiet
  echo "Bumped to {{version}}"

# run nib cli
nib *args:
  cargo run -p nib -- {{args}}

# build cli (musl)
cli-build-musl:
  #!/usr/bin/env bash
  set -euo pipefail
  version=$(cargo pkgid -p nib | sed 's/.*#//')
  target=x86_64-unknown-linux-musl
  cargo build -p nib --release --target "$target"
  mkdir -p dist/cli
  cp "target/$target/release/nib" "dist/cli/nib-v${version}-${target}"
  echo "dist/cli/nib-v${version}-${target}"

# build cli (macos)
cli-build-macos:
  #!/usr/bin/env bash
  set -euo pipefail
  version=$(cargo pkgid -p nib | sed 's/.*#//')
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

# start the php dev container (.docker/) in the background
docker-up:
  docker compose -f .docker/docker-compose.yml up -d

# stop and remove the php dev container
docker-down:
  docker compose -f .docker/docker-compose.yml down

# build + install the php extension inside the running dev container
docker-php-extension-install:
  docker compose -f .docker/docker-compose.yml exec nib-php-dev just php-extension-install

# run a php script inside the running dev container, e.g. `just docker-php-run test.php`
docker-php-run script:
  docker compose -f .docker/docker-compose.yml exec nib-php-dev php {{script}}

# full teardown: remove the container, its image and the cached target/registry volumes
docker-clean:
  docker compose -f .docker/docker-compose.yml down --rmi local --volumes

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