#!/usr/bin/env bash
set -euo pipefail

# Thin wrapper around the canonical Swift/FluidAudio sidecar build.

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SWIFT_DIR="$ROOT_DIR/sidecar/parakeet-swift"

if [[ ! -d "$SWIFT_DIR" ]]; then
  echo "[sidecar] Swift sidecar sources not found at $SWIFT_DIR" >&2
  exit 1
fi

exec "$SWIFT_DIR/build.sh" "$@"
