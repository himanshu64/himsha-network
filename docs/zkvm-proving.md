# ZK Proving (RISC Zero guest) — opt-in

By default the HIMSHA node runs built-in programs via **native dispatch**
([`himsha-vm::dispatch`](../himsha-vm/src/dispatch.rs)): execution is deterministic and
produces the correct state transition, but **no ZK proof is generated** (receipts
are marked `verified: false`). This lets the node run end-to-end without the
RISC Zero toolchain.

This document covers the **opt-in `zkvm` feature**, which proves *every* program —
built-ins included — through a single universal RISC Zero guest.

> ⚠️ **Experimental & unverified in CI.** The guest crates require the RISC Zero
> toolchain, which is not part of the default build or test pipeline. Treat the
> `zkvm` path as a reference implementation that may need adjustment for your
> exact `risc0` version.

---

## Architecture

```
host (himsha-vm::executor)                 guest (himsha-methods/guest)
  write(program_id)            ─────►      let program_id = env::read()
  write(ExecutionInput)        ─────►      let input      = env::read()
                                           dispatch(program_id, accounts, …)
  receipt.journal.decode()     ◄─────      env::commit(ExecutionOutput)
  receipt.verify(image_id)
```

- **One universal guest.** [`himsha-methods/guest`](../himsha-methods/guest) reads the
  `program_id`, then the `ExecutionInput`, dispatches to the matching program's
  `process()`, and commits the `ExecutionOutput`. Because all built-ins share one
  guest, they share **one image id** — registered for every built-in by
  [`himsha-vm::zk::register_builtins`](../himsha-vm/src/zk.rs).
- **Shared I/O types.** `ExecutionInput` / `ExecutionOutput` live in
  [`himsha-runtime::exec`](../himsha-runtime/src/exec.rs) so the guest needs no
  dependency on the prover/host crate.
- **Detached build.** [`himsha-methods`](../himsha-methods) and its guest declare their
  own `[workspace]` and are listed under `exclude` in the root `Cargo.toml`, so
  the default `cargo build --workspace` never compiles them.

## Prerequisites

```bash
cargo install cargo-risczero
cargo risczero install        # installs the r0vm + RISC-V toolchain
```

## Build & run with proving

```bash
# Build the node with the universal guest (compiles himsha-methods → guest ELF):
cargo build -p himsha-node --features zkvm

# Run it — built-ins are now ZK-proven instead of natively dispatched:
HIMSHA_DB=./him.redb cargo run -p himsha-node --features zkvm
```

`RISC0_DEV_MODE=1` runs the prover in fast (non-cryptographic) dev mode, useful
for iterating without paying full proving time.

## How dispatch stays in sync

The guest's dispatch table in
[`himsha-methods/guest/src/main.rs`](../himsha-methods/guest/src/main.rs) mirrors
[`himsha-vm::dispatch`](../himsha-vm/src/dispatch.rs). The guest can't depend on
`himsha-vm` (which links the RISC Zero host), so the match is intentionally
duplicated — **keep both in sync** when adding a program.

## Known caveats

- **Image-id encoding.** `risc0-build` emits the image id as `[u32; 8]`;
  [`himsha-vm::zk::guest_image_id`](../himsha-vm/src/zk.rs) serializes it little-endian
  to the `[u8; 32]` the registry and `Receipt::verify` use here. Confirm this
  matches your `risc0` version's `Digest` conventions.
- **`std` in the guest.** The guest enables `risc0-zkvm`'s experimental `std`
  feature because some programs use `std` types (e.g. `HashMap` in lending).
- **Per-program image ids.** A future refinement is one guest (and image id)
  per program instead of a shared universal guest, so `program_id` derives from
  its own image id as the deploy path assumes.
