#!/usr/bin/env bash
# Shared cargo target report. Read-only by default; deletion only with --apply.
# Usage: scripts/cargo-cache-report.sh [--days N] [--limit-gb N] [--apply]
set -euo pipefail
TARGET="${CARGO_TARGET_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/.cargo-target}"
DAYS=7; LIMIT_GB=30; APPLY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --days) DAYS="$2"; shift 2;;
    --limit-gb) LIMIT_GB="$2"; shift 2;;
    --apply) APPLY=1; shift;;
    -h|--help) sed -n '2,3p' "$0"; exit 0;;
    *) echo "unknown option: $1" >&2; exit 2;;
  esac
done
[ -d "$TARGET" ] || { echo "no target dir: $TARGET"; exit 0; }
kb=$(du -sk "$TARGET" | cut -f1); gb=$((kb / 1024 / 1024))
echo "target: $TARGET"; echo "size: ${gb}GB (limit ${LIMIT_GB}GB)"
[ "$gb" -gt "$LIMIT_GB" ] && echo "WARN: over ${LIMIT_GB}GB - cleanup proposed (needs approval)"
echo "candidates older than ${DAYS} days (profile/deps, profile/build, profile/.fingerprint):"
find "$TARGET" -mindepth 3 -maxdepth 3 \( -path '*/deps/*' -o -path '*/build/*' -o -path '*/.fingerprint/*' \) -mtime +"$DAYS" 2>/dev/null \
  | tee /tmp/cargo-cache-candidates.$$ | head -50
n=$(wc -l < /tmp/cargo-cache-candidates.$$); echo "candidate count: $n"
if [ "$APPLY" -eq 1 ]; then
  if command -v cargo-sweep >/dev/null; then cargo sweep --time "$DAYS" "$TARGET"
  else echo "cargo-sweep missing; refusing --apply" >&2; exit 1; fi
else echo "dry-run: nothing deleted (pass --apply after approval)"; fi
rm -f /tmp/cargo-cache-candidates.$$
