#!/usr/bin/env bash
# Builds bindings-php in release mode for a given Rust target triple and
# copies the resulting cdylib into dist/php-nib/ as
#   php_nib-v<crate-version>-php<major.minor>-<target-triple>.<ext>
#
# The target triple already encodes OS/arch/libc (gnu vs musl), so folding
# it straight into the filename is what keeps 3 PHP versions x N targets
# from colliding on disk. The PHP major.minor comes from whichever `php`
# binary is on PATH at build time (ext-php-rs links against it), so building
# for multiple PHP versions means switching the active PHP (phpbrew/asdf/
# Docker/etc.) between invocations, not something this script controls.
set -euo pipefail

cd "$(dirname "$0")/../.."

target="${1:-}"
if [[ -z "$target" ]]; then
  target=$(rustc -vV | sed -n 's/^host: //p')
fi

case "$target" in
  *-apple-darwin) ext="dylib" ;;
  *-linux-*) ext="so" ;;
  *) echo "error: unsupported target for packaging: $target" >&2; exit 1 ;;
esac

if ! command -v php >/dev/null 2>&1; then
  echo "error: no 'php' binary on PATH — bindings-php's build.rs needs one to detect the Zend API version" >&2
  exit 1
fi

if [[ "$target" == *-musl ]] && ! command -v musl-gcc >/dev/null 2>&1; then
  echo "warning: musl-gcc not found on PATH — install musl-tools (e.g. 'apt install musl-tools') before building for $target" >&2
fi

rustup target add "$target"

cargo build -p bindings-php --release --target "$target"

built_lib="target/$target/release/libphp_nib.$ext"
if [[ ! -f "$built_lib" ]]; then
  echo "error: expected build output not found: $built_lib" >&2
  exit 1
fi

php_version=$(php -r 'echo PHP_MAJOR_VERSION . "." . PHP_MINOR_VERSION;')
crate_version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' bindings-php/Cargo.toml | head -n1)

mkdir -p dist/php-nib
out="dist/php-nib/php_nib-v${crate_version}-php${php_version}-${target}.${ext}"
cp "$built_lib" "$out"

echo "$out"
