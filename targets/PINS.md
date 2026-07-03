# Pinned inputs (frozen day 0 — 2026-07-03)

All external suites, the reference simulator, and OS targets are pinned to the exact versions below. Clones live in `targets/vendor/` (gitignored; reproducible via `targets/fetch.sh`). These pins are frozen: they are never moved to make a milestone pass.

## Test suites and reference simulator

| Component | Source | Pin |
|---|---|---|
| Spike (riscv-isa-sim) | https://github.com/riscv-software-src/riscv-isa-sim | `55b4658dbf574ba0b714083ec436ce2cb5be1998` |
| riscv-tests | https://github.com/riscv-software-src/riscv-tests | `34e6b6d1e7936b526075432fb730d89148623484` (env submodule `6de71edb142be36319e380ce782c3d1830c65d68`) |
| riscv-arch-test | https://github.com/riscv-non-isa/riscv-arch-test | tag `3.9.1` |
| RISCOF | PyPI, installed in `targets/vendor/riscof-venv` | `1.25.3` |

## Toolchain

| Tool | Version |
|---|---|
| rustc / cargo | 1.93.1 |
| RISC-V GCC (bare-metal, with newlib) | xPack `riscv-none-elf-gcc` 15.2.0-1 (darwin-arm64), in `targets/vendor/xpack-riscv-none-elf-gcc-15.2.0-1` |
| RISC-V GCC (headerless, unused for tests) | Homebrew `riscv64-elf-gcc` 16.1.0 — installed but lacks newlib; kept only as objdump/binutils source |
| dtc (device-tree compiler, Spike dependency) | 1.8.1 (Homebrew) |
| Host compiler | Apple clang (CommandLineTools), macOS Darwin 24.6.0 |

## OS targets

(filled in at §8 step 5 — xv6-riscv commit and Linux/BusyBox versions + build recipes)

## Build recipes

### Spike
```
cd targets/vendor/spike && mkdir -p build && cd build
../configure --prefix=$PWD/../install
make -j8 && make install
```

### riscv-tests
```
cd targets/vendor/riscv-tests
autoconf && ./configure
PATH=$PWD/../xpack-riscv-none-elf-gcc-15.2.0-1/bin:$PATH make RISCV_PREFIX=riscv-none-elf- -j8 isa
```
