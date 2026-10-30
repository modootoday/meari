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
exit $status
