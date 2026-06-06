# Testing HIMSHA Locally — Regtest, Testnet & Beyond

How to run and exercise HIMSHA on your own machine, from "no Bitcoin at all" up to a
full regtest stack, testnet/signet, Lightning, and a multi-node failover cluster.

> **Networks at a glance**
>
> | Network | Bitcoin needed? | Funds | Block time | Use it for |
> |---|---|---|---|---|
> | **none** (in-memory) | no | faucet (fake) | instant | program logic, RPC, SDK, CI |
> | **regtest** | yes (local) | you mine them | instant (`generatetoaddress`) | end-to-end settlement, the default |
> | **signet** | yes | free faucet | ~10 min | shared testing, realistic timing |
> | **testnet** | yes | free faucet | ~10 min | pre-mainnet rehearsal |
> | **mainnet** | yes | **real** | ~10 min | ⚠️ not for testing |
>
> `BITCOIN_NETWORK` accepts `regtest` (default), `signet`, `testnet`, `mainnet`/`bitcoin`.

Most development needs **only the first two rows**. Start at level 0 and climb only as far
as the thing you're testing requires.

---

## Level 0 — No Bitcoin (fastest loop)

The node runs perfectly with **no Bitcoin backend**: on-chain settlement is simply skipped
(logged as disabled) and everything else — programs, accounts, blocks, RPC, the SDK, ZK
re-execution — works against an in-memory/redb state. This is the loop you'll use 90% of
the time.

### Run the test suite

```bash
cargo test --workspace          # all crates: runtime, vm, programs, node, threshold
cargo test -p himsha-threshold  # e.g. just the FROST / ROAST custody tests
cargo test -p himsha-node       # node: election, follower, settlement, lightning
```

### Run a node + drive it with the faucet

```bash
# Terminal 1 — a dev node with the faucet enabled
HIMSHA_DB=/tmp/himsha-dev \
HIMSHA_FAUCET=1 \
cargo run -p himsha-node
# → "HIMSHA node listening on http://127.0.0.1:9100"
```

```bash
# Terminal 2 — talk to it
# liveness
curl -s -X POST http://127.0.0.1:9100 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}'

# fund a fresh account (faucet; only works because HIMSHA_FAUCET=1)
curl -s -X POST http://127.0.0.1:9100 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"himsha_requestAirdrop","params":["<pubkey>",1000000]}'

# read it back
curl -s -X POST http://127.0.0.1:9100 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":3,"method":"himsha_getAccountInfo","params":["<pubkey>"]}'
```

> The faucet is **off by default** and gated behind `HIMSHA_FAUCET=1`, with a per-request
> cap (`HIMSHA_FAUCET_MAX`, default `1_000_000_000`). Never set it on a public node.

### Use the CLI instead of curl

```bash
cargo run -p himsha-cli -- node status
cargo run -p himsha-cli -- node slot
cargo run -p himsha-cli -- program list
cargo run -p himsha-cli -- account get <pubkey>
cargo run -p himsha-cli -- account list <program_id>
# all commands accept --rpc-url (default http://127.0.0.1:9100)

# scaffold a new program crate from a compiling counter template:
cargo run -p himsha-cli -- program new escrow
# → himsha-programs/escrow/{Cargo.toml,src/lib.rs}; then `cargo test -p himsha-escrow-program`
```

### Use the TypeScript SDK

```bash
cd himsha-sdk && npm install && npm run build && npm test
```

```ts
import { HimshaConnection } from '@himsha-network/sdk';
const conn = new HimshaConnection('http://localhost:9100');
console.log('ready:', await conn.isNodeReady());
console.log('slot :', await conn.getSlot());
```

---

## Level 1 — Regtest (full end-to-end settlement)

Regtest is a private Bitcoin network where **you mine blocks instantly**, so you can test
the parts that touch real Bitcoin — UTXO lookups, on-chain loan settlement, Taproot
threshold spends — without waiting or spending anything.

### 1a. Start Bitcoin Core in regtest

The fastest route is the Docker stack in
[bitcoin-indexer-docker.md](./bitcoin-indexer-docker.md) (Bitcoin Core + Electrs + Ord).
For a bare `bitcoind` it's just:

```bash
bitcoind -regtest -daemon \
  -rpcuser=himsha -rpcpassword=himsha \
  -rpcbind=127.0.0.1 -rpcport=18443 -fallbackfee=0.0001 -txindex=1
```

Create a wallet and mine some blocks to yourself (coins mature after 100 blocks):

```bash
BCLI="bitcoin-cli -regtest -rpcuser=himsha -rpcpassword=himsha -rpcport=18443"
$BCLI createwallet test
ADDR=$($BCLI getnewaddress)
$BCLI generatetoaddress 101 "$ADDR"     # 101 blocks → 50 BTC spendable
$BCLI getbalance
```

### 1b. Point HIMSHA at it

```bash
HIMSHA_DB=/tmp/himsha-regtest \
HIMSHA_FAUCET=1 \
BITCOIN_RPC_URL=http://127.0.0.1:18443 \
BITCOIN_RPC_USER=himsha \
BITCOIN_RPC_PASS=himsha \
BITCOIN_NETWORK=regtest \
BITCOIN_SYNC_INTERVAL_SECS=5 \
cargo run -p himsha-node
# → "bitcoin indexer auto-sync enabled (5s interval)"
```

When `BITCOIN_RPC_URL` is set the node starts the **indexer auto-sync** loop, and
`himsha_getUtxo` / Bitcoin-backed settlement become live:

```bash
# look up a real regtest UTXO
TXID=$($BCLI sendtoaddress "$ADDR" 1.0); $BCLI generatetoaddress 1 "$ADDR"
curl -s -X POST http://127.0.0.1:9100 -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"himsha_getUtxo\",\"params\":[\"$TXID\",0]}"
```

### 1c. Test on-chain loan settlement

The lending program queues settlements that the node drains and pushes to Bitcoin. To watch
a real settlement: submit a lending repayment via the SDK/CLI, then mine a block and confirm
the UTXO moved with `bitcoin-cli`. Mine on demand to advance "time":

```bash
$BCLI generatetoaddress 1 "$ADDR"      # force confirmation / advance the chain
```

### 1d. Verify the FROST→Taproot threshold settlement on-chain

There's a ready-made end-to-end test that funds the committee's Taproot address, has the
M-of-N committee threshold-sign a key-spend, **broadcasts it, and asserts Bitcoin Core
accepts and confirms it** — proving the tweak/sighash/witness/fee against real consensus,
not just in code. It's `#[ignore]`d so normal runs stay offline; point it at your regtest
node and run it explicitly:

```bash
HIMSHA_REGTEST=1 \
BITCOIN_RPC_URL=http://127.0.0.1:18443 \
BITCOIN_RPC_USER=himsha BITCOIN_RPC_PASS=himsha \
  cargo test -p himsha-node --test regtest_broadcast -- --ignored --nocapture
# ✅ committee key-spend accepted: <txid>
# ✅ confirmed in a block — FROST→Taproot settlement verified end-to-end
```

See [decentralization.md](./decentralization.md) §2 for the custody design.

---

## Level 2 — Signet / Testnet

Same as regtest but pointed at a public test network — useful for realistic block timing
and sharing state with others. You **don't mine**; you request coins from a faucet.

```bash
# bitcoind on signet (recommended over testnet: faster, more stable)
bitcoind -signet -daemon -rpcuser=himsha -rpcpassword=himsha -rpcport=38332 -txindex=1

# HIMSHA
HIMSHA_DB=/tmp/himsha-signet \
BITCOIN_RPC_URL=http://127.0.0.1:38332 \
BITCOIN_RPC_USER=himsha BITCOIN_RPC_PASS=himsha \
BITCOIN_NETWORK=signet \
cargo run -p himsha-node
```

- **Signet faucet:** https://signetfaucet.com
- **Testnet faucet:** search "bitcoin testnet faucet" (use `BITCOIN_NETWORK=testnet`,
  default RPC port `18332`)
- Expect to wait for confirmations (~10 min/block) — there's no `generatetoaddress`.
- Initial block download takes hours; `-txindex=1` is required for UTXO lookups.

`MEMPOOL_SPACE_URL` can be set to a mempool.space-style endpoint for fee/tx data on these
networks.

---

## Level 3 — Lightning (regtest)

To exercise the Lightning settlement rail, run an LND node and set two env vars. The
easiest local setup is [Polar](https://lightningpolar.com) (one-click regtest
Bitcoin+LND), or a manual `lnd` in regtest.

```bash
LND_REST_URL=https://127.0.0.1:8080 \
LND_MACAROON_HEX=$(xxd -ps -u -c 1000 ~/.lnd/data/chain/bitcoin/regtest/admin.macaroon) \
# ...plus the Level-1 regtest BITCOIN_* vars...
cargo run -p himsha-node
```

```bash
# create / pay invoices through HIMSHA
curl -s -X POST http://127.0.0.1:9100 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_createInvoice","params":[1000,"test"]}'
curl -s -X POST http://127.0.0.1:9100 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"himsha_lightningBalance","params":[]}'
```

Without LND configured these return a clean `-32040 lightning not configured` error — which
is itself the test of the unconfigured path. Full setup and caveats: [lightning.md](./lightning.md).

---

## Level 4 — Multi-node failover cluster

Test the ZK-verifying followers and the PreVote quorum election locally by running several
nodes on different ports. See [decentralization.md](./decentralization.md) for the design.

### A primary + a ZK-verifying follower

```bash
# primary
HIMSHA_DB=/tmp/n1 HIMSHA_BIND=127.0.0.1:9100 cargo run -p himsha-node

# follower — replicates by re-deriving each block (does not trust the primary's values)
HIMSHA_DB=/tmp/n2 HIMSHA_BIND=127.0.0.1:9101 \
HIMSHA_FOLLOW=http://127.0.0.1:9100 \
HIMSHA_FOLLOW_INTERVAL_SECS=2 \
cargo run -p himsha-node
# follower logs: "follower replicated block slot=… txs=…"
```

### Partition-safe failover (PreVote quorum election)

Run 3 members; give each the full member set and its own id. Enable failover with
`HIMSHA_FAILOVER_MISSES`:

```bash
# node A (start as primary)
HIMSHA_DB=/tmp/a HIMSHA_BIND=127.0.0.1:9100 \
HIMSHA_SELF=http://127.0.0.1:9100 \
HIMSHA_ELECTION_MEMBERS=http://127.0.0.1:9100,http://127.0.0.1:9101,http://127.0.0.1:9102 \
cargo run -p himsha-node

# nodes B and C (followers that can be elected)
HIMSHA_DB=/tmp/b HIMSHA_BIND=127.0.0.1:9101 \
HIMSHA_SELF=http://127.0.0.1:9101 \
HIMSHA_FOLLOW=http://127.0.0.1:9100 \
HIMSHA_FAILOVER_MISSES=3 \
HIMSHA_ELECTION_MEMBERS=http://127.0.0.1:9100,http://127.0.0.1:9101,http://127.0.0.1:9102 \
cargo run -p himsha-node
# (node C identical with :9102 and its own HIMSHA_SELF)
```

**What to test:**

```bash
# who's the leader?
curl -s -X POST http://127.0.0.1:9101 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_getLeader","params":[]}'

# kill node A → after 3 missed polls, B or C runs PreVote, then a real election,
#   and exactly one promotes: log line "FAILOVER: promoted to sequencer".
# A minority (single node, no quorum) logs "1/3 votes — no quorum" and refuses —
#   the split-brain guard.
```

| Env var | Meaning |
|---|---|
| `HIMSHA_FOLLOW` | URL of the primary to replicate from (makes this a follower) |
| `HIMSHA_FOLLOW_INTERVAL_SECS` | poll interval (default 5) |
| `HIMSHA_FAILOVER_MISSES` | promote after N consecutive failed polls (unset = never) |
| `HIMSHA_ELECTION_MEMBERS` | comma-separated full member set → enables PreVote quorum election |
| `HIMSHA_SELF` | this node's own URL (its identity in the election) |
| `HIMSHA_STANDBY_PEERS` | crash-safe fallback: higher-priority peers to defer to (no quorum) |

---

## Reference — environment variables

| Variable | Default | Purpose |
|---|---|---|
| `HIMSHA_DB` | `himsha.redb` | state database path |
| `HIMSHA_BIND` | `127.0.0.1:9100` | RPC bind address |
| `HIMSHA_FAUCET` | off | `1` enables the dev faucet (`requestAirdrop`, `createAccountWithFaucet`) |
| `HIMSHA_FAUCET_MAX` | `1000000000` | per-request faucet cap (lamports) |
| `HIMSHA_NETWORK` / `HIMSHA_NETWORK_PUBKEY` | — | network identity advertised via `getNetworkPubkey` |
| `BITCOIN_RPC_URL` / `_USER` / `_PASS` | — | Bitcoin Core RPC; presence enables the indexer auto-sync |
| `BITCOIN_NETWORK` | `regtest` | `regtest` / `signet` / `testnet` / `mainnet` |
| `BITCOIN_SYNC_INTERVAL_SECS` | (impl default) | indexer poll interval |
| `MEMPOOL_SPACE_URL` | — | optional mempool.space-style fee/tx endpoint |
| `LND_REST_URL` / `LND_MACAROON_HEX` | — | Lightning (LND REST); presence enables Lightning RPC |
| `HIMSHA_FOLLOW` / `_INTERVAL_SECS` | — | follower replication target + poll interval |
| `HIMSHA_FAILOVER_MISSES` | — | enable self-promotion after N missed polls |
| `HIMSHA_ELECTION_MEMBERS` / `HIMSHA_SELF` | — | PreVote quorum election member set + identity |
| `HIMSHA_STANDBY_PEERS` | — | crash-safe failover priority peers |

---

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `bitcoin indexer auto-sync disabled` in logs | `BITCOIN_RPC_URL` not set — expected at Level 0 |
| `himsha_getUtxo` returns null | UTXO unconfirmed (mine a block) or `-txindex=1` missing on `bitcoind` |
| `faucet disabled (set HIMSHA_FAUCET=1)` | the faucet is gated; set the env var for dev only |
| `lightning not configured` (-32040) | `LND_REST_URL`/`LND_MACAROON_HEX` unset — expected without LND |
| follower never catches up | wrong `HIMSHA_FOLLOW` URL, or primary not producing blocks |
| two nodes both think they're leader | you used `HIMSHA_STANDBY_PEERS` (crash-safe) across a partition — use `HIMSHA_ELECTION_MEMBERS` (quorum) instead |
| `curl` returns empty right after start | the node is still compiling/binding — wait for the "listening" log line |
| coins not spendable on regtest | mine 100+ blocks; coinbase outputs mature after 100 confirmations |

---

## See also

- [bitcoin-indexer-docker.md](./bitcoin-indexer-docker.md) — full Docker stack (Core + Electrs + Ord)
- [bitcoin-indexer-k8s-terraform.md](./bitcoin-indexer-k8s-terraform.md) — cluster setup
- [lightning.md](./lightning.md) — Lightning integration & caveats
- [decentralization.md](./decentralization.md) — followers, FROST/ROAST custody, PreVote failover
- [deployment/](./deployment/README.md) — production deployment (bare metal / AWS / GCP / Azure)
