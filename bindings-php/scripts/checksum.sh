#!/usr/bin/env bash
# Generates dist/php-nib/SHA256SUMS over whatever packages are currently
# there, for attaching to a manual release alongside the binaries.
#
# macOS has no sha256sum by default, only `shasum -a 256`, and Linux
# typically has sha256sum but not shasum — so pick whichever is on PATH.
set -euo pipefail

cd "$(dirname "$0")/../.."

dir="dist/php-nib"
if [[ ! -d "$dir" ]] || [[ -z "$(find "$dir" -maxdepth 1 -type f ! -name SHA256SUMS)" ]]; then
  echo "error: no packages found in $dir — run 'just package-bindings-php' first" >&2
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  checksum=(sha256sum)
elif command -v shasum >/dev/null 2>&1; then
  checksum=(shasum -a 256)
else
  echo "error: neither sha256sum nor shasum found on PATH" >&2
  exit 1
fi

out="$dir/SHA256SUMS"
(cd "$dir" && "${checksum[@]}" -- $(find . -maxdepth 1 -type f ! -name SHA256SUMS -exec basename {} \; | sort) > SHA256SUMS)

echo "$out"
