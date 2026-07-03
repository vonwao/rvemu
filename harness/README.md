# Verification harness — FROZEN

This directory is the project's supervisor. Per the charter, once verified by its self-test (`selftest/`), nothing here is edited, weakened, or re-tuned — ever. If a milestone can't go green, the emulator gets fixed, or the item is logged as blocked in `REPORTS.md`. The freeze is enforced by `chmod -R a-w` and by this contract.

Everything here reports machine-derived facts: pass counts, first-divergence coordinates, transcript diffs. No judgment calls, no thresholds to tune.

## The emulator CLI contract (what `rvemu` must implement — the harness depends on it and will not adapt)

- `rvemu <elf>` — load an ELF (physical addressing, RAM at `0x8000_0000`, entry from the ELF header) and run. If the ELF has a `tohost` symbol, treat a nonzero write per the HTIF test convention: value `1` → exit code 0 (PASS); value `2n+1` → exit code 1, printing `FAIL test <n>` to stderr. If the program never writes `tohost` within `--max-insns`, exit code 2 (TIMEOUT).
- `--max-insns <n>` — hard instruction budget, exit 2 when exhausted.
- `--trace <file>` — write one line per retired instruction in the normalized trace format below (`-` for stdout is not used; always a file or FIFO path).
- `--signature <file> --signature-granularity 4` — RISCOF signature dump: on test exit, write the memory between symbols `begin_signature`/`end_signature` as little-endian 4-byte hex words, one per line.
- Machine reset state: `pc` = ELF entry, `x0..x31` = 0, `mstatus` per spec reset, hart 0, `mhartid`=0.
- Memory map (fixed, qemu-virt-compatible): CLINT `0x0200_0000`, PLIC `0x0c00_0000`, UART0 (16550) `0x1000_0000`, RAM `0x8000_0000` (default 256 MiB, `--ram-mib <n>` to change).
- CLINT `mtime` advances by 1 per 100 retired instructions (Spike's default `CPU_HZ/INSNS_PER_RTC_TICK` behavior), so timer interrupts land deterministically and identically to the pinned Spike.
- Console I/O goes through the UART; `--console-file <file>` mirrors UART output to a file for the boot layer.

## Normalized trace format (one line per retired instruction)

```
<pc-hex-16> <insn-hex-8> [xN=<hex-16>]... [f:<csrname>=<hex-16>]...
```

- `insn` is the raw fetched instruction bits (compressed instructions: 4 hex digits from the 16-bit parcel, zero-extended to 8 digits).
- `xN=` entries are integer register writebacks committed by this instruction, in ascending register order. Writes of the same value still appear (the write happened). `x0` writes never appear.
- `f:<csrname>=` entries are CSR writebacks, ascending alphabetical, using Spike's CSR names. The comparator ignores the counter CSRs (`cycle`, `time`, `instret`, `mcycle`, `minstret`, `mhpm*`) — a fixed, frozen exclusion list, because reference and emulator legitimately differ there.
- Trap entry is visible as the natural CSR writes (`mepc`/`mcause`/... or `sepc`/`scause`/...) attributed to the instruction that trapped, followed by the next line's pc at the trap vector. No special trap records.

`spike_trace_adapter.py` converts Spike's `--log-commits` output into this format; the comparator never parses Spike's format directly.

## Layers

1. `run-riscv-tests.sh [group...]` — runs every `targets/vendor/riscv-tests/isa/<group>-{p,v}-*` binary under `rvemu`, prints `<group>: X/Y` and each failing test name, exit 0 iff all pass. Groups default to `rv64ui rv64um rv64ua rv64uc rv64mi rv64si`.
2. `riscof/` — RISCOF config with `rvemu` as DUT plugin and the pinned Spike as reference. `run-riscof.sh` runs the rv64imac suite and prints per-extension pass/fail.
3. `lockstep/` — `lockstep.sh <elf> [max-insns]` runs the pinned Spike and `rvemu` on the same ELF, converts both traces to the normalized format, and streams them through `lockstep-diff` (Rust, built once from `lockstep/diff/`), which maintains full shadow register state per side and reports the first divergence: instruction index, pc, opcode, and the exact state difference, with the 8 preceding instructions as context. Exit 0 = no divergence to program end / budget.
4. `boot/` — per-OS expect scripts: `boot/<target>.expect` drives `rvemu`, requires exact prompt/output matches per the pinned transcript in `boot/<target>.transcript`. Exit 0 iff the full scripted sequence matches.

## Self-test (`selftest/run.sh`)

Proves the harness catches bugs before it is trusted: (1) a corrupted riscv-tests binary must be reported FAIL by layer 1; (2) an emulator with a deliberately wrong instruction (a one-off patched build that computes `add` incorrectly for one operand pattern) must be flagged by `lockstep-diff` at exactly the patched instruction. The self-test log is committed as `selftest/RESULT.md`.
