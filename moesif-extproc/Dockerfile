# syntax=docker/dockerfile:1.4

# Stage 0: Base image with common dependencies
FROM rust:latest AS base

# Install necessary build dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libprotobuf-dev \
    cmake \
    pkg-config \
    libssl-dev

# Install cargo-chef and sccache
RUN cargo install cargo-chef sccache

# Set environment variables for sccache
ENV RUSTC_WRAPPER=/usr/local/cargo/bin/sccache
ENV SCCACHE_DIR=/sccache
ENV SCCACHE_CACHE_SIZE=1G

# Install target architecture
RUN rustup target add x86_64-unknown-linux-gnu

# Stage 1: Planner
FROM base AS planner

WORKDIR /app

# Copy the build files
COPY Cargo.toml Cargo.lock ./

# **Include the .proto files and other relevant directories early**
COPY build.rs .
COPY data-plane-api/ data-plane-api/
COPY udpa/ udpa/
COPY protoc-gen-validate/ protoc-gen-validate/
COPY xds/ xds/

# Generate the cargo chef recipe
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Cacher
FROM base AS cacher

WORKDIR /app

# Copy the recipe file generated in the planner stage
COPY --from=planner /app/recipe.json recipe.json

# Cook the dependencies (this layer will be cached unless dependencies change)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --target x86_64-unknown-linux-gnu --recipe-path recipe.json

# Stage 3: Builder
FROM base AS builder

WORKDIR /app

# Copy over the cached dependencies
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo

# Copy the source code
COPY . .

# Build the application
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build --release --target x86_64-unknown-linux-gnu

# Stage 4: Final image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates && \
    update-ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/moesif_envoy_extproc_plugin /app/moesif_plugin

CMD ["/app/moesif_plugin"]