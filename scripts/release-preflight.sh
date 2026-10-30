#!/usr/bin/env bash
# Fails while the workspace cannot be published as-is: cargo ignores [patch] in published
# crates, and publish = false crates are skipped by `cargo publish --workspace`.
set -euo pipefail
status=0
meta="$(cargo metadata --no-deps --format-version 1)"
blocked="$(jq -r '.packages[] | select(.publish == []) | .name' <<<"$meta" | grep -Ev '^(meari-xtask|meari-demo|meari-visual)$' || true)"
if [ -n "$blocked" ]; then echo "::error::publish = false: $(echo $blocked)"; status=1; fi
if grep -q '^\[patch\.crates-io\]' Cargo.toml; then
  echo "::error::[patch.crates-io] is not honoured by published crates; rename the vendored forks first"; status=1
fi
# A path dependency without a version is stripped of its source on publish and fails.
nover="$(jq -r '.packages[] | select(.publish != []) | .name as $n | .dependencies[] | select(.path != null and .kind != "dev" and .req == "*") | "\($n)->\(.name)"' <<<"$meta")"
if [ -n "$nover" ]; then echo "::error::path dependency without version: $(echo $nover)"; status=1; fi
# The vendored forks must replace upstream everywhere, or two copies of the DOM types meet.
upstream="$(jq -r '.packages[] | .name as $n | .dependencies[] | select(.source != null and (.name | test("^blitz-(dom|paint|html|shell)$"))) | "\($n)->\(.name)"' <<<"$meta")"
if [ -n "$upstream" ]; then echo "::error::upstream blitz fork still used from the registry: $(echo $upstream)"; status=1; fi
exit $status
