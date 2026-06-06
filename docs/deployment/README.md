# HIMSHA Network — Deployment Guides

Instructions for running a `himsha-node` in different environments. All targets run
the same binary (`cargo build --release -p himsha-node`, listening on
`127.0.0.1:9100`); the cloud guides build on the bare-metal steps.

| Target | Guide | Best for |
|--------|-------|----------|
| Bare metal / self-managed Linux | [bare-metal.md](./bare-metal.md) | Full control, dev boxes, on-prem |
| AWS (EC2 / ECS Fargate) | [aws.md](./aws.md) | AWS-native infra |
| Google Cloud (Compute Engine / Cloud Run) | [gcp.md](./gcp.md) | GCP-native infra |
| Azure (VM / Container Apps) | [azure.md](./azure.md) | Azure-native infra |
| 💰 Cost & hardware sizing | [cost-estimates.md](./cost-estimates.md) | Budgeting cloud vs bare metal vs home vs Raspberry Pi |

## Common facts across all targets

- **Binary**: `himsha-node` (add `--features zkvm` for ZK-proven execution; needs the
  RISC Zero toolchain).
- **Port**: binds `127.0.0.1:9100` — always front it with a TLS reverse proxy / load
  balancer and never expose 9100 directly.
- **State**: a single redb file at `$HIMSHA_DB` — back it up; the node is a single
  stateful process (scale **up**, not out).
- **Bitcoin (optional)**: set `BITCOIN_RPC_URL`, `BITCOIN_RPC_USER`,
  `BITCOIN_RPC_PASS`, `BITCOIN_NETWORK` to enable `himsha_getUtxo` and Ordinals loan
  settlement broadcasting. Without them the node still runs (settlements are logged).
- **Containers**: the node binds localhost by default; for container/LB routing,
  change `bind_addr` in [himsha-node/src/main.rs](../../himsha-node/src/main.rs) to
  `0.0.0.0:9100` and keep it private behind the proxy.

See also: [bitcoin-indexer-docker.md](../bitcoin-indexer-docker.md) and
[bitcoin-indexer-k8s-terraform.md](../bitcoin-indexer-k8s-terraform.md) for the
Bitcoin Core + `ord` side, and [zkvm-proving.md](../zkvm-proving.md) for the
opt-in ZK path.
