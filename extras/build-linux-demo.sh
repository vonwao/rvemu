#!/bin/bash
# Build the extras demo image (base certified image + Micro Tetris).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/vendor/linux-demo"

# Pre-fetch links2 source (twibright.com is flaky from inside Docker); pinned.
LINKS_TGZ="$HERE/linux-demo/links-2.30.tar.gz"
LINKS_SHA="7f0d54f4f7d1f094c25c9cbd657f98bc998311122563b1d757c9aeb1d3423b9e"
if [ ! -f "$LINKS_TGZ" ]; then
  curl -s --max-time 300 -o "$LINKS_TGZ" http://links.twibright.com/download/links-2.30.tar.gz
fi
echo "$LINKS_SHA  $LINKS_TGZ" | shasum -a 256 -c - || { echo "links tarball sha256 mismatch"; exit 1; }

docker build -t rvemu-linux-demo "$HERE/linux-demo"
mkdir -p "$OUT"
cid=$(docker create rvemu-linux-demo)
docker cp "$cid:/build/fw_payload_demo.elf" "$OUT/fw_payload.elf"
docker rm "$cid" >/dev/null
ls -la "$OUT"
