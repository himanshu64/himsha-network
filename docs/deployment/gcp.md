# Deploying HIMSHA Network on Google Cloud (GCP)

> **Educational / proof-of-concept.** See the [root README](../../README.md) disclaimer.
> Reuses build/service steps from the [bare-metal guide](./bare-metal.md).

Two paths: **Compute Engine** (a VM, recommended) and **Cloud Run** (containers).

---

## Architecture

```
Internet → HTTPS Load Balancer (443) → Compute Engine / Cloud Run himsha-node (:9100)
                                              │  HIMSHA_DB on a Persistent Disk
                                              └→ Bitcoin Core (optional, separate VM)
```

---

## Option A — Compute Engine

### 1. Create the VM + data disk

```bash
gcloud compute disks create himsha-data --size=50GB --type=pd-ssd --zone=us-central1-a

gcloud compute instances create himsha-node \
  --zone=us-central1-a \
  --machine-type=e2-standard-2 \          # e2-standard-4+ if also running Bitcoin Core
  --image-family=ubuntu-2204-lts --image-project=ubuntu-os-cloud \
  --disk=name=himsha-data,device-name=himsha-data \
  --service-account=himsha-node@<project>.iam.gserviceaccount.com \
  --scopes=cloud-platform \
  --tags=himsha-node
```

> For ZK proving (`--features zkvm`) use `c2-standard-8`+ (CPU-bound).

### 2. Mount the persistent disk

```bash
sudo mkfs.ext4 -m 0 -F /dev/disk/by-id/google-himsha-data
sudo mkdir -p /var/lib/him
echo '/dev/disk/by-id/google-himsha-data /var/lib/him ext4 defaults,nofail 0 2' | sudo tee -a /etc/fstab
sudo mount -a
```

### 3. Build & run

Follow [bare-metal.md](./bare-metal.md) steps 2–6 (Rust, build `himsha-node`,
systemd unit with `HIMSHA_DB=/var/lib/him/him.redb`).

### 4. Secrets (Bitcoin RPC creds) via Secret Manager

```bash
echo -n 'changeme' | gcloud secrets create himsha-bitcoin-rpc-pass --data-file=-
# At boot (startup script or systemd ExecStartPre):
gcloud secrets versions access latest --secret=himsha-bitcoin-rpc-pass
```

Write them to `/etc/him/bitcoin.env` and reference via `EnvironmentFile=` in the unit.

### 5. Networking & firewall

Keep 9100 private; expose 443 via an HTTPS Load Balancer.

```bash
# Allow LB health checks + your proxy to reach the node port within the VPC only
gcloud compute firewall-rules create allow-himsha-lb \
  --network=default --direction=INGRESS --action=ALLOW \
  --rules=tcp:9100 \
  --source-ranges=130.211.0.0/22,35.191.0.0/16 \  # GCP LB/health-check ranges
  --target-tags=himsha-node
# Do NOT open 9100 to 0.0.0.0/0.
```

Use SSH via IAP (Identity-Aware Proxy) instead of a public SSH rule.

### 6. Verify

```bash
curl -s -X POST http://127.0.0.1:9100 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}'
```

---

## Option B — Cloud Run (containers)

> Cloud Run is request-driven and **stateless** — its filesystem is ephemeral.
> The node's redb `HIMSHA_DB` won't persist across instances, so Cloud Run suits
> demos/ephemeral nodes only. For durable state use Compute Engine, or mount a
> GCS FUSE / Filestore volume (2nd-gen execution environment).

### 1. Container

Use the same Dockerfile as [aws.md](./aws.md#1-dockerfile). The node binds
`127.0.0.1:9100`; Cloud Run requires listening on `0.0.0.0:$PORT`. Change
`bind_addr` in [himsha-node/src/main.rs](../../himsha-node/src/main.rs) to read `PORT`
(default 9100) and bind `0.0.0.0`.

### 2. Build & deploy

```bash
gcloud builds submit --tag gcr.io/<project>/himsha-node
gcloud run deploy himsha-node \
  --image gcr.io/<project>/himsha-node \
  --region us-central1 \
  --cpu 2 --memory 4Gi \
  --no-allow-unauthenticated \             # require IAM auth; front with API Gateway if public
  --set-secrets BITCOIN_RPC_PASS=himsha-bitcoin-rpc-pass:latest \
  --set-env-vars BITCOIN_NETWORK=signet
```

### 3. Persistence (2nd-gen + Filestore)

```bash
gcloud run deploy himsha-node --execution-environment gen2 \
  --add-volume name=himvol,type=nfs,location=<filestore-ip>:/share \
  --add-volume-mount volume=himvol,mount-path=/data \
  --set-env-vars HIMSHA_DB=/data/him.redb
```

---

## Notes

- This PoC node is a single stateful process — there's no clustering. Run one
  instance with a durable disk; scale **up**, not out.
- Use **signet/regtest** Bitcoin in non-prod to avoid the large mainnet disk.
- For a containerized Bitcoin indexer alternative, see
  [bitcoin-indexer-k8s-terraform.md](../bitcoin-indexer-k8s-terraform.md) (GKE-applicable).
