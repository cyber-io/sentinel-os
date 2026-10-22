# Sentinel Operator TUI - Production Dockerfile
# Multi-stage build for minimal attack surface

# Build stage
FROM rust:1.98-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace Cargo.toml and Cargo.lock for dependency caching
COPY sentinel/src/operator-tui/Cargo.toml .
COPY sentinel/src/operator-tui/Cargo.lock .

# Create dummy main.rs to cache dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source
COPY sentinel/src/operator-tui/src ./src

# Build the actual application
RUN cargo build --release

# Runtime stage - need X11 libs for TUI
FROM debian:12-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libxcb-render0 \
    libxcb-shape0 \
    libxcb-xfixes0 \
    libxkbcommon0 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -u 1000 -g nogroup -m -d /home/sentinel sentinel

# Copy CA certificates
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /build/target/release/operator-tui /usr/local/bin/operator-tui

USER sentinel:nogroup

# Expose ports (for API/WebSocket)
EXPOSE 8088

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/operator-tui", "health"]

# Entry point
ENTRYPOINT ["/usr/local/bin/operator-tui"]
CMD ["serve"]