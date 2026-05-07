#!/usr/bin/env bash
# One-stop verification: runs the Rust workspace's tests inside a
# throwaway container (no host rustc needed) and the web tests on the
# host. Named docker volumes cache cargo's registry and the target
# directory so reruns are fast.
#
# Usage:  ./scripts/check.sh           # build image, run all tests
#         ./scripts/check.sh rust      # rust workspace only
#         ./scripts/check.sh web       # web only

set -euo pipefail

cd "$(dirname "$0")/.."

IMAGE=fleet-tester:local
CARGO_VOL=fleet-cargo-cache
TARGET_VOL=fleet-target-cache

run_rust() {
  echo "==> building tester image (cached after first run)"
  docker build -f docker/test.Dockerfile -t "$IMAGE" docker

  echo "==> cargo test --workspace"
  docker run --rm \
    -v "$PWD":/app \
    -v "$CARGO_VOL":/usr/local/cargo/registry \
    -v "$TARGET_VOL":/app/target \
    -w /app \
    "$IMAGE" \
    cargo test --workspace --no-fail-fast
}

run_web() {
  echo "==> web typecheck + tests"
  if [ ! -d web/node_modules ]; then
    ( cd web && npm install --silent )
  fi
  ( cd web && npm run typecheck && npm test )
}

case "${1:-all}" in
  rust) run_rust ;;
  web)  run_web ;;
  all)  run_rust && run_web ;;
  *)    echo "unknown target: $1" >&2; exit 2 ;;
esac

echo
echo "OK"
