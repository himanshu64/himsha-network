# Contributing to HIMSHA Network

Thanks for your interest in HIMSHA (Hashable Instruction Machine). This is an
**educational, proof-of-concept** Bitcoin programmability layer — see the
disclaimer in the [README](./README.md). Contributions that improve clarity,
correctness, and test coverage are very welcome.

---

## Getting started

### Prerequisites

- Rust 1.75+ (`rustup` recommended)
- Bitcoin Core (regtest) for node integration work
- *Optional:* RISC Zero toolchain for real ZK proving:
  `cargo install cargo-risczero && cargo risczero install`

### Build & test

```bash
cargo build --workspace
cargo test  --workspace
```

Both must pass cleanly before you open a PR. Keep the tree warning-free for code
you touch (`cargo clippy --workspace` is encouraged).

---

## Project layout

| Crate | Responsibility |
|-------|----------------|
| `himsha-runtime` | Core shared types (accounts, tx, UTXO, receipts, errors, program IDs) |
| `himsha-vm` | Execution engine — native `dispatch` for built-ins + RISC Zero `executor` |
| `himsha-node` | JSON-RPC node, block producer, Bitcoin indexer, redb state |
| `himsha-programs/*` | On-chain programs (system, token, ata, swap, lending, nft-metadata, runes) |
| `himsha-cli` | Command-line client |

### How a program runs

Every program is a normal Rust crate exposing:

```rust
pub fn process(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError>;
// (lending & runes also take a `timestamp: u64`)
```

The node executes built-ins **natively** through `himsha-vm::dispatch`. Real RISC Zero
proving (compiling each program to a guest ELF via `risc0-build`) is future work and
requires the RISC Zero toolchain.

---

## Adding a new program

1. Create `himsha-programs/<name>/` with a `Cargo.toml` depending on `himsha-runtime`.
2. Define an instruction enum (borsh), builders, and a `process()` entry point.
3. Add a `program_ids::<name>_program()` seed in
   [`himsha-runtime/src/lib.rs`](himsha-runtime/src/lib.rs) and include it in `builtins()`.
4. Wire it into [`himsha-vm/src/dispatch.rs`](himsha-vm/src/dispatch.rs) and add the path
   dependency in `himsha-vm/Cargo.toml`.
5. Add the crate to the workspace `members` in the root `Cargo.toml`.
6. Add unit tests covering happy path + every error branch.
7. Document it in the README program table.

---

## Coding conventions

- Use `checked_add` / `checked_sub` / `checked_mul` for all balance and reserve math;
  return `ProgramError::Overflow` rather than panicking.
- Validate account counts (`NotEnoughAccounts`) before indexing.
- Prefer precise errors (`NotInitialized`, `Unauthorized`, `SlippageExceeded`, …).
- Match the surrounding style: borsh state structs, instruction builders, `process()` dispatch.

---

## Commit & PR guidelines

- Branch from `main`: `feature/<short-desc>` or `fix/<short-desc>`.
- Keep commits focused; write imperative subject lines (`add runes program`).
- Fill out the PR template and confirm `cargo build` + `cargo test` are green.
- Describe **what** changed and **why**, and call out anything intentionally left
  as a stub or follow-up.

---

## Known follow-ups / good first issues

- Harden the opt-in RISC Zero guest path (`--features zkvm`, see
  [`docs/zkvm-proving.md`](./docs/zkvm-proving.md)): verify it in CI with the toolchain,
  and consider one image id per program instead of the shared universal guest.
- Cross-program invocation (CPI) so `swap` actually moves tokens via the token program.
- Money-market refinements: a close factor capping liquidation size, a protocol
  reserve cut on interest, and a price oracle (`himsha-programs/money-market` covers
  supply/borrow/repay, LTV/health, interest accrual, and liquidation today).
- Broadcast Ordinals loan settlements: the lending program queues settlement
  directives (with interest, partial repay, and bid cancellation) and the node
  drains/logs them — wire the actual Bitcoin transaction build + broadcast to the indexer.
- Full per-signer Schnorr verification at the node (programs now enforce `is_signer`
  via `AccountInfo`/`cpi::invoke_signed_indexed`; the node still only checks signature
  count). Extend signer checks to the ATA program.
- Wire the lending settlement broadcaster and `himsha_getUtxo` to a running Bitcoin
  indexer (set `BITCOIN_RPC_URL`/`USER`/`PASS`); both are env-gated today.
