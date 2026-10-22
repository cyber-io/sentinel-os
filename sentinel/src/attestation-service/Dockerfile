# Sentinel Attestation Service - Production Dockerfile
# Multi-stage build for minimal attack surface

# Build stage
FROM rust:1.98-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libtss2-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace Cargo.toml and Cargo.lock for dependency caching
COPY sentinel/src/attestation-service/Cargo.toml .
COPY sentinel/src/attestation-service/Cargo.lock .

# Create dummy main.rs to cache dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source
COPY sentinel/src/attestation-service/src ./src

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM debian:12-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libtss2-0 \
    tpm2-tools \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -u 1000 -g nogroup -m -d /home/sentinel sentinel

# Copy CA certificates
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /build/target/release/attestation-service /usr/local/bin/attestation-service

# Create directories for TPM/IMA logs
RUN mkdir -p /var/log/ima /sys/kernel/security/tpm0 && chown -R sentinel:nogroup /var/log/ima

USER sentinel:nogroup

# Expose ports
EXPOSE 50056 8086

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/attestation-service", "health"]

# Entry point
ENTRYPOINT ["/usr/local/bin/attestation-service"]
CMD ["serve"]