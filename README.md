# rvemu — RISC-V rv64imac instruction-set simulator

A RISC-V CPU emulator in Rust built to boot real operating systems (xv6-riscv, then Linux). Correctness is verified against the official `riscv-tests`, the RISCOF architectural compliance suite, and instruction-lockstep comparison with Spike, the official reference simulator. See `docs/CHARTER.md` for the full project spec and milestone ladder, and `REPORTS.md` for machine-derived progress reports.

**Live demo:** https://vonwao.github.io/rvemu/ — boots Linux in a browser tab and runs `tetris`, `doom`, a graphical web browser (`browse`), and networking (`wget`/`curl` against the real internet through an in-page gateway). See `docs/TESTING.md` to run and verify it locally, and `docs/FUTURE-WORK.md` for the roadmap (apk package support, instant-boot snapshots, more).

## Layout

- `crates/rvemu-core` — the emulator core: decoder, interpreter, CSRs, traps, Sv39 MMU, devices (CLINT, PLIC, 16550 UART). No direct host I/O; everything goes through the `Platform` trait so the core also compiles to WebAssembly.
- `crates/rvemu-cli` — native binary (`rvemu`).
- `harness/` — the verification harness. **Frozen after day 0: never edited to make a milestone pass.** Four layers: riscv-tests runner, RISCOF, Spike lockstep comparison, boot expect-scripts.
- `targets/` — vendored, version-pinned test suites, reference simulator, and OS images, with exact build recipes. Read-only after day 0.

## Fixed technical decisions

Interpreter only (no JIT), single hart, `rv64imac_zicsr_zifencei`, Sv39, soft-float OS targets, no networking or block device (initramfs only).
