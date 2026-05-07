# Shared multi-stage build for the Rust workspace. The target binary is
# selected by the BIN build arg so backend and sim share the same builder
# layer (and therefore the same dependency cache).
FROM rust:1.83-slim-bookworm AS builder
ARG BIN
WORKDIR /app

RUN apt-get update \
 && apt-get install -y --no-install-recommends protobuf-compiler pkg-config \
 && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock* ./
COPY rust-toolchain.toml ./
COPY proto ./proto
COPY crates ./crates

RUN cargo build --release --bin ${BIN}

FROM debian:bookworm-slim AS runtime
ARG BIN
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/${BIN} /usr/local/bin/app
ENV RUST_LOG=info
ENTRYPOINT ["/usr/local/bin/app"]
