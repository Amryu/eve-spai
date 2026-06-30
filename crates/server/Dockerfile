# syntax=docker/dockerfile:1
#
# Production image for the EVE Spai battle-report API server (`eve-spai-br`).
#
# BUILD CONTEXT MUST BE THE REPO ROOT (the directory that contains the top-level
# `Cargo.toml`), NOT `crates/server`. Reason: `crates/server` is its OWN cargo
# workspace, but it pulls in `crates/br-core` by path, and `br-core` inherits
# `version`/`edition`/`license` from the repo-root `[workspace.package]` table
# (`version.workspace = true`). Without the repo-root `Cargo.toml` present, br-core's
# manifest fails to parse ("failed to load manifest for dependency `br-core`"), so a
# `crates/`-only context will NOT build. The repo root is the smallest context that
# carries that root manifest plus both crates.
#
# We COPY only the three paths we need (root Cargo.toml + the two crates), never
# `COPY . .`, so `app/`, `site/`, `target/`, `.git/` etc. never enter the build and no
# .dockerignore at the repo root is required. The repo-root manifest still lists
# `app` as a workspace member, but cargo tolerates the missing member dir when the
# root workspace is consulted only for br-core's field inheritance (verified).
#
# Build from the repo root, e.g.:
#   docker build -f crates/server/Dockerfile -t eve-spai-br .
# (docker compose in deploy/docker-compose.yml sets context to the repo root for you.)

############################
# Stage 1 — builder
############################
FROM rust:1-slim-bookworm AS builder

# rustls is used throughout (sqlx runtime-tokio-rustls, reqwest rustls-tls), so we do
# NOT need OpenSSL/pkg-config. Keep build deps minimal: ca-certs for crates.io TLS.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Repo-root workspace manifest — required so br-core can resolve its inherited
# `version.workspace`/`edition.workspace`/`license.workspace` fields.
COPY Cargo.toml ./Cargo.toml

# The two crates we actually compile. br-core is a path dependency of the server.
COPY crates/br-core crates/br-core
COPY crates/server  crates/server

# Build the server bin. The server is its own workspace, so its lockfile lives at
# crates/server/Cargo.lock (copied above); --locked keeps the build reproducible.
# The SQL migrations are embedded into the binary at compile time via sqlx::migrate!,
# so no migration files need to ship in the runtime image.
WORKDIR /build/crates/server
RUN cargo build --release --locked --bin eve-spai-br \
    && strip target/release/eve-spai-br || true

############################
# Stage 2 — runtime
############################
FROM debian:bookworm-slim AS runtime

# ca-certificates: the server fetches EVE's JWKS over HTTPS (rustls) to verify SSO
# tokens, and curl is used by the compose healthcheck. No OpenSSL runtime dep needed.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin appuser

COPY --from=builder /build/crates/server/target/release/eve-spai-br /usr/local/bin/eve-spai-br

USER appuser

# Internal port only — nginx (on the `frontend` docker network) reaches the container
# by its service DNS name `eve-spai-br`. Set BIND_ADDR=0.0.0.0:8090 in the compose env.
EXPOSE 8090

ENTRYPOINT ["/usr/local/bin/eve-spai-br"]
