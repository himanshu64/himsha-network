# Deploying HIMSHA Network on AWS

> **Educational / proof-of-concept.** See the [root README](../../README.md) disclaimer.
> This guide assumes familiarity with the [bare-metal guide](./bare-metal.md), which
> the EC2 path reuses for build/service steps.

Two paths: **EC2** (a VM you manage, recommended) and **ECS Fargate** (containers).

---

## Architecture

```
Internet → ALB (HTTPS, 443) → EC2/Fargate himsha-node (127.0.0.1:9100)
                                   │  (HIMSHA_DB on EBS / EFS)
                                   └→ Bitcoin Core (optional, separate instance)
```

---

## Option A — EC2

### 1. Provision

- **Instance**: `t3.large` (2 vCPU / 8 GB) for the node alone; bump to `m6i.xlarge`
  if also running Bitcoin Core. ZK proving (`--features zkvm`) is CPU/RAM heavy —
  use `c6i.2xlarge`+.
- **AMI**: Ubuntu 22.04 LTS.
- **Storage**: 30 GB gp3 root; a separate gp3 EBS volume for `HIMSHA_DB`
  (and ~600 GB+ if hosting Bitcoin Core mainnet — use signet/regtest for dev).
- **IAM role**: attach a role allowing SSM (for keyless access) and Secrets Manager read.

```bash
aws ec2 run-instances \
  --image-id ami-xxxxxxxx \
  --instance-type t3.large \
  --key-name my-key \
  --iam-instance-profile Name=himsha-node-role \
  --block-device-mappings '[{"DeviceName":"/dev/sdb","Ebs":{"VolumeSize":50,"VolumeType":"gp3"}}]' \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=himsha-node}]'
```

### 2. Mount the data volume

```bash
sudo mkfs.ext4 /dev/nvme1n1
sudo mkdir -p /var/lib/him
echo '/dev/nvme1n1 /var/lib/him ext4 defaults,nofail 0 2' | sudo tee -a /etc/fstab
sudo mount -a
```

### 3. Build & run

Follow [bare-metal.md](./bare-metal.md) steps 2–6 (install Rust, build
`himsha-node`, install the systemd unit with `HIMSHA_DB=/var/lib/him/him.redb`).

### 4. Secrets (Bitcoin RPC creds)

Store creds in **AWS Secrets Manager** and inject at boot:

```bash
aws secretsmanager get-secret-value --secret-id him/bitcoin-rpc \
  --query SecretString --output text > /etc/him/bitcoin.env
# file: BITCOIN_RPC_URL=...  BITCOIN_RPC_USER=...  BITCOIN_RPC_PASS=...
```

Reference it from the unit: `EnvironmentFile=/etc/him/bitcoin.env`.

### 5. Networking

- **Security group**: inbound 443 from the ALB SG only; **no** inbound 9100.
  SSH via SSM Session Manager (no inbound 22).
- **ALB**: HTTPS listener (ACM cert) → target group → instance `:9100`.
  Health check: `POST /` won't work as a simple GET healthcheck, so add a tiny
  sidecar or use a TCP health check on 9100.

```bash
aws elbv2 create-target-group --name himsha-tg --protocol HTTP --port 9100 \
  --vpc-id vpc-xxxx --health-check-protocol TCP
```

### 6. Verify

```bash
curl -s https://rpc.example.com \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}'
```

---

## Option B — ECS Fargate (containers)

### 1. Dockerfile

```dockerfile
FROM rust:1.79-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p himsha-node

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/himsha-node /usr/local/bin/himsha-node
ENV HIMSHA_DB=/data/him.redb RUST_LOG=himsha_node=info
VOLUME /data
EXPOSE 9100
ENTRYPOINT ["himsha-node"]
```

> The binary binds `127.0.0.1:9100`. For container/ALB routing you must make it
> bind `0.0.0.0` — change `bind_addr` in [himsha-node/src/main.rs](../../himsha-node/src/main.rs)
> to `0.0.0.0:9100` (then keep it private to the VPC + behind the ALB).

### 2. Push to ECR

```bash
aws ecr create-repository --repository-name himsha-node
docker build -t himsha-node .
docker tag himsha-node:latest <acct>.dkr.ecr.<region>.amazonaws.com/himsha-node:latest
aws ecr get-login-password | docker login --username AWS --password-stdin <acct>.dkr.ecr.<region>.amazonaws.com
docker push <acct>.dkr.ecr.<region>.amazonaws.com/himsha-node:latest
```

### 3. Task definition essentials

- **Volume**: EFS access point mounted at `/data` for `HIMSHA_DB` persistence.
- **Secrets**: map Secrets Manager keys → `BITCOIN_RPC_URL/USER/PASS` env.
- **CPU/Mem**: 2 vCPU / 4 GB (much more for `zkvm`).
- **Service**: behind an internal ALB; expose via HTTPS ALB + WAF.

### 4. Logs & monitoring

- `awslogs` driver → CloudWatch Logs (filter on `himsha_node`).
- CloudWatch alarm on task restarts and ALB 5xx.

---

## Cost-saving notes

- Use **signet/regtest** for Bitcoin in non-prod to avoid the ~600 GB mainnet volume.
- Single small instance is fine for this PoC — there is no horizontal scaling story
  (the node is a single stateful process over a redb file).
