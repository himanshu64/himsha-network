#!/usr/bin/env bash
# Create and fund a regtest wallet inside the bitcoin container.
# Run once after `docker compose up`. Re-running is safe (loads the wallet).
set -euo pipefail

cd "$(dirname "$0")/../.."
[ -f .env ] && set -a && . ./.env && set +a

USER="${BITCOIN_RPC_USER:-himuser}"
PASS="${BITCOIN_RPC_PASS:-devpass_change_me}"
CLI=(docker compose exec -T bitcoin bitcoin-cli -regtest -rpcuser="$USER" -rpcpassword="$PASS")

echo "→ ensuring wallet 'main' exists"
"${CLI[@]}" createwallet main >/dev/null 2>&1 || "${CLI[@]}" loadwallet main >/dev/null 2>&1 || true

ADDR=$("${CLI[@]}" -rpcwallet=main getnewaddress)
echo "→ mining 101 blocks to $ADDR (matures coinbase)"
"${CLI[@]}" -rpcwallet=main generatetoaddress 101 "$ADDR" >/dev/null

BAL=$("${CLI[@]}" -rpcwallet=main getbalance)
echo "✓ regtest ready — wallet balance: $BAL BTC"
