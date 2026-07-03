# RISC-V Emulator — Autonomous Build Charter

**This is the day-0 spec the project runs under. The agent runs autonomously to each gate, reporting at checkpoints.**

---

## 0. Project framing and the one rule above all others

This is a **computer-architecture and systems-programming education project**: a RISC-V instruction-set simulator — a CPU emulator — in Rust, whose goal is to execute the RISC-V instruction set correctly enough to run an operating-system kernel. Correctness is measured against the official RISC-V test suites and the reference simulator (Spike).

The agent runs largely unsupervised for hours to days. The human reads progress reports at milestone checkpoints and otherwise stays out of the loop.

**The single unbreakable rule: the verification harness is never written, weakened, or modified to make a milestone pass. It is frozen on day 0 and lives in a read-only directory. If a test fails, fix the emulator, never the test.** Any desire to adjust the reference comparison, the test set, or a pass threshold must be logged as a blocked item in the report instead. The harness is the supervisor; defeating it is the one way to fail this project outright.

Build the harness BEFORE the emulator. Day one is the reference-comparison harness, frozen. Only then comes emulator logic.

---

## 1. What the emulator does

It reads a program image (an OS kernel or a test binary), executes RISC-V machine instructions one at a time, and models enough of the hardware for a kernel to run: the register file, the three execution modes (machine / supervisor / user), the control-and-status registers, trap and interrupt handling, address translation, and a small set of peripheral devices. Console output goes over a modeled UART.

---

## 2. Technology decisions (fixed — do not deviate)

- **Language: Rust**, edition 2021, release builds for any timed or boot run.
- **Target instruction set: `rv64imac` + `Zicsr` + `Zifencei`.** Decode C from the start — do not build a 32-bit-only decoder and retrofit.
- **Address translation: Sv39** (three-level page tables). Not Sv48.
- **Execution: a straightforward interpreter loop.** No JIT, no dynamic recompilation.
- **Single hart.** No multi-core.
- **I/O is abstracted behind a trait from the first commit** (console in/out, timer, program image loading) so the same core compiles to both a native binary and a WebAssembly target later with no logic changes.
- **No emulator crates.** `clap`, `anyhow`/`thiserror`, and bit-manipulation helpers are fine. Instruction semantics, trap logic, and translation are all original.

---

## 3. The verification harness (build first, then freeze)

`harness/` is read-only once working. Four layers:

1. **Unit ISA tests — `riscv-tests`** (pinned commit). Small self-checking binaries per instruction group (`rv64ui`, `rv64um`, `rv64ua`, `rv64uc`, `rv64mi`, `rv64si`). Harness reports X/Y passing per group.
2. **Architectural compliance — RISCOF** (pinned). Runs a conformance test set on the emulator and the reference model and compares execution signatures. Green RISCOF for `rv64imac` is the authoritative statement that instruction semantics match the spec.
3. **Lockstep reference comparison — Spike** (pinned commit). Runs Spike and the emulator on the same program one instruction at a time, comparing architectural state (all registers, PC, relevant CSRs) after each step. The first diverging instruction is the bug.
4. **Boot conformance — expect-scripts.** For each OS target, a scripted expected console transcript with exact text matching.

All four layers are external, pinned, and frozen. Glue is written once, verified (§8 step 4), and never touched again.

---

## 4. Frozen inputs (assembled and pinned on day 0)

Under `targets/` (read-only after day 0), each pinned to an exact version, toolchain versions recorded: `riscv-tests`, RISCOF + its `rv64imac` test set, Spike, xv6-riscv (pinned commit, pinned RISC-V GCC toolchain, recorded build recipe), and a Linux + BusyBox initramfs image (pinned kernel version, soft-float, `defconfig`-based, single-core, minimal BusyBox initramfs baked in; pinned `.config` and BusyBox build, recorded recipe).

If any source is unreachable from the sandbox, log it as a blocked item and proceed with whatever can be fetched — but freeze whatever is used.

---

## 5. Milestone ladder

Report at every gate. A gate passes only when the harness says so.

**Gate A — Base camp.** Machine-mode-only emulator, no address translation. Executes `rv64imac` + `Zicsr`, handles traps/exceptions and timer interrupts (CLINT). Passing: `rv64ui`, `rv64um`, `rv64ua`, `rv64uc`, `rv64mi` green; RISCOF green for base and M/A/C.

**Gate B — Runs a real OS.** Add supervisor mode, Sv39 MMU, PLIC, 16550 UART. Full RISCOF `rv64imac` green, `rv64si` green. **xv6-riscv boots to its shell prompt and runs a scripted command sequence with expected output.**

**Gate C — Mainline Linux (stretch; each rung a defensible stop).**
- **C1** — Linux prints its banner and reaches filesystem mount, lockstep-clean to that point.
- **C2** — Linux boots to a BusyBox shell and runs a fixed command sequence with expected output.
- **C3 (trophy)** — the emulator compiled to WebAssembly, booting the kernel in a browser tab.

Never trade correctness for a Gate C rung. A faster or further boot that fails lockstep or RISCOF is worth nothing.

---

## 6. Non-goals

No JIT. No RV32, no F/D (targets are soft-float). No SMP. No networking / virtio-net. No block device (initramfs only; virtio-block is an optional post-C3 extra). No performance target. No GDB stub (lockstep replaces it). No config-file system, plugin architecture, or polished CLI.

---

## 7. Progress report protocol

At each gate — and at most once per ~2 hours otherwise — append to `REPORTS.md`, machine-derived facts only: current gate; pass/fail per harness layer; riscv-tests X/Y per group with failing test names; RISCOF pass/fail per extension; lockstep first-divergence details when a boot fails; boot console tail vs. expected; what changed since last report; **blocked items** (the only channel for anything needing a human decision).

No estimates, no narration. Harness numbers only.

---

## 8. First actions (in order)

1. Scaffold the Rust project and `harness/`, with the I/O trait in place from the start.
2. Vendor and pin Spike, `riscv-tests`, and RISCOF; build them; confirm each runs.
3. Wire the lockstep comparison harness against Spike and the `riscv-tests` runner.
4. **Prove the harness catches errors before trusting it:** a deliberately wrong one-instruction emulator stub must be flagged at the exact bad instruction, and a corrupted test binary must be reported as FAIL.
5. Assemble and pin `targets/` (xv6 and the soft-float Linux+BusyBox image); record build recipes.
6. Freeze `harness/` and `targets/`. Write the first report confirming the harness is live and self-tested.
7. Begin Gate A.
