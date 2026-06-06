# =========================================================
# HIMSHA Node — multi-stage build
#
# Default (native dispatch) build: does NOT require the RISC Zero toolchain.
# The `zkvm` feature is off by default, so built-in programs run via native
# dispatch. See docs/zkvm-proving.md to enable real proving.
# =========================================================

# --- Stage 1: builder ---
# Needs >= 1.85 (edition 2024 is used by transitive deps like indexmap 2.14).
FROM rust:1.90-slim-bookworm AS builder

WORKDIR /build

# build-essential + autotools + cmake are needed because risc0 pulls in
# `protobuf-src`, which compiles protobuf from C++ source (g++, make, autoconf,
# automake, libtool). pkg-config/libssl-dev are for native TLS.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev clang \
        build-essential cmake autoconf automake libtool \
    && rm -rf /var/lib/apt/lists/*

# Copy the whole workspace. .dockerignore keeps target/, website/, etc. out so
# the context stays small. (The per-crate copy trick in the old docs is omitted
# on purpose — it broke every time a workspace member was added.)
COPY . .

# Debug build of just the node binary.
#
# Intentionally NOT --release: the workspace release profile uses
# `lto = true` + `codegen-units = 1`, and LTO-linking the full dependency
# tree (risc0, tokio, bitcoin libs) is memory-hungry enough to OOM-kill the
# Docker Desktop VM. A debug build is plenty for a local test node.
#
# CARGO_BUILD_JOBS=2 caps parallel rustc processes so peak RAM stays under the
# Docker VM's memory (compiling risc0 8-wide OOM-kills a 4 GB VM on an 8 GB host).
ENV CARGO_BUILD_JOBS=2 CARGO_NET_RETRY=5
RUN cargo build -p himsha-node

# --- Stage 2: runtime ---
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create /data owned by himuser BEFORE declaring the volume, so a fresh named
# volume inherits 1001:1001 ownership (otherwise it inits root-owned and the
# non-root node can't write him.redb).
RUN useradd -r -u 1001 -m -s /sbin/nologin himuser \
    && mkdir -p /data && chown -R 1001:1001 /data
USER himuser

COPY --from=builder /build/target/debug/himsha-node /usr/local/bin/himsha-node

# Bind to all interfaces inside the container so the published port is reachable.
ENV HIMSHA_BIND=0.0.0.0:9100
ENV HIMSHA_DB=/data/him.redb

VOLUME ["/data"]
EXPOSE 9100

ENTRYPOINT ["himsha-node"]
