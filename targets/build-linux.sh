#!/bin/bash
# Build the Gate C Linux target in Docker (see targets/linux/Dockerfile for
# the pinned recipe). Produces targets/vendor/linux-build/fw_payload.elf
# (OpenSBI 1.6 + Linux 6.12.10 + BusyBox 1.36.1 initramfs + platform DTB)
# plus the exact kernel .config used.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/vendor/linux-build"

docker build -t rvemu-linux-target "$HERE/linux"
mkdir -p "$OUT"
cid=$(docker create rvemu-linux-target)
docker cp "$cid:/build/fw_payload.elf" "$OUT/fw_payload.elf"
docker cp "$cid:/build/Image" "$OUT/Image"
docker cp "$cid:/build/kernel.config" "$OUT/kernel.config"
docker cp "$cid:/build/platform.dtb" "$OUT/platform.dtb"
docker rm "$cid" >/dev/null
echo "Linux target artifacts in $OUT:"
ls -la "$OUT"
