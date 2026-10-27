# Sentinel Capability Broker - Production Dockerfile
# Multi-stage build for minimal attack surface

# Build stage
FROM rust:1.99-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace Cargo.toml and Cargo.lock for dependency caching
COPY sentinel/src/capability-broker/Cargo.toml .
COPY sentinel/src/capability-broker/Cargo.lock .

# Create dummy main.rs to cache dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source
COPY sentinel/src/capability-broker/src ./src

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

# Copy CA certificates for TLS
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /build/target/release/capability-broker /usr/local/bin/capability-broker

# Create non-root user (distroless uses nonroot:nonroot by default)
USER nonroot:nonroot

# Expose ports
EXPOSE 50051 8080 8081

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/capability-broker", "health"]

# Entry point
ENTRYPOINT ["/usr/local/bin/capability-broker"]
CMD ["serve"]