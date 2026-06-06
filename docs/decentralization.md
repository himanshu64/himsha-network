# Decentralization — ZK-native (Option B)

HIMSHA decentralizes **verification and custody**, not **re-execution**. Because every
state transition is (or can be) backed by a ZK receipt, peers don't need to re-vote on
execution the way a validator-consensus chain does — they only need to *verify* and
replicate. This contrasts with validator-consensus chains that re-vote on execution.

Two pieces:

## 1. ZK-verifying follower nodes — ✅ built (read decentralization)

A **follower** ([`himsha-node/src/follower.rs`](../himsha-node/src/follower.rs)) replicates
a primary's state **without trusting its account values**:

1. Polls the primary over JSON-RPC (`himsha_getSlot`, `himsha_getBlock`).
2. For each new block, **independently re-derives** every state transition by
   re-executing the block's transactions through the same executor. In native mode the
   re-execution *is* the verification; under `--features zkvm` the executor verifies the
   RISC Zero receipt instead.
3. Persists only what it recomputed, and serves trust-minimized reads on its own RPC.

It does **not** produce blocks and does **not** broadcast Bitcoin settlements.

**Run a follower:**
```bash
# primary
HIMSHA_DB=./primary.redb HIMSHA_BIND=127.0.0.1:9100 cargo run -p himsha-node
# follower (replica) on another port, replicating from the primary
HIMSHA_DB=./follower.redb HIMSHA_BIND=127.0.0.1:9101 \
  HIMSHA_FOLLOW=http://127.0.0.1:9100 HIMSHA_FOLLOW_INTERVAL_SECS=1 \
  cargo run -p himsha-node
```

New env vars: `HIMSHA_BIND` (default `127.0.0.1:9100`), `HIMSHA_FOLLOW` (primary URL →
enables follower mode), `HIMSHA_FOLLOW_INTERVAL_SECS` (poll cadence).

Verified: a unit test re-executes a transfer and replicates balances + slot; a live
primary+follower run shows the follower replicating and serving reads on a second port.

## 2. FROST threshold signer for the Bitcoin key — ✅ built (custody decentralization)

The settlement key is split **M-of-N** across independent signers via a FROST
threshold-Schnorr committee — no single party can move funds.
[`himsha-threshold`](../himsha-threshold/src/lib.rs) wraps the audited
`frost-secp256k1` crate:

```rust
use himsha_threshold::Committee;

let committee = Committee::generate(2, 3)?;        // 2-of-3 settlement key
let group_key = committee.group_public_key();      // the on-chain settlement key
let ids = committee.signer_ids();
let sig = committee.sign(&sighash, &ids[..2])?;     // any quorum of 2 signs
assert!(committee.verify(&sighash, &sig));          // one aggregate Schnorr signature
```

Flow: keygen → round 1 nonce commitments → round 2 signature shares → aggregate into a
single Schnorr signature → verify under the group key. A quorum below threshold is
refused.

**Keygen has two modes:**
- `Committee::generate(t, n)` — trusted-dealer split (simple bootstrap).
- `Committee::generate_dkg(t, n)` — **distributed key generation**: no party ever holds
  the full key, even at setup (FROST 3-round DKG: `part1`/`part2`/`part3`).

(tests: quorum sign/verify, full quorum, below-threshold rejection, tampered-message
rejection, stable group key, **DKG** sign/verify with no dealer, and **robust signing** —
see below.)

**ROAST-style robust signing — built.** Plain FROST aborts the whole round if any chosen
signer sends an invalid or equivocating share — so one disruptive signer can stall
settlement. `Committee::sign_robust` (and the Taproot variant) close this with the
**ROAST** robust-signing protocol: the coordinator attempts a quorum, and if the aggregator identifies a
signer whose share fails verification (`frost::Error::culprit`), that signer is **excluded
and the round retried** with the remaining honest signers — repeating until a valid
signature is produced or fewer than `threshold` honest signers remain.

```rust
let committee = Committee::generate(3, 5)?;          // 3-of-5
let ids = committee.signer_ids();
// Even with 2 disruptive signers, settlement still produces a valid signature:
let robust = committee.sign_robust(&sighash, &ids, &ids[..2])?;
assert_eq!(robust.excluded, 2);                       // both culprits dropped
assert!(committee.verify(&sighash, &robust.signature));
```

In production the `disruptive` set is empty — real faults (bad/missing shares) are
discovered through the *same* culprit-identification path. This gives the liveness
guarantee ROAST is designed for: **a valid threshold signature as long as `threshold`
honest signers are online.** (Tests: tolerate-disruptive, clean single-round path, and
exhaustion when too few honest signers remain — for both the secp256k1 and Taproot committees.)

**Taproot key-spend — built:**
- `TaprootCommittee` (`himsha-threshold::taproot`, over `frost-secp256k1-tr`) produces a
  **Taproot-valid** aggregate signature; `group_xonly()` is the 32-byte output key.
- [`himsha-node::settlement_tx`](../himsha-node/src/settlement_tx.rs) builds the unsigned
  key-spend tx, computes the **BIP-341 sighash**, has the committee threshold-sign it, and
  attaches the 64-byte Schnorr **witness** → signed raw tx hex for `bitcoin_indexer::broadcast`.
  `settle_with_committee(...)` ties it together. (Node tests cover sighash size, witness
  attachment, bad-sig rejection; threshold crate tests cover Taproot sign/verify.)

> ⚠️ **Unverified without regtest.** Compilation + the build→sighash→sign→witness path are
> tested, but real on-chain acceptance (exact tweak, fee policy, a funded committee UTXO,
> broadcast) must be confirmed against Bitcoin Core. Wiring `settle_with_committee` into the
> lending settlement (replacing single-wallet signing) is the final regtest step.

This is the only place HIMSHA needs a small multi-party protocol — scoped to *custody*,
not execution, keeping the "proofs, not votes" thesis intact.

## 3. Sequencer failover — ✅ built (liveness)

Because a follower already holds fully-replicated state, promoting it to sequencer is
clean. Set `HIMSHA_FAILOVER_MISSES=N`: if the primary is unreachable for N consecutive
polls, the follower self-promotes and starts producing blocks from its replicated tip
(`follower::should_promote` + `run_until_promote`). Verified live: kill the primary →
follower logs *"promoting to sequencer"* and keeps serving RPC.

**Multi-standby, two modes:**

1. *Crash-safe* (`HIMSHA_STANDBY_PEERS=<urls>`) — promote if no higher-priority peer is
   alive (`follower::wins_election`). Simple; safe only for crash faults / single standby.

2. *Partition-safe quorum election* (`HIMSHA_ELECTION_MEMBERS=<urls>` + `HIMSHA_SELF=<url>`)
   — the real fix. Raft-style: monotonic **terms**, **one vote per term** per node
   (`himsha_requestVote` RPC), and a candidate promotes only after a **majority** of the
   member set votes for it (`election::has_quorum`). By quorum intersection there is **at
   most one leader per term**, and a **minority partition can never reach majority → can
   never elect** → no split-brain. A small per-node jitter staggers candidacy for liveness.

Verified: unit tests for the vote rule (one-per-term, newer-term reset, stale-term reject)
and quorum math; live 3-member test where a lone standby (minority) logs
`1/3 votes — no quorum` and **refuses to promote** — exactly what the crash-safe scheme
got wrong.

**Liveness — built.** A leader advertises itself via the `himsha_getLeader` RPC (the
heartbeat). On primary loss a node first **discovers the live leader** among members and
**re-points** its follow target to it (`follower::discover_leader` → swap `primary_url`),
resuming replication *without* an election. It only contests an election when no leader is
reachable; on winning it calls `become_leader`, and any node granting a higher-term vote
**steps down** (`observe_term`). Verified live: a follower pointed at a dead URL discovers
the leader and logs `re-pointing follow target to leader …`.

**PreVote — built (term-inflation fix).** The last classic Raft-liveness hazard is a
partitioned node that keeps timing out and **bumping its term**; when the partition heals,
its inflated term forces the healthy leader to step down — needless churn. HIMSHA adds the
Raft **PreVote** phase (§9.6): before a real election a candidate runs a *non-binding* poll
at `term+1` (`himsha_preVote` RPC → `election::consider_pre_vote`) that **does not mutate
term or vote**. It proceeds to bump its term only if a **quorum would vote for it**, and a
live leader refuses to endorse a challenger. So a node that can't reach a majority never
inflates its term and never disrupts the leader. (Tests: pre-vote is non-binding, a live
leader refuses, stale terms rejected.)

> What we still don't do (by design): full Raft **log reconciliation** — HIMSHA replicates
> state by ZK-verified block re-derivation (followers) instead of a replicated op-log.

## What we deliberately do NOT add
- **Execution consensus / re-voting** — replaced by ZK receipts.
- **A heavyweight validator set** — followers verify; they don't arbitrate ordering.
- Ordering/finality still anchors to **Bitcoin L1**; a sequencer-rotation scheme is only
  needed if a single sequencer becomes a liveness bottleneck.
