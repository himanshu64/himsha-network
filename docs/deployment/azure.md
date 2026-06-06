# Deploying HIMSHA Network on Microsoft Azure

> **Educational / proof-of-concept.** See the [root README](../../README.md) disclaimer.
> Reuses build/service steps from the [bare-metal guide](./bare-metal.md).

Two paths: **Azure VM** (recommended) and **Container Apps** (containers).

---

## Architecture

```
Internet → Application Gateway (HTTPS, 443) → Azure VM / Container App himsha-node (:9100)
                                                    │  HIMSHA_DB on a Managed Disk
                                                    └→ Bitcoin Core (optional, separate VM)
```

---

## Option A — Azure VM

### 1. Resource group + VM + data disk

```bash
az group create -n himsha-rg -l eastus

az vm create -g himsha-rg -n himsha-node \
  --image Ubuntu2204 \
  --size Standard_D2s_v5 \                 # D4s_v5+ if also running Bitcoin Core
  --admin-username azureuser \
  --generate-ssh-keys \
  --assign-identity                         # managed identity for Key Vault access

az vm disk attach -g himsha-rg --vm-name himsha-node \
  --name himsha-data --new --size-gb 50 --sku Premium_LRS
```

> For ZK proving (`--features zkvm`) choose a compute-optimized size, e.g. `Standard_F8s_v2`.

### 2. Mount the data disk

```bash
sudo parted /dev/sdc --script mklabel gpt mkpart primary ext4 0% 100%
sudo mkfs.ext4 /dev/sdc1
sudo mkdir -p /var/lib/him
echo '/dev/sdc1 /var/lib/him ext4 defaults,nofail 0 2' | sudo tee -a /etc/fstab
sudo mount -a
```

### 3. Build & run

Follow [bare-metal.md](./bare-metal.md) steps 2–6 (Rust, build `himsha-node`,
systemd unit with `HIMSHA_DB=/var/lib/him/him.redb`).

### 4. Secrets (Bitcoin RPC creds) via Key Vault

```bash
az keyvault create -g himsha-rg -n himsha-kv
az keyvault secret set --vault-name himsha-kv -n bitcoin-rpc-pass --value 'changeme'

# Grant the VM's managed identity access, then fetch at boot:
az keyvault secret show --vault-name himsha-kv -n bitcoin-rpc-pass --query value -o tsv
```

Write the creds to `/etc/him/bitcoin.env` and reference with `EnvironmentFile=`.

### 5. Networking (NSG + Application Gateway)

Keep 9100 private; terminate TLS at the Application Gateway.

```bash
# Allow only the App Gateway subnet to reach 9100; no public 9100/22.
az network nsg rule create -g himsha-rg --nsg-name himsha-nodeNSG -n allow-appgw \
  --priority 100 --direction Inbound --access Allow --protocol Tcp \
  --destination-port-ranges 9100 --source-address-prefixes <appgw-subnet-cidr>
```

Use **Azure Bastion** for admin SSH instead of a public 22 rule.

### 6. Verify

```bash
curl -s -X POST http://127.0.0.1:9100 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}'
```

---

## Option B — Azure Container Apps

### 1. Container

Use the same Dockerfile as [aws.md](./aws.md#1-dockerfile). The node binds
`127.0.0.1:9100`; Container Apps ingress requires `0.0.0.0:<targetPort>`, so
change `bind_addr` in [himsha-node/src/main.rs](../../himsha-node/src/main.rs) to bind
`0.0.0.0:9100`.

### 2. Build & push to ACR

```bash
az acr create -g himsha-rg -n himacr --sku Basic
az acr build -r himacr -t himsha-node:latest .
```

### 3. Deploy with a persistent volume

Container Apps filesystems are ephemeral — mount **Azure Files** for `HIMSHA_DB`.

```bash
az containerapp env create -g himsha-rg -n himsha-env -l eastus
# Register an Azure Files share as a storage in the environment, then:
az containerapp create -g himsha-rg -n himsha-node \
  --environment himsha-env \
  --image himacr.azurecr.io/himsha-node:latest \
  --target-port 9100 --ingress external \
  --min-replicas 1 --max-replicas 1 \       # single stateful instance
  --cpu 2 --memory 4Gi \
  --secrets bitcoin-pass=keyvaultref:... \
  --env-vars HIMSHA_DB=/data/him.redb BITCOIN_NETWORK=signet BITCOIN_RPC_PASS=secretref:bitcoin-pass
# Attach the Azure Files volume mount at /data via `az containerapp update --yaml`.
```

> Keep `min/max-replicas = 1`: the node is a single stateful process over one redb
> file and cannot run multiple concurrent writers.

---

## Notes

- Scale **up** (bigger VM), not out — there's no multi-node clustering in this PoC.
- Use **signet/regtest** Bitcoin in non-prod to avoid the large mainnet disk.
- Put the Application Gateway behind Azure WAF and add request auth before
  exposing the RPC publicly.
