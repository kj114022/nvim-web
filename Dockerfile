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
    ca-certificates \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Neovim v0.10.0 (Debian package is too old)
RUN curl -LO https://github.com/neovim/neovim/releases/download/v0.10.0/nvim-linux64.tar.gz \
    && tar xzf nvim-linux64.tar.gz -C /opt \
    && rm nvim-linux64.tar.gz \
    && ln -s /opt/nvim-linux64/bin/nvim /usr/local/bin/nvim

# Copy binary from builder
COPY --from=builder /app/target/release/nvim-web-host /usr/local/bin/

# Expose ports
EXPOSE 8080 9001

# Run as non-root user
RUN useradd -m nvim
USER nvim
WORKDIR /home/nvim

ENTRYPOINT ["nvim-web-host"]
