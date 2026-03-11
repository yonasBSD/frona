#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
docker buildx build --platform "${PLATFORM:-linux/amd64,linux/arm64}" \
  -f build/Dockerfile --target prod -t frona "$@" .
