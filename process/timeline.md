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

## 2026-07-03 (day 0, evening) — Gate B

- **~14:45** Process instrumentation created and backfilled; repo pushed to github.com/vonwao/rvemu; formal Gate A report committed (`4df2a9a`).
- **~15:0x** Gate B implementation: Sv39 translation (Svade, MPRV/SUM/MXR), PLIC and ns16550 modeled register-for-register on the vendored Spike sources (including the first-enabled-context PLIC quirk and the level-triggered THRE behavior), console plumbing through the Platform trait.
- **~15:2x** riscv-tests: rv64si 7/7 (dirty and icache-alias pass with Svade); -v variants driven green through four lockstep/RISCOF-diagnosed fixes (mstatus.FS writable, tohost physical-address match, plus Gate A trace conventions). Full suite: **106/108** — the two ma_data variants only (see failures.md). Certified by `harness/run-riscv-tests.sh` after the trap-quantum commit (`d0f07e7` tree).
- **~15:3x** **xv6 boots to its shell on rvemu on the first console-wired attempt**; the frozen boot layer certifies it: `harness/boot/run-boot.sh xv6` → **BOOT-OK**, full scripted sequence (banner, init, prompt, exact 23-line `ls`, `echo boothello`). RISCOF regression after the Gate B MMU/device work: **136/136**.
- **~15:4x–16:5x** Lockstep vs Spike on the xv6 boot: three architectural divergences found and fixed (medeleg WARL mask, sie/sip dual-token logging, the RTC quantum model — see divergences.md). Final run: **prefix-clean over 423,107,530 instructions**, zero divergences to the reference's budget end (`boh8yg4ww` run, tree `d0f07e7`+quantum-trap fix).

## 2026-07-03/04 (night) — Gate C

- **~17:5x** Gate C start. fw_payload PIE load-offset fixed (divergence at lockstep instruction 4); **Linux 6.12 boots to the BusyBox shell on rvemu**; frozen boot layer: `run-boot.sh linux` → **BOOT-OK** (exact `uname -a` and scripted echo).
- **~18:0x** **C3: wasm build** (hand-rolled extern "C" exports, no new dependencies) boots the same fw_payload to the shell and executes a typed command under Node driving the module exactly as the browser page does — `WASM-SHELL-OK`, ~400M instructions. Browser page at `web/index.html` (+`web/serve.sh`).
- **~18:0x–00:1x** Lockstep hunt through OpenSBI+Linux: ten divergences fixed (see divergences.md Gate C). Final: **prefix-clean 317,547,717 instructions, zero divergences**, with deterministic replay showing the shell prompt inside the clean region. riscv-tests regression: 106/108 (ma_data pair only). RISCOF regression re-running post-changes.

## 2026-07-04 (early) — Browser-demo responsiveness

- Operator confirmed **C3 in the literal sense: Linux booted in a browser tab** via web/serve.sh — but typing was unusable. Two causes: the page ran 30M-instruction chunks (keystrokes only enter between wasm calls), and translation had no cache (a full 3-level walk per fetch).
- Fixes: adaptive run-chunking targeting ~40ms per call, and a Spike-equivalent TLB (flushed on satp/sfence.vma/mstatus writes, trap entries, xret) + LTO. Native speed 4 → 16 MIPS.
- **Re-certification of the changed tree, all green:** riscv-tests 106/108 (ma_data pair only), RISCOF 136/136, both frozen boot layers BOOT-OK, wasm smoke WASM-SHELL-OK, and the Linux lockstep PREFIX-CLEAN over the identical 317,547,717 instructions. Operator confirmed the shell is now responsive.

## 2026-07-04 — Terminal + public demo

- **VT100 terminal** (`web/term.js`, no dependencies: cursor addressing, erase, insert/delete line/char, SGR colors/inverse, scroll regions, cursor-position report) replaced the dumb `<pre>`. Verified headlessly under Node driving the real wasm emulator: vi created and saved a file, `cat` confirmed the content, top rendered its table — **TERMINAL-VERIFIED**.
- Repo made **public** per operator instruction; demo published to **GitHub Pages from the `gh-pages` branch** (index.html, term.js, wasm build, pinned fw_payload.elf; `.nojekyll` added after the Jekyll-processed first build stalled on the 21MB image).
- Final proof: the published artifacts, downloaded back from https://vonwao.github.io/rvemu/, boot Linux to the BusyBox prompt under Node — **LIVE-ARTIFACTS-BOOT-OK**.
