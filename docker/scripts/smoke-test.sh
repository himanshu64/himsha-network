#!/usr/bin/env bash
# Exercise the running HIMSHA node over JSON-RPC.
set -euo pipefail

cd "$(dirname "$0")/../.."
[ -f .env ] && set -a && . ./.env && set +a

PORT="${HIMSHA_NODE_PORT:-9100}"
URL="http://localhost:${PORT}"

call() {
  local method="$1" params="${2:-[]}"
  curl -s -X POST "$URL" -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":$params}"
  echo
}

echo "# node ready?";      call himsha_isNodeReady
echo "# version";          call himsha_getVersion
echo "# current slot";     call himsha_getSlot
echo "# programs";         call himsha_listPrograms
echo "# stats";            call himsha_getStats

# Faucet demo (requires HIMSHA_FAUCET=1).
PK="11111111111111111111111111111111"
echo "# airdrop to $PK"; call himsha_requestAirdrop "[\"$PK\",1000000]"
echo "# account info";   call himsha_getAccountInfo "[\"$PK\"]"
