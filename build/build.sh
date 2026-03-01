#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
docker build -f build/Dockerfile --target prod -t frona "$@" .
