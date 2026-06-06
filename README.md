<p align="center">
  <img src="./assets/banner.svg" alt="HIMSHA Network — ZK-proven Bitcoin programmability" width="100%">
</p>

<p align="center">
  <a href="#implementation-status"><img alt="status: proof of concept" src="https://img.shields.io/badge/status-proof--of--concept-orange"></a>
  <img alt="tests: 182 passing" src="https://img.shields.io/badge/tests-182%20passing-brightgreen">
  <a href="./LICENSE"><img alt="license: MIT" src="https://img.shields.io/badge/license-MIT-blue"></a>
  <img alt="not audited" src="https://img.shields.io/badge/security-not%20audited-critical">
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white">
  <img alt="RISC Zero zkVM" src="https://img.shields.io/badge/RISC%20Zero-zkVM-5B2A86">
  <img alt="Bitcoin" src="https://img.shields.io/badge/Bitcoin-F7931A?logo=bitcoin&logoColor=white">
  <img alt="Taproot / Schnorr (BIP-340)" src="https://img.shields.io/badge/Taproot-Schnorr%20BIP--340-4B5563">
  <img alt="secp256k1" src="https://img.shields.io/badge/secp256k1-FROST%20threshold-2D6A4F">
  <img alt="Lightning (LND)" src="https://img.shields.io/badge/Lightning-LND-792EE5">
  <img alt="Tokio" src="https://img.shields.io/badge/async-Tokio-0B7261">
  <img alt="redb" src="https://img.shields.io/badge/storage-redb-1F6FEB">
</p>

# HIMSHA Network

> **DISCLAIMER: This project is for educational purposes only. It is NOT production-grade software. Do not use it with real Bitcoin mainnet funds. The ZK proofs, consensus mechanisms, and Bitcoin integration are proof-of-concept implementations that have not been audited or tested for production use.**

HIMSHA (Hashable Instruction Machine) is an experimental Bitcoin programmability layer. Every state transition is proven correct by a RISC Zero ZK receipt — not validator majority vote.

---

## Architecture

```
User → himsha-node (JSON-RPC :9100)
           ↓ RuntimeTransaction
       himsha-vm (RISC Zero zkVM)
           ↓ ZK receipt + new account state
       himsha-runtime (account model, UTXO anchoring)
           ↓ commit state to Bitcoin UTXO
       Bitcoin L1 (final settlement)
```

Unlike consensus-only systems, HIMSHA uses ZK proofs to guarantee program correctness independently of validator honesty.

---

## Implementation Status

A snapshot of the core security model. **Done** = implemented and enforced, covered by
the Rust test suite (182 tests, `cargo test --workspace`). **Under testing / partial** =
the mechanism exists but isn't fully hardened, or needs an external system (the RISC Zero
toolchain or a Bitcoin regtest node) to exercise end-to-end.

### ✅ Done (enforced + unit-tested)

| Area | What's enforced |
|------|-----------------|
| **Transaction signatures** | BIP-340 Schnorr (secp256k1); the node rejects unsigned, forged, or tampered transactions at ingestion. |
| **Replay protection** | Signed `recent_blockhash` + `chain_id`, recent-window expiry, and txid de-duplication. |
| **Writable enforcement** | `write_data` refuses to mutate an account an instruction declared read-only. |
| **CPI depth limit** | Nested cross-program calls bounded (`MAX_CPI_DEPTH = 4`) — no stack-blow DoS. |
| **Atomic state** | A transaction's account writes commit in a single DB transaction (all-or-nothing). |
| **ZK receipt binding** | The node persists a state transition only if its receipt commits to exactly the accounts produced (native integrity gate). |
| **Programs** | system, token, ATA, swap (AMM), lending, money-market (interest-bearing cToken lender shares), yield vault (lends idle assets via CPI, NAV from share price, auto-undeploy on withdraw), NFT metadata, runes, oracle. |

### 🧪 Under testing / partial

| Area | Status |
|------|--------|
| **ZK proving (soundness)** | Native execution is integrity-checked but not cryptographically proven. The verified-receipt path needs the RISC Zero toolchain (`--features zkvm`); `proof_bytes` currently holds the journal, not a re-verifiable STARK seal. |
| **Account owner enforcement** | Writability is enforced; "only the owning program may write" needs the program id threaded through every program (not yet done). |
| **Compute metering** | CPI depth is bounded; arbitrary in-program loops/compute are bounded only on the zkVM path (cycle limit), not native dispatch. |
| **Execution timing** | Per-tx writes are atomic, but execution still happens at RPC time (before block inclusion); moving it to block production remains. |
| **Bitcoin L1 anchoring** | No commitment of block/state roots to Bitcoin yet. |
| **Consensus replication** | Raft *election* safety only — no log replication / commit index; followers re-derive by polling the leader. |
| **Threshold custody** | FROST/Taproot committee settlement code exists but isn't wired; live settlement uses a single hot wallet. Needs regtest to verify. |
| **Lightning** | Requires an external LND node; unverified without one. |

> See [`CONTRIBUTING.md`](./CONTRIBUTING.md) and the root disclaimer — this is an
> educational proof of concept, not audited or production-ready.

---

## Programs

| Package | Description |
|---------|-------------|
| `himsha-programs/system` | Account creation, lamport transfer, ownership |
| `himsha-programs/token` | Fungible token (mint, transfer, burn, freeze) |
| `himsha-programs/ata` | Deterministic per-user token accounts |
| `himsha-programs/swap` | Constant-product AMM (x·y=k) |
| `himsha-programs/lending` | Bitcoin Ordinals collateral lending |
| `himsha-programs/nft-metadata` | On-chain NFT name, symbol, URI, royalties |
| `himsha-programs/runes` | Bitcoin Runes fungible tokens (etch, open-mint, transfer, burn) |
| `himsha-programs/money-market` | Over-collateralized borrowing (supply, borrow, repay, interest accrual, liquidation) |
| `himsha-programs/vault` | Automated yield vault (ERC-4626-style shares, keeper-reported NAV, performance fees) |

---

## Quick Start

### Prerequisites

- Rust 1.75+
- Bitcoin Core (regtest for local dev)
- RISC Zero toolchain: `cargo install cargo-risczero && cargo risczero install`

### Build

```bash
cargo build --workspace
```

### Run a local node

```bash
HIMSHA_DB=./him.redb cargo run -p himsha-node
# Node listens on http://127.0.0.1:9100
```

### CLI

```bash
# Check node status
himsha node status

# Deploy a program
himsha deploy --elf ./target/deploy/my_program.so --image-id <hex>

# Query an account
himsha account get <pubkey>

# Get current slot
himsha node slot
```

### JSON-RPC

All methods use `http://localhost:9100` with `Content-Type: application/json`:

```bash
# Check readiness
curl -X POST http://localhost:9100 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}'

# Get slot
curl -X POST http://localhost:9100 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_getSlot","params":[]}'
```

---

## RPC Reference

| Method | Params | Returns |
|--------|--------|---------|
| `himsha_sendTransaction` | `RuntimeTransaction` | tx id (hex) |
| `himsha_getAccountInfo` | `pubkey: String` | `AccountInfo \| null` |
| `himsha_getProgramAccounts` | `program_id: String` | `AccountInfo[]` |
| `himsha_deployProgram` | `elf_hex, image_id_hex` | program pubkey |
| `himsha_getBlock` | `slot: u64` | `Block \| null` |
| `himsha_getSlot` | — | `u64` |
| `himsha_isNodeReady` | — | `bool` |
| `himsha_listPrograms` | — | `String[]` |
| `himsha_getUtxo` | `txid, vout` | `UtxoInfo \| null` |
| `himsha_requestAirdrop` | `pubkey, lamports` | new balance (dev faucet; `HIMSHA_FAUCET=1`) |
| `himsha_getMultipleAccounts` | `pubkeys[]` | `(AccountInfo \| null)[]` |
| `himsha_getProcessedTransaction` | `txid` | `RuntimeTransaction \| null` |
| `himsha_getVersion` | — | `String` |
| `himsha_getPeers` | — | `String[]` |
| `himsha_createAccountWithFaucet` | `pubkey, lamports, space` | `AccountInfo` (dev faucet) |
| `himsha_sendTransactions` | `RuntimeTransaction[]` | tx ids `String[]` |
| `himsha_recentTransactions` | `limit` | `RuntimeTransaction[]` |
| `himsha_getAccountAddress` | `pubkey` | Bitcoin P2TR address |
| `himsha_getBlockHash` | `slot` | hash hex \| null |
| `himsha_getBestBlockHash` | — | hash hex \| null |
| `himsha_getNetworkPubkey` | — | `String` |
| `himsha_preVote` | `term, candidate` | `VoteReply` (Raft PreVote; non-binding) |
| `himsha_requestVote` | `term, candidate` | `VoteReply` (Raft election) |
| `himsha_getLeader` | — | `LeaderInfo` (heartbeat / re-point) |
| `himsha_createInvoice` | `amount_sat, memo` | BOLT-11 string ⚡ |
| `himsha_payInvoice` | `bolt11` | payment hash ⚡ |
| `himsha_lightningBalance` | — | channel balance (sats) ⚡ |
| `himsha_getAllAccounts` | `limit` | `AccountInfo[]` (0 = all) |
| `himsha_getTxidFromBtcTxid` | `btc_txid` | HIMSHA txid \| null (settlement lookup) |
| `himsha_getStats` | — | `{accounts, transactions, tip_slot, programs}` (indexed) |

⚡ Lightning methods require an LND node configured via `LND_REST_URL` +
`LND_MACAROON_HEX`; otherwise they return error `-32040` (*lightning not
configured*). See [docs/lightning.md](docs/lightning.md).

---

## Repository Layout

```
bitcoin-tect/
├── himsha-runtime/        Core types (accounts, transactions, UTXO, ZK receipt)
├── himsha-vm/             RISC Zero zkVM executor + program registry
├── himsha-node/           JSON-RPC node, Bitcoin indexer, block producer
├── himsha-programs/
│   ├── system/         System program
│   ├── token/          Token program
│   ├── ata/            Associated Token Account program
│   ├── swap/           AMM swap program
│   ├── lending/        Ordinals lending program
│   ├── nft-metadata/   NFT metadata program
│   ├── runes/          Bitcoin Runes program
│   └── money-market/   Over-collateralized borrowing
├── himsha-cli/            Command-line tool
└── docs/               Infrastructure setup guides
```

---

## Program execution (native vs. zkVM)

Each built-in program is a plain Rust crate exposing `process()`. There are two execution paths:

- **Native dispatch** (default today) — the node runs built-in programs directly via
  [`himsha-vm::dispatch`](himsha-vm/src/dispatch.rs). Execution is deterministic and produces the
  same state transition the guest would, but **skips proof generation** (the receipt is marked
  `verified: false`). This lets the node run end-to-end without the RISC Zero toolchain.
- **zkVM proving** — deployed (non-built-in) programs, and built-ins once compiled to guest ELFs,
  run through `ProgramExecutor::execute` which generates and verifies a RISC Zero receipt.

Enable proving for **all** programs (built-ins via a universal RISC Zero guest) with the opt-in
`zkvm` feature — this requires the RISC Zero toolchain (`cargo-risczero` + `r0vm`):

```bash
cargo run -p himsha-node --features zkvm
```

See [`docs/zkvm-proving.md`](./docs/zkvm-proving.md) for the architecture and caveats, and
[`CONTRIBUTING.md`](./CONTRIBUTING.md) to contribute.

---

## Use Cases

The product suite built on these programs — see [`docs/use-cases/`](./docs/use-cases/README.md):

- [Swap](./docs/use-cases/swap.md) — native BTC trading with atomic settlement
- [Lend](./docs/use-cases/lend.md) — Bitcoin-backed credit with fast liquidation
- [Prime](./docs/use-cases/prime.md) — real-time portfolio management
- [Yield Vaults](./docs/use-cases/yield-vaults.md) — automated yield strategies
- [AI Copilot](./docs/use-cases/ai-copilot.md) — LLM + RAG advisory across all four

Per-module developer docs: [`docs/modules/`](./docs/modules/README.md).

---

## Infrastructure Guides

See [`docs/`](./docs/) for:

- [Testing locally — regtest, testnet, Lightning, failover](./docs/testing-locally.md)
- [Bitcoin Indexer + Ord Setup with Docker](./docs/bitcoin-indexer-docker.md)
- [Bitcoin Indexer + Ord Setup with Kubernetes & Terraform](./docs/bitcoin-indexer-k8s-terraform.md)
- [ZK Proving (RISC Zero guest) — opt-in](./docs/zkvm-proving.md)
- [Lightning Network integration ⚡](./docs/lightning.md)

### Deployment

- [Deployment guides overview](./docs/deployment/README.md)
- [Bare metal](./docs/deployment/bare-metal.md)
- [AWS (EC2 / ECS Fargate)](./docs/deployment/aws.md)
- [Google Cloud (Compute Engine / Cloud Run)](./docs/deployment/gcp.md)
- [Azure (VM / Container Apps)](./docs/deployment/azure.md)
