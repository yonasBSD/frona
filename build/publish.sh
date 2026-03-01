#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
build/build.sh "$@"
docker tag frona:latest ghcr.io/fronalabs/frona:latest
docker push ghcr.io/fronalabs/frona:latest
