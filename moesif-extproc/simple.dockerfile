# syntax=docker/dockerfile:1.4

# Stage 1: Builder
FROM rust:latest AS builder

# Install necessary build dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libprotobuf-dev \
    cmake \
    pkg-config \
    libssl-dev

# Install target architecture
RUN rustup target add x86_64-unknown-linux-gnu

WORKDIR /app

# Copy over the cargo manifests and build scripts
COPY Cargo.toml Cargo.lock build.rs ./

# Copy the protobuf dependency directories used in build.rs
COPY data-plane-api/ data-plane-api/
COPY udpa/ udpa/
COPY protoc-gen-validate/ protoc-gen-validate/
COPY xds/ xds/

# Prepare the persistent cargo registry and target directories
RUN mkdir -p /usr/local/cargo/registry /usr/local/cargo/git

# Copy the actual source code (excluding the already copied directories)
COPY . . 

# Build the application with cache mount and copy to a persistent directory
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --target x86_64-unknown-linux-gnu && \
    cp -r /app/target/x86_64-unknown-linux-gnu/release/moesif_envoy_extproc_plugin /app/moesif_envoy_extproc_plugin


# Stage 2: Final image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/moesif_envoy_extproc_plugin /app/moesif_envoy_extproc_plugin

CMD ["/app/moesif_envoy_extproc_plugin"]
