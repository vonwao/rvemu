#!/bin/bash
# Build the extras demo image (base certified image + Micro Tetris).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/vendor/linux-demo"
docker build -t rvemu-linux-demo "$HERE/linux-demo"
mkdir -p "$OUT"
cid=$(docker create rvemu-linux-demo)
docker cp "$cid:/build/fw_payload_demo.elf" "$OUT/fw_payload.elf"
docker rm "$cid" >/dev/null
ls -la "$OUT"
