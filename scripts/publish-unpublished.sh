#!/usr/bin/env bash
# Publishes (or dry-runs) only the workspace crates whose current version is not on crates.io
# yet, so a tag run does not fail on versions that were published manually during bootstrap.
# Usage: scripts/publish-unpublished.sh [--dry-run]
set -euo pipefail
extra=("$@")
meta="$(cargo metadata --no-deps --format-version 1)"
args=()
while IFS=$'\t' read -r name version; do
  code="$(curl -s -o /dev/null -w '%{http_code}' -A 'meari-release (github.com/modootoday/meari)' "https://crates.io/api/v1/crates/${name}/${version}")"
  case "$code" in
    200) echo "skip $name $version (already on crates.io)" ;;
    404) args+=(-p "$name") ;;
    *) echo "::error::crates.io lookup for $name $version returned $code"; exit 1 ;;
  esac
done < <(jq -r '.packages[] | select(.publish != []) | [.name, .version] | @tsv' <<<"$meta")
if [ "${#args[@]}" -eq 0 ]; then echo "nothing to publish"; exit 0; fi
echo "publishing: ${args[*]}"
cargo publish --locked "${args[@]}" "${extra[@]}"
