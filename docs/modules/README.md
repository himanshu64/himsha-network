# HIMSHA Network — Module Guide

How to use each module of the workspace on its own. Every module is a normal Rust
crate (or, for the SDKs, a language package); this guide shows what each one is for
and how to call it independently.

> Educational / proof-of-concept — see the [root README](../../README.md) disclaimer.

## The two ways to use HIMSHA

1. **As a client** — run a node, then build transactions and send them over JSON-RPC
   (via [`himsha-cli`](./himsha-cli.md) or an [SDK](./sdks.md)). You don't link any Rust crate.
2. **As a library** — depend on the crates directly: build `Instruction`s with the
   program builder functions, or call a program's `process()` in tests/embedding.

## Dependency map

```
himsha-runtime ─ core types (accounts, instructions, tx, utxo, receipts, cpi, errors, program_ids)
   ▲   ▲   ▲
   │   │   └── himsha-programs/*  (system, token, ata, swap, lending, nft-metadata, runes, money-market)
   │   │            ▲
   │   └── himsha-vm ──┘   executes programs (native dispatch + RISC Zero executor)
   │         ▲
   └── himsha-node ─┘  JSON-RPC server, block producer, Bitcoin indexer, redb state
         ▲
      himsha-cli      command-line client (talks to the node over RPC)

himsha-methods (opt-in, --features zkvm)  compiles programs into a RISC Zero guest
himsha-sdk / himsha-sdk-dart / himsha-sdk-python  client libraries that build + send transactions
```

## Modules

### Core crates
- [himsha-runtime](./himsha-runtime.md) — shared types every other crate uses
- [himsha-vm](./himsha-vm.md) — the execution engine (native dispatch + zkVM)
- [himsha-node](./himsha-node.md) — the JSON-RPC node, block producer, Bitcoin indexer
- [himsha-cli](./himsha-cli.md) — command-line client
- [himsha-methods](./himsha-methods.md) — RISC Zero guest (opt-in ZK proving)

### Programs ([details](./programs/))
- [system](./programs/system.md) — accounts, lamport transfer, ownership
- [token](./programs/token.md) — fungible tokens (SPL-style)
- [ata](./programs/ata.md) — deterministic associated token accounts
- [swap](./programs/swap.md) — constant-product AMM
- [lending](./programs/lending.md) — Ordinals-collateral lending
- [nft-metadata](./programs/nft-metadata.md) — on-chain NFT metadata
- [runes](./programs/runes.md) — Bitcoin Runes fungible tokens
- [money-market](./programs/money-market.md) — over-collateralized borrowing

### Client libraries
- [SDKs (TypeScript / Dart / Python)](./sdks.md)

## Conventions shared by all programs

Every program crate exposes:

```rust
// State structs (borsh-encoded into account `data`)
pub struct SomeState { /* … */ }

// An instruction enum (borsh; 1-byte variant tag + fields)
pub enum SomeInstruction { /* … */ }

// Builder helpers that return a himsha_runtime::Instruction
pub fn some_action(/* keys + args */) -> Instruction;

// The entry point the VM calls
pub fn process(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError>;
// (lending, runes, money-market also take a `timestamp: u64`)
```

So the universal recipe to *use* a program as a library is:

```rust
use himsha_runtime::{transaction::{Message, RuntimeTransaction}};
let ix = some_program::some_action(/* … */);          // build instruction
let msg = Message { instructions: vec![ix], timestamp, /* … */ };
let tx  = RuntimeTransaction { message: msg, signatures: vec![/* … */] };
// send tx to a node over himsha_sendTransaction
```

…or to *test* it directly without a node:

```rust
let mut accounts = vec![/* AccountInfo::new(...).as_signer() where required */];
let data = borsh::to_vec(&SomeInstruction::Variant { /* … */ }).unwrap();
some_program::process(&mut accounts, &data).unwrap();
```
