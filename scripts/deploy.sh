#!/usr/bin/env bash
# Deploys the ticketing contract's wasm to the given network.
# Usage: scripts/deploy.sh <identity> <network>
set -euo pipefail
cd "$(dirname "$0")/.."

IDENTITY="${1:?identity required}"
NETWORK="${2:?network required (testnet|futurenet|mainnet)}"

stellar contract deploy \
  --wasm target/wasm32v1-none/release/stellar_tickets_ticketing.wasm \
  --source "$IDENTITY" \
  --network "$NETWORK"
