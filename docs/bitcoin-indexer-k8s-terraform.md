# Bitcoin Indexer + Ord — Kubernetes & Terraform Setup

> ⚠️ **Disclaimer**: Educational and development use only. Review all credentials,
> firewall rules, and IAM policies carefully before any production deployment.

---

## Architecture

```
Internet
    │
  AWS Route53 (DNS)
    │
  Application Load Balancer (ALB)
    │  TLS termination via ACM certificate
  Ingress-NGINX
    ├── api.him.yourdomain.com  → himsha-node:9100
    └── ord.him.yourdomain.com  → ord:8080
    │
  EKS Cluster (himsha-network)
    Namespace: himsha-infra
    ├── StatefulSet:  bitcoin-core   (1 replica, EBS volume)
    ├── Deployment:   electrs         (1 replica)
    ├── Deployment:   ord             (1 replica)
    └── Deployment:   himsha-node        (1 replica)
    │
  AWS EBS (gp3, encrypted)
    ├── bitcoin-pvc   100 GB testnet / 700 GB mainnet
    ├── electrs-pvc   100 GB
    ├── ord-pvc       100 GB
    └── himsha-pvc       20 GB
    │
  AWS Secrets Manager
    └── him/bitcoin-rpc-creds  (username, password)
```

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Terraform | 1.6+ | `brew install terraform` |
| kubectl | 1.28+ | `brew install kubectl` |
| Helm | 3.12+ | `brew install helm` |
| AWS CLI | 2.x | `brew install awscli` |
| eksctl | 0.170+ | `brew tap weaveworks/tap && brew install eksctl` |

```bash
# Verify versions
terraform version
kubectl version --client
helm version
aws --version
eksctl version

# Configure AWS credentials
aws configure
# AWS Access Key ID: <your key>
# AWS Secret Access Key: <your secret>
# Default region: us-east-1
# Default output format: json
```

---

## Repository Layout

```
infrastructure/
├── terraform/
│   ├── main.tf              # Root module — wires everything together
│   ├── variables.tf         # Input variables
│   ├── outputs.tf           # Stack outputs
│   ├── versions.tf          # Provider version locks
│   ├── terraform.tfvars     # Values file (not committed)
│   └── modules/
│       ├── vpc/             # VPC, subnets, NAT
│       ├── eks/             # EKS cluster + node groups
│       ├── storage/         # EBS CSI driver, storage classes
│       └── secrets/         # Secrets Manager entries
└── kubernetes/
    ├── namespaces.yaml
    ├── secrets/
    │   └── bitcoin-rpc-secret.yaml
    ├── bitcoin/
    │   ├── statefulset.yaml
    │   ├── service.yaml
    │   └── configmap.yaml
    ├── electrs/
    │   ├── deployment.yaml
    │   ├── service.yaml
    │   └── configmap.yaml
    ├── ord/
    │   ├── deployment.yaml
    │   └── service.yaml
    ├── himsha-node/
    │   ├── deployment.yaml
    │   ├── service.yaml
    │   └── pvc.yaml
    ├── ingress/
    │   ├── ingress.yaml
    │   └── certificate.yaml
    └── monitoring/
        ├── servicemonitor-electrs.yaml
        └── servicemonitor-himsha-node.yaml
```

---

## Part 1 — Terraform

### `terraform/versions.tf`

```hcl
terraform {
  required_version = ">= 1.6.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.30"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.25"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.12"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }

  # Remote state in S3 + DynamoDB locking
  backend "s3" {
    bucket         = "himsha-terraform-state-<your-account-id>"
    key            = "himsha-network/terraform.tfstate"
    region         = "us-east-1"
    encrypt        = true
    dynamodb_table = "himsha-terraform-locks"
  }
}
```

### `terraform/variables.tf`

```hcl
variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "cluster_name" {
  description = "EKS cluster name"
  type        = string
  default     = "himsha-network"
}

variable "network" {
  description = "Bitcoin network: regtest, testnet, or mainnet"
  type        = string
  default     = "testnet"

  validation {
    condition     = contains(["regtest", "testnet", "mainnet"], var.network)
    error_message = "Network must be regtest, testnet, or mainnet."
  }
}

variable "node_instance_type" {
  description = "EC2 instance type for EKS worker nodes"
  type        = string
  default     = "t3.xlarge"    # 4 vCPU, 16 GB — use m5.2xlarge for mainnet
}

variable "min_nodes" {
  type    = number
  default = 2
}

variable "max_nodes" {
  type    = number
  default = 5
}

variable "bitcoin_disk_gb" {
  description = "EBS volume size for Bitcoin data"
  type        = number
  default     = 100    # Use 700 for mainnet
}

variable "electrs_disk_gb" {
  type    = number
  default = 100
}

variable "ord_disk_gb" {
  type    = number
  default = 100
}

variable "domain_name" {
  description = "Base domain for ingress (e.g. him.yourdomain.com)"
  type        = string
  default     = ""
}

variable "acm_certificate_arn" {
  description = "ACM certificate ARN for TLS"
  type        = string
  default     = ""
}

variable "bitcoin_rpc_user" {
  description = "Bitcoin RPC username"
  type        = string
  sensitive   = true
}

variable "bitcoin_rpc_pass" {
  description = "Bitcoin RPC password (min 20 chars)"
  type        = string
  sensitive   = true

  validation {
    condition     = length(var.bitcoin_rpc_pass) >= 20
    error_message = "Bitcoin RPC password must be at least 20 characters."
  }
}
```

### `terraform/main.tf`

```hcl
provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project     = "himsha-network"
      Network     = var.network
      ManagedBy   = "terraform"
    }
  }
}

# ---- Data sources ----
data "aws_availability_zones" "available" {
  state = "available"
}

data "aws_caller_identity" "current" {}

# ---- VPC ----
module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "5.4"

  name = "${var.cluster_name}-vpc"
  cidr = "10.0.0.0/16"

  azs             = slice(data.aws_availability_zones.available.names, 0, 3)
  private_subnets = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
  public_subnets  = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]

  enable_nat_gateway     = true
  single_nat_gateway     = false    # HA: one NAT per AZ for production
  enable_dns_hostnames   = true
  enable_dns_support     = true

  # Required tags for EKS to discover subnets
  private_subnet_tags = {
    "kubernetes.io/role/internal-elb"             = "1"
    "kubernetes.io/cluster/${var.cluster_name}"   = "owned"
  }
  public_subnet_tags = {
    "kubernetes.io/role/elb"                      = "1"
    "kubernetes.io/cluster/${var.cluster_name}"   = "owned"
  }
}

# ---- EKS Cluster ----
module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "20.4"

  cluster_name    = var.cluster_name
  cluster_version = "1.29"

  vpc_id     = module.vpc.vpc_id
  subnet_ids = module.vpc.private_subnets

  # Public endpoint for kubectl (restrict in production)
  cluster_endpoint_public_access       = true
  cluster_endpoint_public_access_cidrs = ["0.0.0.0/0"]   # Restrict to your IP

  # EKS Managed Add-ons
  cluster_addons = {
    coredns = {
      most_recent = true
    }
    kube-proxy = {
      most_recent = true
    }
    vpc-cni = {
      most_recent = true
    }
    aws-ebs-csi-driver = {
      most_recent              = true
      service_account_role_arn = module.ebs_csi_irsa.iam_role_arn
    }
  }

  # Managed node group
  eks_managed_node_groups = {
    bitcoin-nodes = {
      name           = "bitcoin-nodes"
      instance_types = [var.node_instance_type]
      ami_type       = "AL2_x86_64"

      min_size     = var.min_nodes
      max_size     = var.max_nodes
      desired_size = var.min_nodes

      # Node labels for pod scheduling
      labels = {
        role = "bitcoin-infra"
      }

      # Taint so only bitcoin workloads run here (optional)
      taints = []

      block_device_mappings = {
        xvda = {
          device_name = "/dev/xvda"
          ebs = {
            volume_size           = 100   # Node OS disk
            volume_type           = "gp3"
            iops                  = 3000
            throughput            = 125
            encrypted             = true
            delete_on_termination = true
          }
        }
      }

      iam_role_additional_policies = {
        ebs_csi = "arn:aws:iam::aws:policy/service-role/AmazonEBSCSIDriverPolicy"
      }
    }
  }
}

# ---- IAM Role for EBS CSI Driver ----
module "ebs_csi_irsa" {
  source  = "terraform-aws-modules/iam/aws//modules/iam-role-for-service-accounts-eks"
  version = "5.30"

  role_name             = "${var.cluster_name}-ebs-csi"
  attach_ebs_csi_policy = true

  oidc_providers = {
    main = {
      provider_arn               = module.eks.oidc_provider_arn
      namespace_service_accounts = ["kube-system:ebs-csi-controller-sa"]
    }
  }
}

# ---- Secrets Manager ----
resource "aws_secretsmanager_secret" "bitcoin_rpc" {
  name                    = "him/${var.network}/bitcoin-rpc-creds"
  recovery_window_in_days = 7
  description             = "Bitcoin Core RPC credentials for HIMSHA Network"
}

resource "aws_secretsmanager_secret_version" "bitcoin_rpc" {
  secret_id = aws_secretsmanager_secret.bitcoin_rpc.id
  secret_string = jsonencode({
    username = var.bitcoin_rpc_user
    password = var.bitcoin_rpc_pass
  })
}

# ---- Kubernetes namespace + StorageClass ----
provider "kubernetes" {
  host                   = module.eks.cluster_endpoint
  cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)

  exec {
    api_version = "client.authentication.k8s.io/v1beta1"
    command     = "aws"
    args        = ["eks", "get-token", "--cluster-name", var.cluster_name]
  }
}

resource "kubernetes_namespace" "himsha_infra" {
  metadata {
    name = "himsha-infra"
    labels = {
      "app.kubernetes.io/managed-by" = "terraform"
    }
  }
}

resource "kubernetes_storage_class" "gp3" {
  metadata {
    name = "gp3-encrypted"
    annotations = {
      "storageclass.kubernetes.io/is-default-class" = "true"
    }
  }
  storage_provisioner    = "ebs.csi.aws.com"
  reclaim_policy         = "Retain"        # Retain data even if PVC is deleted
  volume_binding_mode    = "WaitForFirstConsumer"
  allow_volume_expansion = true

  parameters = {
    type      = "gp3"
    iops      = "3000"
    throughput = "125"
    encrypted = "true"
  }
}

# ---- NGINX Ingress Controller (via Helm) ----
provider "helm" {
  kubernetes {
    host                   = module.eks.cluster_endpoint
    cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)

    exec {
      api_version = "client.authentication.k8s.io/v1beta1"
      command     = "aws"
      args        = ["eks", "get-token", "--cluster-name", var.cluster_name]
    }
  }
}

resource "helm_release" "nginx_ingress" {
  name             = "ingress-nginx"
  repository       = "https://kubernetes.github.io/ingress-nginx"
  chart            = "ingress-nginx"
  version          = "4.9.0"
  namespace        = "ingress-nginx"
  create_namespace = true

  values = [<<-YAML
    controller:
      service:
        type: LoadBalancer
        annotations:
          service.beta.kubernetes.io/aws-load-balancer-type: "nlb"
          service.beta.kubernetes.io/aws-load-balancer-scheme: "internet-facing"
          service.beta.kubernetes.io/aws-load-balancer-ssl-cert: "${var.acm_certificate_arn}"
      metrics:
        enabled: true
  YAML
  ]
}
```

### `terraform/outputs.tf`

```hcl
output "cluster_endpoint" {
  value = module.eks.cluster_endpoint
}

output "cluster_name" {
  value = module.eks.cluster_name
}

output "configure_kubectl" {
  description = "Run this command to configure kubectl"
  value       = "aws eks update-kubeconfig --region ${var.aws_region} --name ${var.cluster_name}"
}

output "bitcoin_secret_arn" {
  value = aws_secretsmanager_secret.bitcoin_rpc.arn
}

output "load_balancer_hostname" {
  value       = helm_release.nginx_ingress.status
  description = "After deploy, get LB hostname: kubectl -n ingress-nginx get svc"
}
```

### `terraform/terraform.tfvars` (not committed)

```hcl
aws_region          = "us-east-1"
cluster_name        = "himsha-network"
network             = "testnet"
node_instance_type  = "t3.xlarge"
min_nodes           = 2
max_nodes           = 5
bitcoin_disk_gb     = 100
electrs_disk_gb     = 100
ord_disk_gb         = 100
domain_name         = "him.yourdomain.com"
acm_certificate_arn = "arn:aws:acm:us-east-1:123456789012:certificate/..."

# These are sensitive — consider using AWS SSM or environment variables instead
bitcoin_rpc_user    = "himuser"
bitcoin_rpc_pass    = "your_strong_random_password_here_min_20_chars"
```

---

## Part 2 — Kubernetes Manifests

### `kubernetes/namespaces.yaml`

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: himsha-infra
  labels:
    app.kubernetes.io/managed-by: terraform
```

### `kubernetes/secrets/bitcoin-rpc-secret.yaml`

```yaml
# In production: use External Secrets Operator to pull from AWS Secrets Manager.
# For development only — do not commit real credentials.
apiVersion: v1
kind: Secret
metadata:
  name: bitcoin-rpc-creds
  namespace: himsha-infra
type: Opaque
stringData:
  username: "himuser"
  password: "change_me_before_use"
```

### `kubernetes/bitcoin/configmap.yaml`

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: bitcoin-config
  namespace: himsha-infra
data:
  bitcoin.conf: |
    testnet=1
    server=1
    txindex=1
    blockfilterindex=1
    rpcbind=0.0.0.0
    rpcallowip=0.0.0.0/0
    zmqpubrawblock=tcp://0.0.0.0:28332
    zmqpubrawtx=tcp://0.0.0.0:28333
    zmqpubhashblock=tcp://0.0.0.0:28334
    dbcache=1024
    maxmempool=300
    maxconnections=40
```

### `kubernetes/bitcoin/statefulset.yaml`

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: bitcoin-core
  namespace: himsha-infra
  labels:
    app: bitcoin-core
spec:
  serviceName: bitcoin-core
  replicas: 1
  selector:
    matchLabels:
      app: bitcoin-core
  template:
    metadata:
      labels:
        app: bitcoin-core
      annotations:
        prometheus.io/scrape: "false"
    spec:
      terminationGracePeriodSeconds: 60
      securityContext:
        runAsUser: 1000
        runAsGroup: 1000
        fsGroup: 1000
      containers:
        - name: bitcoin-core
          image: ruimarinho/bitcoin-core:24
          imagePullPolicy: IfNotPresent
          command:
            - bitcoind
            - -conf=/bitcoin-config/bitcoin.conf
            - -datadir=/data
            - -rpcuser=$(BITCOIN_RPC_USER)
            - -rpcpassword=$(BITCOIN_RPC_PASS)
          env:
            - name: BITCOIN_RPC_USER
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: username
            - name: BITCOIN_RPC_PASS
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: password
          ports:
            - name: rpc
              containerPort: 18332    # testnet; 8332 mainnet
            - name: zmq-block
              containerPort: 28332
            - name: zmq-tx
              containerPort: 28333
          readinessProbe:
            exec:
              command:
                - /bin/sh
                - -c
                - >
                  bitcoin-cli -testnet
                  -rpcuser=$(BITCOIN_RPC_USER)
                  -rpcpassword=$(BITCOIN_RPC_PASS)
                  getblockchaininfo
            initialDelaySeconds: 30
            periodSeconds: 20
            failureThreshold: 20
          livenessProbe:
            exec:
              command:
                - /bin/sh
                - -c
                - >
                  bitcoin-cli -testnet
                  -rpcuser=$(BITCOIN_RPC_USER)
                  -rpcpassword=$(BITCOIN_RPC_PASS)
                  ping
            initialDelaySeconds: 60
            periodSeconds: 60
            failureThreshold: 5
          resources:
            requests:
              cpu: "1000m"
              memory: "2Gi"
            limits:
              cpu: "4000m"
              memory: "8Gi"
          volumeMounts:
            - name: bitcoin-data
              mountPath: /data
            - name: bitcoin-config
              mountPath: /bitcoin-config
      volumes:
        - name: bitcoin-config
          configMap:
            name: bitcoin-config
  volumeClaimTemplates:
    - metadata:
        name: bitcoin-data
      spec:
        accessModes: ["ReadWriteOnce"]
        storageClassName: gp3-encrypted
        resources:
          requests:
            storage: 100Gi    # 700Gi for mainnet

---
apiVersion: v1
kind: Service
metadata:
  name: bitcoin-core
  namespace: himsha-infra
spec:
  selector:
    app: bitcoin-core
  clusterIP: None    # Headless — DNS for StatefulSet
  ports:
    - name: rpc
      port: 18332
    - name: zmq-block
      port: 28332
    - name: zmq-tx
      port: 28333
    - name: zmq-hashblock
      port: 28334
```

### `kubernetes/electrs/configmap.yaml`

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: electrs-config
  namespace: himsha-infra
data:
  config.toml: |
    network = "testnet"
    daemon_rpc_addr = "bitcoin-core:18332"
    daemon_dir = "/bitcoin-data"
    db_dir = "/electrs-data"
    electrum_rpc_addr = "0.0.0.0:50001"
    http_addr = "0.0.0.0:3002"
    monitoring_addr = "0.0.0.0:4224"
    wait_duration_secs = 5
    index_batch_size = 10
    bulk_index_threads = 4
```

### `kubernetes/electrs/deployment.yaml`

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: electrs
  namespace: himsha-infra
  labels:
    app: electrs
spec:
  replicas: 1
  strategy:
    type: Recreate    # Electrs needs exclusive access to its DB
  selector:
    matchLabels:
      app: electrs
  template:
    metadata:
      labels:
        app: electrs
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port:   "4224"
        prometheus.io/path:   "/metrics"
    spec:
      initContainers:
        - name: wait-for-bitcoin
          image: curlimages/curl:8
          command: ["/bin/sh", "-c"]
          args:
            - |
              echo "Waiting for Bitcoin Core..."
              until curl -sf \
                -u "$(cat /rpc/username):$(cat /rpc/password)" \
                -X POST http://bitcoin-core:18332 \
                -H "Content-Type: text/plain" \
                -d '{"method":"getblockchaininfo","params":[],"id":1}'; do
                echo "Bitcoin not ready, retrying in 10s..."
                sleep 10
              done
              echo "Bitcoin Core is ready."
          volumeMounts:
            - name: bitcoin-rpc-creds
              mountPath: /rpc
      containers:
        - name: electrs
          image: getumbrel/electrs:v0.10.2
          command:
            - electrs
            - --conf
            - /etc/electrs/config.toml
            - --auth
            - $(BITCOIN_RPC_USER):$(BITCOIN_RPC_PASS)
          env:
            - name: BITCOIN_RPC_USER
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: username
            - name: BITCOIN_RPC_PASS
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: password
          ports:
            - name: electrum
              containerPort: 50001
            - name: http
              containerPort: 3002
            - name: metrics
              containerPort: 4224
          readinessProbe:
            httpGet:
              path: /blocks/tip/height
              port: 3002
            initialDelaySeconds: 30
            periodSeconds: 15
            failureThreshold: 20
          livenessProbe:
            httpGet:
              path: /blocks/tip/height
              port: 3002
            initialDelaySeconds: 120
            periodSeconds: 30
          resources:
            requests:
              cpu: "500m"
              memory: "1Gi"
            limits:
              cpu: "2000m"
              memory: "4Gi"
          volumeMounts:
            - name: electrs-data
              mountPath: /electrs-data
            - name: bitcoin-data
              mountPath: /bitcoin-data
              readOnly: true
            - name: electrs-config
              mountPath: /etc/electrs
      volumes:
        - name: electrs-data
          persistentVolumeClaim:
            claimName: electrs-pvc
        - name: bitcoin-data
          persistentVolumeClaim:
            claimName: bitcoin-core-bitcoin-data-bitcoin-core-0   # StatefulSet PVC name
        - name: electrs-config
          configMap:
            name: electrs-config
        - name: bitcoin-rpc-creds
          secret:
            secretName: bitcoin-rpc-creds

---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: electrs-pvc
  namespace: himsha-infra
spec:
  accessModes: ["ReadWriteOnce"]
  storageClassName: gp3-encrypted
  resources:
    requests:
      storage: 100Gi

---
apiVersion: v1
kind: Service
metadata:
  name: electrs
  namespace: himsha-infra
spec:
  selector:
    app: electrs
  ports:
    - name: electrum
      port: 50001
    - name: http
      port: 3002
    - name: metrics
      port: 4224
```

### `kubernetes/ord/deployment.yaml`

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ord
  namespace: himsha-infra
  labels:
    app: ord
spec:
  replicas: 1
  strategy:
    type: Recreate
  selector:
    matchLabels:
      app: ord
  template:
    metadata:
      labels:
        app: ord
    spec:
      initContainers:
        - name: wait-for-bitcoin
          image: curlimages/curl:8
          command: ["/bin/sh", "-c"]
          args:
            - |
              until curl -sf \
                -u "$(BITCOIN_RPC_USER):$(BITCOIN_RPC_PASS)" \
                -X POST http://bitcoin-core:18332 \
                -H "Content-Type: text/plain" \
                -d '{"method":"getblockchaininfo","params":[],"id":1}'; do
                sleep 10
              done
          env:
            - name: BITCOIN_RPC_USER
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: username
            - name: BITCOIN_RPC_PASS
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: password
      containers:
        - name: ord
          image: ordinals/ord:latest
          command:
            - ord
            - --testnet
            - --bitcoin-rpc-url
            - "http://bitcoin-core:18332"
            - --bitcoin-rpc-username
            - $(BITCOIN_RPC_USER)
            - --bitcoin-rpc-password
            - $(BITCOIN_RPC_PASS)
            - --data-dir
            - /ord-data
            - server
            - --http-port
            - "8080"
          env:
            - name: BITCOIN_RPC_USER
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: username
            - name: BITCOIN_RPC_PASS
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: password
          ports:
            - name: http
              containerPort: 8080
          readinessProbe:
            httpGet:
              path: /status
              port: 8080
            initialDelaySeconds: 60
            periodSeconds: 30
            failureThreshold: 20
          livenessProbe:
            httpGet:
              path: /status
              port: 8080
            initialDelaySeconds: 180
            periodSeconds: 60
          resources:
            requests:
              cpu: "500m"
              memory: "1Gi"
            limits:
              cpu: "3000m"
              memory: "6Gi"
          volumeMounts:
            - name: ord-data
              mountPath: /ord-data
            - name: bitcoin-data
              mountPath: /bitcoin-data
              readOnly: true
      volumes:
        - name: ord-data
          persistentVolumeClaim:
            claimName: ord-pvc
        - name: bitcoin-data
          persistentVolumeClaim:
            claimName: bitcoin-core-bitcoin-data-bitcoin-core-0

---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: ord-pvc
  namespace: himsha-infra
spec:
  accessModes: ["ReadWriteOnce"]
  storageClassName: gp3-encrypted
  resources:
    requests:
      storage: 100Gi

---
apiVersion: v1
kind: Service
metadata:
  name: ord
  namespace: himsha-infra
spec:
  selector:
    app: ord
  ports:
    - name: http
      port: 8080
```

### `kubernetes/himsha-node/deployment.yaml`

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: himsha-node
  namespace: himsha-infra
  labels:
    app: himsha-node
spec:
  replicas: 1
  strategy:
    type: Recreate
  selector:
    matchLabels:
      app: himsha-node
  template:
    metadata:
      labels:
        app: himsha-node
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port:   "9101"
    spec:
      containers:
        - name: himsha-node
          image: ghcr.io/your-org/himsha-node:latest
          imagePullPolicy: Always
          env:
            - name: HIMSHA_DB
              value: /data/him.redb
            - name: BITCOIN_RPC_URL
              value: "http://bitcoin-core:18332"
            - name: BITCOIN_RPC_USER
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: username
            - name: BITCOIN_RPC_PASS
              valueFrom:
                secretKeyRef:
                  name: bitcoin-rpc-creds
                  key: password
            - name: ELECTRS_URL
              value: "http://electrs:3002"
            - name: ORD_URL
              value: "http://ord:8080"
            - name: RUST_LOG
              value: "himsha_node=info,warn"
          ports:
            - name: rpc
              containerPort: 9100
          readinessProbe:
            exec:
              command:
                - /bin/sh
                - -c
                - >
                  curl -sf -X POST http://localhost:9100
                  -H "Content-Type: application/json"
                  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}'
            initialDelaySeconds: 10
            periodSeconds: 15
          livenessProbe:
            exec:
              command:
                - /bin/sh
                - -c
                - >
                  curl -sf -X POST http://localhost:9100
                  -H "Content-Type: application/json"
                  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_getSlot","params":[]}'
            initialDelaySeconds: 30
            periodSeconds: 30
          resources:
            requests:
              cpu: "250m"
              memory: "512Mi"
            limits:
              cpu: "2000m"
              memory: "4Gi"
          volumeMounts:
            - name: himsha-data
              mountPath: /data
      volumes:
        - name: himsha-data
          persistentVolumeClaim:
            claimName: himsha-node-pvc

---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: himsha-node-pvc
  namespace: himsha-infra
spec:
  accessModes: ["ReadWriteOnce"]
  storageClassName: gp3-encrypted
  resources:
    requests:
      storage: 20Gi

---
apiVersion: v1
kind: Service
metadata:
  name: himsha-node
  namespace: himsha-infra
spec:
  selector:
    app: himsha-node
  ports:
    - name: rpc
      port: 9100
```

### `kubernetes/ingress/ingress.yaml`

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: himsha-ingress
  namespace: himsha-infra
  annotations:
    kubernetes.io/ingress.class: nginx
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "300"
    nginx.ingress.kubernetes.io/proxy-body-size: "10m"
    # Rate limiting — protect RPC endpoint
    nginx.ingress.kubernetes.io/limit-rps: "20"
    nginx.ingress.kubernetes.io/limit-connections: "10"
spec:
  tls:
    - hosts:
        - api.him.yourdomain.com
        - ord.him.yourdomain.com
      secretName: himsha-tls-secret
  rules:
    - host: api.him.yourdomain.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: himsha-node
                port:
                  number: 9100
    - host: ord.him.yourdomain.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: ord
                port:
                  number: 8080
```

### `kubernetes/monitoring/servicemonitor-electrs.yaml`

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: electrs
  namespace: himsha-infra
  labels:
    release: kube-prometheus-stack
spec:
  selector:
    matchLabels:
      app: electrs
  endpoints:
    - port: metrics
      interval: 30s
      path: /metrics
```

---

## Part 3 — Deploy

### Step 1: Bootstrap Terraform state bucket

```bash
# Create S3 bucket for state (one-time)
aws s3api create-bucket \
  --bucket himsha-terraform-state-$(aws sts get-caller-identity --query Account --output text) \
  --region us-east-1

aws s3api put-bucket-versioning \
  --bucket himsha-terraform-state-$(aws sts get-caller-identity --query Account --output text) \
  --versioning-configuration Status=Enabled

# Create DynamoDB lock table
aws dynamodb create-table \
  --table-name himsha-terraform-locks \
  --attribute-definitions AttributeName=LockID,AttributeType=S \
  --key-schema AttributeName=LockID,KeyType=HASH \
  --billing-mode PAY_PER_REQUEST \
  --region us-east-1
```

### Step 2: Apply Terraform

```bash
cd infrastructure/terraform

# Initialize
terraform init

# Preview changes
terraform plan -var-file=terraform.tfvars -out=tfplan

# Apply (creates VPC, EKS, EBS driver, nginx ingress)
terraform apply tfplan

# Configure kubectl
$(terraform output -raw configure_kubectl)

# Verify cluster
kubectl get nodes
kubectl get ns
```

### Step 3: Deploy Kubernetes workloads

```bash
cd infrastructure/kubernetes

# Apply in order
kubectl apply -f namespaces.yaml
kubectl apply -f secrets/bitcoin-rpc-secret.yaml

kubectl apply -f bitcoin/configmap.yaml
kubectl apply -f bitcoin/statefulset.yaml

# Wait for Bitcoin to be healthy before starting indexers
kubectl -n himsha-infra wait --for=condition=ready pod/bitcoin-core-0 --timeout=300s

kubectl apply -f electrs/configmap.yaml
kubectl apply -f electrs/deployment.yaml
kubectl apply -f ord/deployment.yaml
kubectl apply -f himsha-node/deployment.yaml
kubectl apply -f ingress/ingress.yaml
kubectl apply -f monitoring/
```

### Step 4: Verify

```bash
# Watch all pods start
kubectl -n himsha-infra get pods -w

# Check pod logs
kubectl -n himsha-infra logs -f statefulset/bitcoin-core
kubectl -n himsha-infra logs -f deployment/electrs
kubectl -n himsha-infra logs -f deployment/ord
kubectl -n himsha-infra logs -f deployment/himsha-node

# Port-forward for local testing (no ingress needed)
kubectl -n himsha-infra port-forward svc/himsha-node 9100:9100 &
kubectl -n himsha-infra port-forward svc/electrs  3002:3002 &
kubectl -n himsha-infra port-forward svc/ord       8080:8080 &

# Verify HIMSHA node
curl -s -X POST http://localhost:9100 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"himsha_isNodeReady","params":[]}' | jq .

# Verify Electrs
curl -s http://localhost:3002/blocks/tip/height

# Verify Ord
curl -s http://localhost:8080/status | jq .
```

---

## Part 4 — Scaling & Operations

```bash
# Scale HIMSHA node replicas (note: stateful — only safe if using shared PVC or read replicas)
kubectl -n himsha-infra scale deployment himsha-node --replicas=2

# Expand a PVC (gp3 supports online expansion)
kubectl -n himsha-infra patch pvc electrs-pvc -p '{"spec":{"resources":{"requests":{"storage":"200Gi"}}}}'

# Rolling restart (pull new image)
kubectl -n himsha-infra rollout restart deployment/himsha-node

# Force re-index (delete and recreate Electrs PVC — DESTRUCTIVE)
kubectl -n himsha-infra delete deployment electrs
kubectl -n himsha-infra delete pvc electrs-pvc
kubectl apply -f electrs/deployment.yaml

# Execute a command inside a pod
kubectl -n himsha-infra exec -it deployment/himsha-node -- /bin/sh
```

---

## Part 5 — Teardown

```bash
# Remove Kubernetes resources (PVCs with Retain policy keep data)
kubectl delete namespace himsha-infra

# Destroy AWS infrastructure (IRREVERSIBLE — destroys all data)
cd infrastructure/terraform
terraform destroy -var-file=terraform.tfvars
```

---

## Cost Estimate (AWS, Testnet, us-east-1)

| Resource | Spec | $/month |
|----------|------|---------|
| EKS control plane | — | $73 |
| 2× t3.xlarge nodes | 4 vCPU / 16 GB | $240 |
| EBS bitcoin-pvc | 100 GB gp3 | $8 |
| EBS electrs-pvc | 100 GB gp3 | $8 |
| EBS ord-pvc | 100 GB gp3 | $8 |
| EBS himsha-pvc | 20 GB gp3 | $2 |
| NAT gateway (2 AZ) | — | $90 |
| ALB | — | $18 |
| **Total** | | **~$447/mo** |

> Mainnet: Add ~$50/mo for larger EBS volumes (700 GB bitcoin + 200 GB others).
