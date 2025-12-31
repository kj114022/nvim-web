# Multi-stage build for nvim-web
# Stage 1: Build
FROM rust:1.75-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build release binary
RUN cargo build --release -p nvim-web-host

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    neovim \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/nvim-web-host /usr/local/bin/

# Expose ports
EXPOSE 8080 9001

# Run as non-root user
RUN useradd -m nvim
USER nvim
WORKDIR /home/nvim

ENTRYPOINT ["nvim-web-host"]
