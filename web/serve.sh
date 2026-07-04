#!/bin/bash
# Serve the browser demo: copies the current wasm build + Linux image next to
# index.html and starts a local server. Open http://localhost:8000/
cd "$(dirname "$0")"
cp ../target/wasm32-unknown-unknown/release/rvemu_wasm.wasm .
cp ../targets/vendor/linux-build/fw_payload.elf .
exec python3 -m http.server 8000
