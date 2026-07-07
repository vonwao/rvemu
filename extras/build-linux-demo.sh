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


GPM_TGZ="$HERE/linux-demo/gpm-1.20.7.tar.gz"
GPM_SHA="c7e4661c24e05ae13547176b649bac8e3a0db2575f7dd57559f9e0b509f90f49"
if [ ! -f "$GPM_TGZ" ]; then
  curl -sL --max-time 300 -o "$GPM_TGZ" http://deb.debian.org/debian/pool/main/g/gpm/gpm_1.20.7.orig.tar.gz
fi
echo "$GPM_SHA  $GPM_TGZ" | shasum -a 256 -c - || { echo "gpm tarball sha256 mismatch"; exit 1; }

docker build -t rvemu-linux-demo "$HERE/linux-demo"
mkdir -p "$OUT"
cid=$(docker create rvemu-linux-demo)
docker cp "$cid:/build/fw_payload_demo.elf" "$OUT/fw_payload.elf"
docker rm "$cid" >/dev/null
ls -la "$OUT"
