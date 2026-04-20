#!/usr/bin/env bash
set -e

cargo build -p frona-cli-mcp
cp target/debug/mcpctl /app/bin/mcpctl

cargo run -p frona-server
