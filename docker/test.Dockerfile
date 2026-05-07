# Image used by scripts/check.sh to run the Rust workspace's tests
# without requiring rustc on the host. Kept minimal; the actual cargo
# invocation lives in the script so we don't bake test args into the
# image layer.
FROM rust:1.89-slim-bookworm
RUN apt-get update \
 && apt-get install -y --no-install-recommends protobuf-compiler pkg-config ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
