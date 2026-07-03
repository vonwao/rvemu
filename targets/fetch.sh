#!/bin/bash
# Reproduce targets/vendor/ exactly as pinned in PINS.md. Run from repo root.
set -euo pipefail
cd "$(dirname "$0")/vendor" 2>/dev/null || { mkdir -p "$(dirname "$0")/vendor"; cd "$(dirname "$0")/vendor"; }

clone_pin() { # url dir commit
  [ -d "$2" ] || git clone "$1" "$2"
  git -C "$2" fetch -q origin "$3" 2>/dev/null || true
  git -C "$2" checkout -q "$3"
  git -C "$2" submodule update --init --recursive -q
}

clone_pin https://github.com/riscv-software-src/riscv-isa-sim.git spike 55b4658dbf574ba0b714083ec436ce2cb5be1998
clone_pin https://github.com/riscv-software-src/riscv-tests.git riscv-tests 34e6b6d1e7936b526075432fb730d89148623484
[ -d riscv-arch-test ] || git clone --depth 1 -b 3.9.1 https://github.com/riscv-non-isa/riscv-arch-test.git riscv-arch-test

[ -d riscof-venv ] || { python3 -m venv riscof-venv && ./riscof-venv/bin/pip install riscof==1.25.3; }

XPACK=xpack-riscv-none-elf-gcc-15.2.0-1
[ -d "$XPACK" ] || {
  curl -sL -o xpack-gcc.tar.gz "https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases/download/v15.2.0-1/${XPACK}-darwin-arm64.tar.gz"
  tar xzf xpack-gcc.tar.gz && rm xpack-gcc.tar.gz
}
echo "vendor tree ready; build per PINS.md recipes"
