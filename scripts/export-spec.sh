#!/usr/bin/env bash
# Exports the ticketing contract's spec (functions, types, errors and
# events) next to the built wasm, for release assets (issue #60).
#
# Usage: scripts/export-spec.sh [output-dir]
#
# Writes, into output-dir (default: target/wasm32v1-none/release):
#   stellar_tickets_ticketing.spec.json  formatted JSON, one SCSpecEntry per item
#   stellar_tickets_ticketing.spec.xdr   the same entries as base64 XDR
set -euo pipefail
cd "$(dirname "$0")/.."

OUT_DIR="${1:-target/wasm32v1-none/release}"
WASM="target/wasm32v1-none/release/stellar_tickets_ticketing.wasm"

if ! command -v stellar >/dev/null 2>&1; then
  echo "Error: 'stellar' CLI is not installed or not available in PATH." >&2
  echo "Install it via: cargo install --locked stellar-cli --version ^27" >&2
  exit 1
fi
if [ ! -f "$WASM" ]; then
  echo "Error: $WASM not found. Run 'stellar contract build' first." >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
JSON="$OUT_DIR/stellar_tickets_ticketing.spec.json"
XDR="$OUT_DIR/stellar_tickets_ticketing.spec.xdr"

# The CLI prints a progress line on stderr; only stdout is the spec.
stellar contract info interface --wasm "$WASM" --output json-formatted > "$JSON"
stellar contract info interface --wasm "$WASM" --output xdr-base64 > "$XDR"

# Fail the release rather than publish an empty or truncated spec.
if ! grep -q '"function_v0"' "$JSON" || ! grep -q '"event_v0"' "$JSON"; then
  echo "Error: exported spec has no functions or events: $JSON" >&2
  exit 1
fi

echo "Wrote $JSON"
echo "Wrote $XDR"
