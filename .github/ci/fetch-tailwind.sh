#!/usr/bin/env bash
# Downloads one official Tailwind standalone asset into runtime/tools/tailwind/ and
# checks it against the upstream sha256sums.txt of the same release and the hash
# pinned in crates/meari-native/src/infrastructure/styling/tailwind.rs.
set -euo pipefail
asset="$1"
version="${TAILWIND_VERSION:?}"
base="https://github.com/tailwindlabs/tailwindcss/releases/download/v${version}"
dir="runtime/tools/tailwind"
mkdir -p "$dir"
curl -fsSL "$base/$asset" -o "$dir/$asset"
expected=$(curl -fsSL "$base/sha256sums.txt" | awk -v a="./$asset" '$2==a {print $1}')
if command -v sha256sum >/dev/null; then actual=$(sha256sum "$dir/$asset" | cut -d' ' -f1)
else actual=$(shasum -a 256 "$dir/$asset" | cut -d' ' -f1); fi
[ -n "$expected" ] && [ "$expected" = "$actual" ] || { echo "sha256 mismatch for $asset" >&2; exit 1; }
grep -q "$actual" crates/meari-native/src/infrastructure/styling/tailwind.rs || { echo "$asset hash not pinned" >&2; exit 1; }
chmod +x "$dir/$asset"
