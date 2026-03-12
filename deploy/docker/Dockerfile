# AZCOIN mining pool - production Docker image
FROM rust:1-bookworm AS builder

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
RUN cargo build --release -p azcoin-pool

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/azcoin-pool /usr/local/bin/

EXPOSE 8080 3333

ENTRYPOINT ["azcoin-pool"]
