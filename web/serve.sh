#!/bin/bash
# Serve the browser demo: copies the current wasm build + Linux image next to
# index.html and starts a local server. Open http://localhost:8000/
cd "$(dirname "$0")"
cp ../target/wasm32-unknown-unknown/release/rvemu_wasm.wasm .
# Demo image (certified base + tetris) if built, else the certified image.
if [ -f ../extras/vendor/linux-demo/fw_payload.elf ]; then
  cp ../extras/vendor/linux-demo/fw_payload.elf .
else
  cp ../targets/vendor/linux-build/fw_payload.elf .
fi
exec python3 -m http.server 8000
