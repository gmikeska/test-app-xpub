#!/usr/bin/env bash
# Fails if the vendored @emvault/jade driver has drifted from the source repo.
# Assumes emvault-jade is checked out as a sibling of this app.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
src="${EMVAULT_JADE_SRC:-$here/../emvault-jade/src}"
dst="$here/static/vendor/emvault-jade"
if [ ! -d "$src" ]; then
  echo "check-vendor: source not found at $src (set EMVAULT_JADE_SRC); skipping." >&2
  exit 0
fi
status=0
for f in index.js jade-rpc.js cbor.js; do
  if ! diff -q "$src/$f" "$dst/$f" >/dev/null 2>&1; then
    echo "check-vendor: DRIFT in $f — re-copy from $src" >&2
    status=1
  fi
done
[ "$status" -eq 0 ] && echo "check-vendor: vendored @emvault/jade is in sync."
exit "$status"
