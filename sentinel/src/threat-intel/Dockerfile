# Sentinel Threat Intelligence - Production Dockerfile
# Multi-stage build for minimal attack surface

# Build stage
FROM rust:1.82-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace Cargo.toml and Cargo.lock for dependency caching
COPY sentinel/src/threat-intel/Cargo.toml .
COPY sentinel/src/threat-intel/Cargo.lock .

# Create dummy main.rs to cache dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source
COPY sentinel/src/threat-intel/src ./src

# Build the actual application
RUN cargo build --release

# Runtime stage - need additional libs for ONNX runtime
FROM debian:12-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -u 1000 -g nogroup -m -d /home/sentinel sentinel

# Copy CA certificates
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /build/target/release/threat-intel /usr/local/bin/threat-intel

# Create model directory
RUN mkdir -p /opt/sentinel/models/threat-intel && chown -R sentinel:nogroup /opt/sentinel

USER sentinel:nogroup

# Expose ports
EXPOSE 50055 8085

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/threat-intel", "health"]

# Entry point
ENTRYPOINT ["/usr/local/bin/threat-intel"]
CMD ["serve"]