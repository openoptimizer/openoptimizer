# Multi-stage build for Cutting Stock Optimizer
# This builds all components in a Rust builder container and creates minimal runtime images

# Stage 1: Builder
FROM rust:latest AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy workspace files
COPY Cargo.toml ./
COPY crates ./crates
COPY openapi.yaml ./openapi.yaml

# Build release binaries
RUN cargo build --release

# Stage 2: Runtime image for API
FROM debian:bookworm-slim AS api

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the API binary
COPY --from=builder /app/target/release/optimizer-api /app/optimizer-api

# Copy web UI
COPY web ./web

# Copy examples (optional, for testing)
COPY examples ./examples

# Copy OpenAPI specification for reference
COPY openapi.yaml ./openapi.yaml

# Expose API port
EXPOSE 3000

# Run the API server
CMD ["/app/optimizer-api"]

# Stage 3: Runtime image for CLI
FROM debian:bookworm-slim AS cli

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the CLI binary
COPY --from=builder /app/target/release/optimizer /app/optimizer

# Copy examples
COPY examples ./examples

# Default command shows help
ENTRYPOINT ["/app/optimizer"]
CMD ["--help"]
