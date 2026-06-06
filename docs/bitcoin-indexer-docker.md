# Bitcoin Indexer + Ord — Complete Docker Setup Guide

> ⚠️ **Disclaimer**: This guide is for educational and development purposes only.
> Do not expose RPC credentials publicly. Never run with real mainnet funds without
> a thorough security review.

---

## Overview

This guide sets up a complete Bitcoin data stack using Docker Compose:

| Service | Role | Image |
|---------|------|-------|
| **Bitcoin Core** | Full Bitcoin node, transaction index | `ruimarinho/bitcoin-core:24` |
| **Electrs** | Fast UTXO indexer (Electrum protocol) | `getumbrel/electrs:v0.10.2` |
| **Ord** | Ordinals / inscription indexer | `ordinals/ord:latest` |
| **HIMSHA Node** | ZK-proven Bitcoin programmability layer | (built locally) |

---

## System Requirements

| Network | CPU | RAM | Disk | Time to sync |
|---------|-----|-----|------|-------------|
| Regtest | 2 cores | 2 GB | 5 GB | Instant |
| Testnet4 | 4 cores | 4 GB | 100 GB | 2–4 hours |
| Mainnet | 8 cores | 16 GB | 700 GB SSD | 2–5 days |

---

## Project Layout

```
bitcoin-infra/
├── docker-compose.yml           # Main orchestration file
├── docker-compose.override.yml  # Local dev overrides (not committed)
├── .env                         # Environment variables (not committed)
├── config/
│   ├── bitcoin/
│   │   └── bitcoin.conf         # Bitcoin Core config
│   ├── electrs/
│   │   └── config.toml          # Electrs config
│   └── ord/
│       └── ord.toml             # Ord config
├── scripts/
│   ├── init-regtest.sh          # Fund regtest wallet on first start
│   ├── healthcheck.sh           # Composite health check
│   └── backup.sh                # Data backup helper
└── data/                        # Volume mount point (gitignored)
    ├── bitcoin/
    ├── electrs/
    ├── ord/
    └── him/
```

---

## Step 1 — Environment File

Create `.env` in the project root. **Never commit this file.**

```bash
# .env
BITCOIN_NETWORK=regtest          # regtest | testnet | mainnet

# RPC credentials — change before any real use
BITCOIN_RPC_USER=himuser
BITCOIN_RPC_PASS=change_me_before_use_29aX

# Ports exposed on host
BITCOIN_RPC_PORT=18443           # 18332 testnet, 8332 mainnet
BITCOIN_P2P_PORT=18444           # 18333 testnet, 8333 mainnet
ELECTRS_PORT=50001
ORD_PORT=8080
HIMSHA_NODE_PORT=9100

# Mempool.space URL for inscription queries (optional)
MEMPOOL_SPACE_URL=https://mempool.space/testnet

# HIMSHA node database path inside container
HIMSHA_DB=/data/him.redb
```

---

## Step 2 — Bitcoin Core Configuration

### `config/bitcoin/bitcoin.conf`

```ini
# =========================================================
# Bitcoin Core Configuration
# =========================================================

# --- Network ---
# Uncomment exactly ONE of the following:
regtest=1
#testnet=1
#mainnet=1     (remove the line entirely for mainnet)

# --- RPC Server ---
server=1
rpcbind=0.0.0.0
rpcallowip=0.0.0.0/0            # Restrict to 10.0.0.0/8 in production

# Credentials are injected via environment variables in docker-compose
# rpcuser and rpcpassword are set via -rpcuser / -rpcpassword CLI flags

# --- Indexing ---
txindex=1                        # Full transaction index (required by Ord + Electrs)
blockfilterindex=1               # BIP157 compact filters
coinstatsindex=1                 # UTXO statistics

# --- ZMQ (real-time notifications to HIMSHA node) ---
zmqpubrawblock=tcp://0.0.0.0:28332
zmqpubrawtx=tcp://0.0.0.0:28333
zmqpubhashblock=tcp://0.0.0.0:28334
zmqpubhashtx=tcp://0.0.0.0:28335

# --- Performance ---
dbcache=512                      # MB of RAM for UTXO cache (increase for mainnet: 4096+)
maxmempool=300                   # MB
maxconnections=40
par=4                            # Script validation threads

# --- Logging ---
debug=rpc
debug=zmq
logips=0                         # Don't log peer IPs in production
```

---

## Step 3 — Electrs Configuration

### `config/electrs/config.toml`

```toml
# =========================================================
# Electrs Configuration
# Fast UTXO indexer implementing the Electrum server protocol
# =========================================================

# --- Network (must match bitcoin.conf) ---
network = "regtest"              # regtest | testnet | mainnet

# --- Bitcoin Core connection ---
daemon_rpc_addr = "bitcoin:18443"   # host:port — use 18332/8332 for testnet/mainnet
daemon_dir = "/bitcoin-data"
auth = "himuser:change_me_before_use_29aX"

# --- Electrs storage ---
db_dir = "/electrs-data"

# --- Listen addresses ---
electrum_rpc_addr = "0.0.0.0:50001"  # Electrum protocol
http_addr = "0.0.0.0:3002"           # REST HTTP API

# --- Monitoring ---
monitoring_addr = "0.0.0.0:4224"     # Prometheus metrics endpoint

# --- Sync behavior ---
wait_duration_secs = 5               # Poll Bitcoin Core every N seconds
index_batch_size = 10                # Blocks per indexing batch
bulk_index_threads = 4               # Parallel index threads
```

---

## Step 4 — Ord Configuration

Ord is configured primarily via command-line arguments. We also provide `config/ord/ord.toml`:

```toml
# =========================================================
# Ord Configuration
# Ordinals / inscription indexer
# =========================================================

bitcoin_rpc_url = "http://bitcoin:18443"
bitcoin_rpc_username = "himuser"
bitcoin_rpc_password = "change_me_before_use_29aX"

data_dir = "/ord-data"

# Network (regtest | testnet | mainnet)
# Passed via CLI flag --regtest / --testnet

# Inscription content serving
serve_contents = true

# Allow larger inscriptions (useful for development)
max_recoverable_inscription_number = 0
```

---

## Step 5 — Docker Compose

### `docker-compose.yml`

```yaml
version: "3.9"

# =========================================================
# Bitcoin + Electrs + Ord + HIMSHA Node — Docker Compose
# =========================================================

networks:
  bitcoin-net:
    driver: bridge
    ipam:
      config:
        - subnet: 172.20.0.0/24

volumes:
  bitcoin-data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: ${PWD}/data/bitcoin
  electrs-data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: ${PWD}/data/electrs
  ord-data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: ${PWD}/data/ord
  himsha-data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: ${PWD}/data/him

services:

  # ==========================================================
  # Bitcoin Core
  # ==========================================================
  bitcoin:
    image: ruimarinho/bitcoin-core:24
    container_name: bitcoin-core
    restart: unless-stopped
    networks:
      bitcoin-net:
        ipv4_address: 172.20.0.10
    ports:
      - "${BITCOIN_RPC_PORT:-18443}:18443"     # RPC (change port for testnet/mainnet)
      - "${BITCOIN_P2P_PORT:-18444}:18444"     # P2P
      - "28332:28332"                           # ZMQ blocks
      - "28333:28333"                           # ZMQ txs
    volumes:
      - bitcoin-data:/home/bitcoin/.bitcoin
      - ./config/bitcoin/bitcoin.conf:/home/bitcoin/.bitcoin/bitcoin.conf:ro
    command: >
      bitcoind
      -conf=/home/bitcoin/.bitcoin/bitcoin.conf
      -datadir=/home/bitcoin/.bitcoin
      -rpcuser=${BITCOIN_RPC_USER}
      -rpcpassword=${BITCOIN_RPC_PASS}
      -rpcport=18443
    environment:
      BITCOIN_RPC_USER: ${BITCOIN_RPC_USER}
      BITCOIN_RPC_PASS: ${BITCOIN_RPC_PASS}
    healthcheck:
      test:
        - CMD
        - bitcoin-cli
        - -regtest
        - -rpcuser=${BITCOIN_RPC_USER}
        - -rpcpassword=${BITCOIN_RPC_PASS}
        - -rpcport=18443
        - getblockchaininfo
      interval: 15s
      timeout: 10s
      retries: 20
      start_period: 30s
    logging:
      driver: "json-file"
      options:
        max-size: "50m"
        max-file: "5"

  # ==========================================================
  # Electrs — UTXO Indexer
  # ==========================================================
  electrs:
    image: getumbrel/electrs:v0.10.2
    container_name: electrs
    restart: unless-stopped
    networks:
      bitcoin-net:
        ipv4_address: 172.20.0.11
    ports:
      - "${ELECTRS_PORT:-50001}:50001"          # Electrum RPC
      - "3002:3002"                              # HTTP REST API
      - "4224:4224"                              # Prometheus metrics
    volumes:
      - electrs-data:/electrs-data
      - bitcoin-data:/bitcoin-data:ro            # Reads Bitcoin blocks directly
      - ./config/electrs/config.toml:/etc/electrs/config.toml:ro
    command: electrs --conf /etc/electrs/config.toml
    depends_on:
      bitcoin:
        condition: service_healthy
    healthcheck:
      test:
        - CMD
        - curl
        - -sf
        - http://localhost:3002/blocks/tip/height
      interval: 30s
      timeout: 10s
      retries: 10
      start_period: 60s
    logging:
      driver: "json-file"
      options:
        max-size: "50m"
        max-file: "5"

  # ==========================================================
  # Ord — Ordinals / Inscription Indexer
  # ==========================================================
  ord:
    image: ordinals/ord:latest
    container_name: ord
    restart: unless-stopped
    networks:
      bitcoin-net:
        ipv4_address: 172.20.0.12
    ports:
      - "${ORD_PORT:-8080}:8080"
    volumes:
      - ord-data:/ord-data
      - bitcoin-data:/bitcoin-data:ro
    command: >
      ord
      --regtest
      --bitcoin-rpc-url http://bitcoin:18443
      --bitcoin-rpc-username ${BITCOIN_RPC_USER}
      --bitcoin-rpc-password ${BITCOIN_RPC_PASS}
      --data-dir /ord-data
      server
      --http-port 8080
    depends_on:
      bitcoin:
        condition: service_healthy
    healthcheck:
      test:
        - CMD
        - curl
        - -sf
        - http://localhost:8080/status
      interval: 30s
      timeout: 10s
      retries: 10
      start_period: 120s
    logging:
      driver: "json-file"
      options:
        max-size: "50m"
        max-file: "5"

  # ==========================================================
  # HIMSHA Node
  # ==========================================================
  himsha-node:
    build:
      context: ..
      dockerfile: Dockerfile
      target: runtime
    container_name: himsha-node
    restart: unless-stopped
    networks:
      bitcoin-net:
        ipv4_address: 172.20.0.13
    ports:
      - "${HIMSHA_NODE_PORT:-9100}:9100"
    volumes:
      - himsha-data:/data
    environment:
      HIMSHA_DB: ${HIMSHA_DB:-/data/him.redb}
      BITCOIN_RPC_URL: "http://bitcoin:18443"
      BITCOIN_RPC_USER: ${BITCOIN_RPC_USER}
      BITCOIN_RPC_PASS: ${BITCOIN_RPC_PASS}
      ELECTRS_URL: "http://electrs:3002"
      ORD_URL: "http://ord:8080"
      MEMPOOL_SPACE_URL: ${MEMPOOL_SPACE_URL:-}
      RUST_LOG: "himsha_node=info,warn"
    depends_on:
      electrs:
        condition: service_healthy
      ord:
        condition: service_healthy
    healthcheck:
      test:
        - CMD
        - curl
        - -sf
        - -X POST
        - http://localhost:9100
        - -H "Content-Type: application/json"
        - -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}'
      interval: 20s
      timeout: 10s
      retries: 5
      start_period: 15s
    logging:
      driver: "json-file"
      options:
        max-size: "50m"
        max-file: "5"
```

---

## Step 6 — Dockerfile (HIMSHA Node)

Place this in the project root (`../Dockerfile` relative to `bitcoin-infra/`):

```dockerfile
# =========================================================
# HIMSHA Node — Multi-stage Dockerfile
# =========================================================

# --- Stage 1: RISC Zero + Rust builder ---
FROM rust:1.75-slim-bookworm AS builder

WORKDIR /build

# System dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev clang lld cmake git curl \
    && rm -rf /var/lib/apt/lists/*

# Install RISC Zero toolchain
RUN cargo install cargo-risczero \
    && cargo risczero install

# Cache dependencies separately from source
COPY Cargo.toml Cargo.lock ./
COPY himsha-runtime/Cargo.toml    himsha-runtime/
COPY himsha-vm/Cargo.toml         himsha-vm/
COPY himsha-node/Cargo.toml       himsha-node/
COPY himsha-programs/system/Cargo.toml    himsha-programs/system/
COPY himsha-programs/token/Cargo.toml     himsha-programs/token/
COPY himsha-programs/ata/Cargo.toml       himsha-programs/ata/
COPY himsha-programs/swap/Cargo.toml      himsha-programs/swap/
COPY himsha-programs/lending/Cargo.toml   himsha-programs/lending/
COPY himsha-programs/nft-metadata/Cargo.toml himsha-programs/nft-metadata/
COPY himsha-cli/Cargo.toml        himsha-cli/

# Dummy source to cache dependencies
RUN mkdir -p himsha-runtime/src && echo "pub fn main(){}" > himsha-runtime/src/lib.rs \
    && mkdir -p himsha-node/src   && echo "fn main(){}"   > himsha-node/src/main.rs \
    && cargo build --release -p himsha-node 2>/dev/null; true

# Copy actual source
COPY himsha-runtime/src   himsha-runtime/src
COPY himsha-vm/src        himsha-vm/src
COPY himsha-node/src      himsha-node/src
COPY himsha-programs      himsha-programs
COPY himsha-cli/src       himsha-cli/src

# Build release binary
RUN cargo build --release -p himsha-node

# --- Stage 2: Minimal runtime image ---
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -u 1001 -s /sbin/nologin himuser
USER himuser

COPY --from=builder /build/target/release/himsha-node /usr/local/bin/himsha-node

VOLUME ["/data"]
EXPOSE 9100

ENTRYPOINT ["himsha-node"]
```

---

## Step 7 — Helper Scripts

### `scripts/init-regtest.sh` — Fund the regtest wallet

```bash
#!/usr/bin/env bash
# Run once after starting regtest to create and fund a wallet.
set -euo pipefail

RPC="docker compose exec bitcoin bitcoin-cli -regtest \
  -rpcuser=${BITCOIN_RPC_USER} -rpcpassword=${BITCOIN_RPC_PASS}"

echo "Creating wallet..."
$RPC createwallet "main" || $RPC loadwallet "main"

ADDRESS=$($RPC getnewaddress)
echo "Mining 101 blocks to: $ADDRESS"
$RPC generatetoaddress 101 "$ADDRESS"

BALANCE=$($RPC getbalance)
echo "Wallet balance: $BALANCE BTC"
echo "Regtest ready."
```

### `scripts/healthcheck.sh` — Composite health check

```bash
#!/usr/bin/env bash
# Check all services are healthy. Exit 0 = all good, 1 = something wrong.
set -euo pipefail

OK="\033[0;32m✓\033[0m"
FAIL="\033[0;31m✗\033[0m"
all_ok=true

check() {
  local name="$1"; local cmd="$2"
  if eval "$cmd" &>/dev/null; then
    echo -e "$OK $name"
  else
    echo -e "$FAIL $name"
    all_ok=false
  fi
}

check "Bitcoin Core" \
  "docker compose exec bitcoin bitcoin-cli -regtest \
    -rpcuser=${BITCOIN_RPC_USER} -rpcpassword=${BITCOIN_RPC_PASS} \
    getblockchaininfo"

check "Electrs HTTP" \
  "curl -sf http://localhost:3002/blocks/tip/height"

check "Ord API" \
  "curl -sf http://localhost:8080/status"

check "HIMSHA Node" \
  "curl -sf -X POST http://localhost:9100 \
    -H 'Content-Type: application/json' \
    -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"himsha_isNodeReady\",\"params\":[]}'"

$all_ok && echo -e "\nAll services healthy." || { echo -e "\nSome services unhealthy!"; exit 1; }
```

### `scripts/backup.sh` — Data backup

```bash
#!/usr/bin/env bash
# Backup all data volumes to ./backup/
set -euo pipefail

BACKUP_DIR="./backup/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$BACKUP_DIR"

echo "Stopping HIMSHA node before backup..."
docker compose stop himsha-node

for vol in bitcoin electrs ord him; do
  echo "Backing up $vol data..."
  docker run --rm \
    -v "bitcoin-infra_${vol}-data:/source:ro" \
    -v "$BACKUP_DIR:/backup" \
    alpine tar czf "/backup/${vol}.tar.gz" -C /source .
  echo "  → $BACKUP_DIR/${vol}.tar.gz"
done

docker compose start himsha-node
echo "Backup complete: $BACKUP_DIR"
```

---

## Step 8 — Start the Stack

```bash
# Create data directories
mkdir -p data/bitcoin data/electrs data/ord data/him

# Start all services
docker compose up -d

# Watch startup
docker compose logs -f

# Check status
docker compose ps

# Run composite health check
bash scripts/healthcheck.sh

# For regtest: fund the wallet
bash scripts/init-regtest.sh
```

---

## Step 9 — Verify Each Service

```bash
# --- Bitcoin Core ---
# Get blockchain info
curl -s -u "${BITCOIN_RPC_USER}:${BITCOIN_RPC_PASS}" \
  -X POST http://localhost:18443 \
  -H "Content-Type: text/plain" \
  -d '{"method":"getblockchaininfo","params":[],"id":1}' | jq .

# --- Electrs ---
# Current indexed block height
curl -s http://localhost:3002/blocks/tip/height

# Get address UTXOs
curl -s http://localhost:3002/address/bcrt1q.../utxo | jq .

# --- Ord ---
# Indexer status
curl -s http://localhost:8080/status | jq .

# List inscriptions
curl -s http://localhost:8080/inscriptions | jq .

# Look up a specific inscription
curl -s "http://localhost:8080/inscription/<inscription_id>" | jq .

# --- HIMSHA Node ---
# Readiness check
curl -s -X POST http://localhost:9100 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}' | jq .

# Current slot
curl -s -X POST http://localhost:9100 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_getSlot","params":[]}' | jq .

# List deployed programs
curl -s -X POST http://localhost:9100 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_listPrograms","params":[]}' | jq .
```

---

## Step 10 — Network-Specific Config

### Switching from Regtest to Testnet

1. In `.env`: set `BITCOIN_NETWORK=testnet`, `BITCOIN_RPC_PORT=18332`
2. In `config/bitcoin/bitcoin.conf`: replace `regtest=1` with `testnet=1`
3. In `docker-compose.yml` Bitcoin command: replace `-regtest` with `-testnet`
4. In `config/electrs/config.toml`: set `network = "testnet"` and `daemon_rpc_addr = "bitcoin:18332"`
5. In Ord command: replace `--regtest` with `--testnet`

### Mainnet Checklist

- [ ] Minimum 700 GB SSD
- [ ] `dbcache=4096` in bitcoin.conf
- [ ] Remove `regtest=1`; use default Bitcoin port 8332
- [ ] Strong, unique RPC password (32+ chars)
- [ ] Firewall: block ports 8332, 50001, 8080, 9100 from the internet
- [ ] Use a reverse proxy (nginx/caddy) with TLS for any exposed endpoints

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `bitcoin-cli: Connection refused` | Bitcoin not started | Check `docker compose logs bitcoin` |
| Electrs stuck at block 0 | Bitcoin not fully synced | Wait for Bitcoin sync to complete |
| Ord shows 0 inscriptions | Ord index not built | Wait; mainnet index takes hours |
| HIMSHA node not ready | Electrs/Ord not healthy | Check `docker compose ps` |
| Out of disk | Volume full | Add storage or prune old data |

```bash
# Tail a specific service log
docker compose logs -f bitcoin
docker compose logs -f --tail=100 electrs

# Restart one service without stopping others
docker compose restart electrs

# Check resource usage
docker stats

# Remove stopped containers + unused volumes (careful!)
docker system prune -f
```

---

## Docker Compose Reference

```bash
docker compose up -d              # Start all services detached
docker compose down               # Stop and remove containers
docker compose down -v            # Also delete volumes (DESTRUCTIVE)
docker compose pull               # Pull latest images
docker compose ps                 # Service status
docker compose top                # Process list inside containers
docker compose exec bitcoin bash  # Shell into Bitcoin container
```
