# syntax=docker/dockerfile:1

# =============================================================================
# Multi-stage Dockerfile for Talon workspace
# Build any binary with: docker build --build-arg BINARY=talon-gateway .
# =============================================================================

ARG RUST_VERSION=1.93.0

# -----------------------------------------------------------------------------
# Stage 1: chef - install cargo-chef for dependency caching
# -----------------------------------------------------------------------------
FROM rust:${RUST_VERSION}-bookworm AS chef

RUN cargo install cargo-chef --locked
WORKDIR /app

# -----------------------------------------------------------------------------
# Stage 2: planner - analyze workspace and produce a dependency recipe
# -----------------------------------------------------------------------------
FROM chef AS planner

# Copy the full workspace (source + manifests + proto + build scripts)
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# -----------------------------------------------------------------------------
# Stage 3: builder - compile dependencies (cached), then build the binary
# -----------------------------------------------------------------------------
FROM chef AS builder

ARG BINARY=talon-gateway

# Install protobuf compiler (required by build.rs in talon-gateway and talon-channel-sdk)
RUN apt-get update && \
    apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && \
    rm -rf /var/lib/apt/lists/*

# Cook dependencies first (this layer is cached as long as recipe.json is unchanged)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Now copy the full source and build the target binary
COPY . .
RUN cargo build --release --bin ${BINARY} && \
    cp target/release/${BINARY} /app/binary

# -----------------------------------------------------------------------------
# Stage 4: runtime - minimal image with just the compiled binary
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

ARG BINARY=talon-gateway

# Install minimal runtime dependencies (TLS certs, ca-certificates for HTTPS)
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN groupadd --gid 1001 talon && \
    useradd --uid 1001 --gid talon --shell /bin/false --create-home talon

# Copy the compiled binary
COPY --from=builder /app/binary /usr/local/bin/service

# Copy config and policy files (gateway needs these at runtime)
COPY config.toml /etc/talon/config.toml
COPY policies/ /etc/talon/policies/

# Set working directory for config-relative paths
WORKDIR /etc/talon

USER talon

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/service"]
