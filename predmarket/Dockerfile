FROM rust:1.75-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    libpq-dev \
    cmake \
    clang \
    llvm \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY api/Cargo.toml api/Cargo.lock ./
COPY api/crates ./crates

RUN cargo build --release --bin api-http

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -s /bin/bash predmarket

WORKDIR /app

COPY --from=builder /build/target/release/api-http /app/api-http
COPY --from=builder /build/target/release/libpredmarket_engine.so /app/lib/

RUN chown -R predmarket:predmarket /app

USER predmarket

EXPOSE 8080

ENTRYPOINT ["/app/api-http"]
