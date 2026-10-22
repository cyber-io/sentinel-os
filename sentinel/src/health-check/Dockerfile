# Sentinel Health Check - Production Dockerfile
# Multi-stage build for minimal attack surface

# Build stage
FROM rust:1.98-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace Cargo.toml and Cargo.lock for dependency caching
COPY sentinel/src/health-check/Cargo.toml .
COPY sentinel/src/health-check/Cargo.lock .

# Create dummy main.rs to cache dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source
COPY sentinel/src/health-check/src ./src

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

# Copy CA certificates for TLS
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /build/target/release/health-check /usr/local/bin/health-check

# Create non-root user
USER nonroot:nonroot

# Expose ports
EXPOSE 50057 8087

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/health-check", "health"]

# Entry point
ENTRYPOINT ["/usr/local/bin/health-check"]
CMD ["serve"]