#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

docker buildx inspect multiarch >/dev/null 2>&1 || \
  docker buildx create --name multiarch --use
docker buildx use multiarch

docker buildx build --platform linux/amd64,linux/arm64 \
  -f build/Dockerfile --target prod \
  -t ghcr.io/fronalabs/frona:latest \
  --push "$@" .
