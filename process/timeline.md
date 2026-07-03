# Timeline

Dated, ordered, descriptive. Pass/fail claims are only restated here after the harness certified them; the certifying artifact is named in each entry.

## 2026-07-03 (day 0)

- **~11:57** Repo scaffolded: cargo workspace (rvemu-core with the `Platform` I/O trait from the first commit, rvemu-cli), `harness/`, `targets/`, charter recorded in `docs/CHARTER.md`. First commit `5b2b855`.
- **~12:0x** Vendored and pinned: Spike `55b4658`, riscv-tests `34e6b6d`, riscv-arch-test `3.9.1`, RISCOF `1.25.3`, xPack riscv-none-elf-gcc `15.2.0-1` (Homebrew's riscv64-elf-gcc lacks newlib — first build attempt failed, switched toolchains). Spike built from source; riscv-tests built (all six charter groups incl. `-v` variants).
- **~12:2x** Harness authored: layer-1 runner, `lockstep-diff` comparator (Rust, Spike commit-log parsing built in, counter-CSR exclusion list fixed at authoring time), `lockstep.sh` FIFO driver, RISCOF config with rvemu-DUT/Spike-reference plugins, boot-layer runner. Canonical trace format calibrated against real Spike output (trap entries produce no commit line; mret/sret do log mstatus).
- **~12:3x** **Harness self-test: 7/7 checks passed** (`harness/selftest/run.sh` output, committed run): comparator catches a single wrong instruction at its exact index; truncation detected; FIFO plumbing verified; layer-1 counting verified against the reference executor; two corrupted binaries reported FAIL.
- **~12:4x–13:0x** Targets assembled: xv6 pinned `1982fd1` and built rv64imac soft-float; Linux v6.12 + BusyBox 1.36.1 + OpenSBI 1.6 recipe in Docker. Both needed multiple evidence-driven fixes — see `failures.md`.
- **~12:5x–13:3x** Gate A emulator core written (full rv64imac_zicsr_zifencei interpreter, M/S/U CSRs, traps/interrupts, CLINT with Spike's instret-derived mtime, triggers, HTIF, ELF loader, Spike-compatible reset ROM, canonical tracing). First harness runs: rv64ui 53/54-p on the first full run; misaligned-access semantics corrected to match the reference (see `failures.md`, ma_data entry).
- **~13:3x** **Planted-bug check passed**: a deliberately wrong `add` in a real rvemu build flagged by lockstep at instruction 80, opcode `00c58733` (`process/divergences.md`). Clean binary re-verified.
- **~13:4x–14:1x** Three real emulator bugs found by the harness and fixed (`divergences.md`); two budget-semantics bugs fixed. **RISCOF 136/136** (`work/riscof-console.log`, summarized in REPORTS.md): I 51, M 13, A 18, C 35, Zifencei 1, privilege 18.
- **~13:5x–14:3x** xv6 boots to shell on Spike and executes `ls` (golden transcript `harness/boot/xv6.transcript`); Linux boots to interactive BusyBox shell on Spike, `uname -a`/`echo` verified (`harness/boot/linux.transcript`).
- **~14:30** **Freeze**: `harness/` chmod'd read-only, target recipes locked, first REPORTS.md entry committed (`cfbfda8`). No blocked items.
- **~14:4x** Process instrumentation added (this directory) and backfilled from the day's evidence.
