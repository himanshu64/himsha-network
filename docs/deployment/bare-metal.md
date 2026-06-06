# Deploying HIMSHA Network on Bare Metal

> **Educational / proof-of-concept.** Do not run with real mainnet funds. See the
> [root README](../../README.md) disclaimer.

This guide deploys a `himsha-node` on a self-managed Linux server (Ubuntu 22.04+ /
Debian 12 assumed; adapt for RHEL/Arch).

---

## 1. What you're deploying

| Component | Purpose | Listens on |
|-----------|---------|------------|
| `himsha-node` | JSON-RPC node, block producer, state DB | `127.0.0.1:9100` |
| Bitcoin Core *(optional)* | UTXO source for `himsha_getUtxo` + loan settlement | `:8332` (RPC) |
| `ord` *(optional)* | Ordinals/inscription indexing | `:80` (HTTP) |

`himsha-node` runs **without** Bitcoin Core (built-in programs execute via native
dispatch; `himsha_getUtxo` returns `null` and lending settlements are logged but not
broadcast until Bitcoin RPC is configured).

---

## 2. Prerequisites

```bash
# System packages
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev git curl

# Rust 1.75+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# (Optional) RISC Zero toolchain — only for ZK-proven execution (--features zkvm)
cargo install cargo-risczero
cargo risczero install
```

## 3. Build

```bash
git clone <your-repo-url> himsha-network && cd himsha-network/bitcoin-tect
cargo build --release -p himsha-node
# binary at target/release/himsha-node
```

For ZK proving:

```bash
cargo build --release -p himsha-node --features zkvm   # requires the RISC Zero toolchain
```

## 4. Create a service user and data dir

```bash
sudo useradd --system --create-home --shell /usr/sbin/nologin him
sudo mkdir -p /var/lib/him
sudo install -m 0755 target/release/himsha-node /usr/local/bin/himsha-node
sudo chown -R him:him /var/lib/him
```

## 5. Configuration (environment)

| Variable | Default | Notes |
|----------|---------|-------|
| `HIMSHA_DB` | `him.redb` | redb state file path |
| `RUST_LOG` | — | e.g. `himsha_node=info` |
| `BITCOIN_RPC_URL` | — | e.g. `http://127.0.0.1:8332` (enables `getUtxo` + settlement) |
| `BITCOIN_RPC_USER` / `BITCOIN_RPC_PASS` | — | Bitcoin Core RPC creds |
| `BITCOIN_NETWORK` | `regtest` | `mainnet` / `testnet` / `signet` / `regtest` |
| `MEMPOOL_SPACE_URL` | — | optional inscription lookups |

The node binds `127.0.0.1:9100`. To expose it, front it with a reverse proxy
(see step 7) — do **not** bind it directly to a public interface.

## 6. systemd service

`/etc/systemd/system/himsha-node.service`:

```ini
[Unit]
Description=HIMSHA Network Node
After=network-online.target
Wants=network-online.target

[Service]
User=him
Group=him
Environment=HIMSHA_DB=/var/lib/him/him.redb
Environment=RUST_LOG=himsha_node=info
# Uncomment to connect to a local Bitcoin Core:
# Environment=BITCOIN_RPC_URL=http://127.0.0.1:8332
# Environment=BITCOIN_RPC_USER=him
# Environment=BITCOIN_RPC_PASS=changeme
# Environment=BITCOIN_NETWORK=signet
ExecStart=/usr/local/bin/himsha-node
Restart=on-failure
RestartSec=5
# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/him
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now himsha-node
sudo systemctl status himsha-node
journalctl -u himsha-node -f
```

## 7. Expose with TLS (nginx)

```nginx
server {
    listen 443 ssl;
    server_name rpc.example.com;
    ssl_certificate     /etc/letsencrypt/live/rpc.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/rpc.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:9100;
        proxy_set_header Content-Type application/json;
        # Add auth (e.g. an API key header check) before exposing publicly.
    }
}
```

Open only 443 in the firewall; keep 9100 bound to localhost:

```bash
sudo ufw allow 443/tcp
sudo ufw enable
```

## 8. Verify

```bash
curl -s -X POST http://127.0.0.1:9100 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}'
# {"jsonrpc":"2.0","result":true,"id":1}

curl -s -X POST http://127.0.0.1:9100 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_listPrograms","params":[]}'
# 8 built-in program IDs
```

## 9. Bitcoin Core (optional, for settlement)

```bash
# Install bitcoind, then run e.g. signet:
bitcoind -signet -server -rpcuser=him -rpcpassword=changeme -txindex=1 -daemon
```

Set the matching `BITCOIN_RPC_*` env vars and restart `himsha-node`. For Ordinals
loan settlement you also need a wallet that controls the inscription UTXOs and,
for inscription indexing, `ord`. See [bitcoin-indexer-docker.md](../bitcoin-indexer-docker.md).

## 10. Backups & upgrades

- **Backup**: stop the node, copy `$HIMSHA_DB` (single redb file), restart.
- **Upgrade**: `git pull && cargo build --release -p himsha-node`, then
  `sudo install -m0755 target/release/himsha-node /usr/local/bin/ && sudo systemctl restart himsha-node`.

## Operational checklist

- [ ] `himsha-node` bound to localhost, fronted by TLS proxy with auth
- [ ] `HIMSHA_DB` on a backed-up volume
- [ ] `journalctl` / log shipping configured
- [ ] Bitcoin RPC creds stored as systemd env or a secrets file (not in git)
