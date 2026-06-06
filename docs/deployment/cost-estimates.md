# Cost & Hardware Estimates — Running a HIMSHA Node

> **Educational / proof-of-concept.** Prices below are *indicative* (USD, early 2026) and
> change constantly — treat them as ballparks and check current provider pricing. See the
> root [disclaimer](../../README.md).

## What actually drives the cost

The `himsha-node` binary itself is **light** — a single Rust process with a `redb`
key-value store and a JSON-RPC server. On its own it runs comfortably on a Raspberry Pi.

Three things, in order, decide your bill:

1. **Which Bitcoin network you anchor to.** This dominates storage. The node can run with
   *no* Bitcoin backend at all (in-memory / regtest), or alongside Bitcoin Core on
   signet / testnet / mainnet. Mainnet's chainstate + `txindex` is the heavy part.
2. **Do you run the indexers?** Electrs (UTXO) and Ord (inscriptions) add disk + CPU.
   Only needed for the Ordinals/Runes features.
3. **Do you generate ZK proofs?** Native dispatch (default) is cheap. Real RISC Zero
   proving (`--features zkvm`) is CPU/GPU-heavy and is the single biggest cost wildcard.

## Resource profile by setup

| Setup | What's running | vCPU | RAM | Disk | Notes |
|---|---|---|---|---|---|
| **0. Node only** (in-memory / regtest) | himsha-node | 2 | 2 GB | 5 GB | dev, CI, demos. No real Bitcoin. |
| **1. Node + signet/testnet** | + Bitcoin Core | 4 | 4–8 GB | 50–150 GB SSD | shared testing, realistic timing |
| **2. Node + mainnet** | + Bitcoin Core (`txindex`) | 4–8 | 8–16 GB | ~800 GB SSD | full-node territory |
| **3. + Ordinals/Runes** | + Electrs + Ord | 8 | 16–32 GB | ~1 TB SSD | indexers double the I/O |
| **4. + ZK proving** | + RISC Zero prover | 8–16 (or GPU) | 32+ GB | as above | proving is the cost driver — see below |

(Setups 1–3 mirror the table in [`bitcoin-indexer-docker.md`](../bitcoin-indexer-docker.md).)

---

## ☁️ Cloud

Roughly what you'd pay **per month**, always-on, on-demand (no reserved/spot discounts).
Storage assumes SSD (AWS `gp3`, GCP `pd-ssd`, Azure Premium SSD).

| Use case | Example instance | Compute | + SSD | ~ Total / mo |
|---|---|---|---|---|
| **Dev / demo** (node only) | AWS `t3.small` (2 vCPU / 2 GB) | ~$15 | 10 GB ≈ $1 | **~$16** |
| **Signet/testnet** | AWS `t3.large` (2 vCPU / 8 GB) | ~$60 | 150 GB ≈ $12 | **~$72** |
| **Mainnet node** | AWS `m6i.large` (2 vCPU / 8 GB) | ~$70 | 1 TB ≈ $80 | **~$150** |
| **Mainnet + indexers** | AWS `m6i.xlarge` (4 vCPU / 16 GB) | ~$140 | 1 TB ≈ $80 | **~$220** |
| **+ ZK proving (GPU)** | AWS `g5.xlarge` (A10G GPU) | ~$730 | 1 TB ≈ $80 | **~$810** |

Notes:
- **GCP / Azure** land within ~±15% of the AWS figures for equivalent specs.
- **Hetzner Cloud** is dramatically cheaper for the CPU-only setups: a `CCX23`
  (4 vCPU / 16 GB) is **~€30/mo**, and 1 TB of volume ~€48/mo → a mainnet+indexers box for
  **~€80/mo** vs ~$220 on AWS. No managed GPU, though.
- **Egress** is the hidden cost on the big clouds (Bitcoin P2P + RPC traffic). Hetzner/OVH
  include generous bandwidth; AWS/GCP/Azure bill per-GB out.
- GPU proving on-demand only when you actually prove (batch jobs) is far cheaper than
  always-on — see the proving section.

See the provider-specific guides: [AWS](./aws.md) · [GCP](./gcp.md) · [Azure](./azure.md).

---

## 🖥️ Bare metal / dedicated server

Best price-per-resource for an always-on full node + indexers. One-time setup, flat
monthly fee, no egress metering.

| Provider | Example box | Specs | ~ Cost / mo |
|---|---|---|---|
| **Hetzner** | `AX42` | 6c/12t Ryzen, 64 GB, 2×512 GB NVMe | **~€46** |
| **Hetzner** | `AX102` | 16c Ryzen, 128 GB, 2×1.9 TB NVMe | **~€100** |
| **OVH / SoYouStart** | entry dedicated | 4c, 32 GB, 1 TB SSD | **~$40–70** |

A single ~€50/mo Hetzner box comfortably runs **node + mainnet Bitcoin Core + Electrs +
Ord** with headroom — the most cost-effective option for a serious always-on node.
See [bare-metal.md](./bare-metal.md).

---

## 🏠 Home server (DIY)

One-time hardware cost + electricity. After ~1 year this beats cloud for an always-on node.

| Build | Example | One-time | Power | Electricity/yr* |
|---|---|---|---|---|
| **Mini PC** | Intel N100 / N305, 16 GB, 1 TB NVMe | **~$300–450** | ~10–20 W | **~$15–35** |
| **NUC / SFF** | i5/i7, 32 GB, 2 TB NVMe | **~$500–800** | ~20–35 W | **~$30–60** |
| **Old desktop + SSD** | repurposed, add 2 TB SSD | **~$120** (SSD only) | ~40–60 W | **~$60–100** |

\* at ~$0.15/kWh, running 24/7. Halve it in low-tariff regions.

A 16 GB mini-PC with a 1 TB NVMe runs **node + mainnet Bitcoin Core** fine. Add Electrs/Ord
and you'll want 32 GB + 2 TB. This is the sweet spot for hobbyists: ~$400 up front, then a
few dollars a month in power.

---

## 🍓 Raspberry Pi

A Pi runs the HIMSHA node and a Bitcoin full node well (this is the same class of hardware
as Umbrel / RaspiBlitz / myNode). The catch is **ZK proving** — don't.

| Component | Item | ~ Cost |
|---|---|---|
| Board | **Raspberry Pi 5, 8 GB** | ~$80 |
| Storage | 1–2 TB NVMe + M.2 HAT | ~$90–140 |
| Power + cooling + case | active cooler, 27 W PSU, case | ~$35 |
| **Total** | | **~$200–250** one-time |

Power draw ~5–10 W → **~$7–15/yr** electricity. Runs continuously for the price of a coffee
a month.

**What works on a Pi 5 (8 GB):**
- ✅ himsha-node (native dispatch) — easily.
- ✅ Bitcoin Core mainnet on the NVMe (full or pruned). Initial sync takes 1–3 days.
- ⚠️ Electrs/Ord — possible but slow to build the index; budget extra time + the 2 TB drive.

**What does *not* work on a Pi:**
- ❌ **RISC Zero proof generation** (`--features zkvm`). It's far too CPU/RAM-hungry for a
  Pi — run the node in native-dispatch mode, or offload proving to a separate machine
  (a cloud GPU job, or a home desktop). Verifying proofs is cheap; *producing* them isn't.

> Use a Pi 5 (8 GB) + **NVMe** (not a microSD — SD cards die under a Bitcoin node's writes).
> A Pi 4 works for node-only / testnet but is tight for mainnet + indexers.

---

## ⚡ The ZK-proving wildcard

Everything above assumes **native dispatch** (the default — programs run deterministically,
receipts are integrity-checked but not cryptographically proven). Turning on real proving
(`--features zkvm`, RISC Zero) changes the math:

- **CPU proving** works but is slow and RAM-heavy (tens of GB), making per-transaction
  proving impractical on small boxes.
- **GPU proving** (NVIDIA, CUDA) is the realistic path. A cloud GPU instance is ~$500–800/mo
  always-on, **or** pennies-to-dollars per proof if you run proving as **on-demand batch
  jobs** and shut the GPU down between them — usually far cheaper.
- **Verification is cheap** — a follower verifying receipts needs no GPU. So a common shape
  is: cheap CPU nodes everywhere + one GPU machine (or on-demand cloud GPU) that produces
  proofs.

Building the guest also needs the RISC Zero toolchain (`cargo-risczero` + `r0vm`), which
wants a 16 GB+ build machine (see the Docker build notes — an 8 GB box struggles).

---

## TL;DR — pick by goal

| Goal | Cheapest sensible option | Ballpark |
|---|---|---|
| Try it / dev / CI | Laptop, or a $5–16/mo cloud VM (regtest) | ~$0–16/mo |
| Always-on testnet/signet node | Mini PC at home, or Hetzner Cloud CCX | ~$0 power / ~€15/mo |
| Always-on mainnet node + indexers | **Hetzner dedicated (~€50/mo)** or a ~$400 home mini-PC | **~€50/mo** or one-time |
| Privacy-max home node | **Raspberry Pi 5 + NVMe** | **~$220 once + ~$1/mo power** |
| Generating ZK proofs | On-demand cloud GPU (batch), not always-on | per-proof, $-cents to $ |

**Rule of thumb:** if it's always-on, **bare metal or a home box beats cloud within a year**;
the only reason to be in the cloud long-term is on-demand GPU proving or managed ops.
