#!/usr/bin/env bash
# Re-verifies a VENDORED.sha256 crate against crates.io: downloads the upstream .crate
# (read-only), checks its sha256 against the manifest header and diffs src/.
# Usage: scripts/vendored-upstream-diff.sh crates/meari-shell
set -euo pipefail
dir="$1"; manifest="$dir/VENDORED.sha256"
read -r _ _ name version _ _ want < <(grep "^# upstream:" "$manifest" | tr -d ,)
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
curl -fsSL -A "meari-vendored-check (github.com/modootoday/meari)" "https://static.crates.io/crates/$name/$name-$version.crate" -o "$tmp/c.crate"
got="$(sha256sum "$tmp/c.crate" | cut -d' ' -f1)"
[ "$got" = "$want" ] || { echo "::error::$name $version sha256 $got != $want"; exit 1; }
tar -xzf "$tmp/c.crate" -C "$tmp"
diff -r "$tmp/$name-$version/src" "$dir/src"
echo "$dir/src == crates.io $name $version ($want)"
