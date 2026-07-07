# Progress reports

Machine-derived facts only, per charter §7. Newest entry last.

---

## 2026-07-03 — Day 0 complete: harness live, self-tested, frozen; targets assembled and verified on reference; Gate A layers 1+2 already green

**Current gate:** finishing Gate A (M-mode emulator built and passing; lockstep spot-checks green; remaining Gate A exit criteria all met except formal re-run at freeze point, below).

**Harness self-test (charter §8 step 4)** — `harness/selftest/run.sh`: 7/7 checks passed: identical traces OK; single wrong instruction flagged at exact index 40; truncated trace flagged as early end; lockstep.sh FIFO plumbing verified end-to-end (identical and corrupted replays); layer-1 runner counted rv64um 26/26 via the reference executor; two corrupted binaries (bit-flip in .text, truncated ELF) both reported FAIL. Additionally, per the charter's literal step-4 requirement, a real rvemu build with a deliberately planted `add` off-by-one was run under lockstep: **DIVERGENCE reported at instruction 80, opcode 00c58733 (the first `add` executed), dut value 0x1 vs reference 0x0** — the planted bug's exact location. The unpatched binary was rebuilt and re-verified afterwards.

**Layer 1 — riscv-tests** (X/Y per group; `-p` = physical/M-mode variants, `-v` = virtual-memory variants requiring the Gate B MMU by design):
- rv64ui: 53/54 -p (54/108 with -v). FAIL: `rv64ui-p-ma_data` — **fails identically on the pinned Spike itself** (`spike rv64ui-p-ma_data` → `*** FAILED *** (tohost = 668)`, same failing test number as rvemu; the test requires hardware misaligned-access support absent in the pinned reference build). rvemu matches the reference behavior (misaligned loads/stores trap), which the RISCOF privilege misalign tests require.
- rv64um: 13/13 -p. rv64ua: 19/19 -p. rv64uc: 1/1 -p. rv64mi: 17/17. rv64si: 5/7 (`dirty`, `icache-alias` need Sv39 — Gate B).
**vs Gate A definition (rv64ui/um/ua/uc/mi green):** green for the M-mode (-p) variants Gate A targets, with the one reference-identical exception above.

**Layer 2 — RISCOF (rv64imac vs pinned Spike): 136/136 signatures match.** Per extension: I 51/51, M 13/13, A 18/18, C 35/35, Zifencei 1/1, privilege 18/18. PASS.

**Layer 3 — lockstep:** live and in daily use; found and pinpointed three real rvemu bugs during Gate A (load-trace token order; missing mret/sret mstatus commit annotation; c.lui-x0 HINT treated as illegal). Exit-code semantics as documented in harness/README.md.

**Layer 4 — boot conformance:** expect scripts + golden transcripts recorded from the pinned Spike:
- xv6: boots to `$ ` prompt on Spike; scripted `ls` produces the full 23-entry listing; transcript pinned in `harness/boot/xv6.transcript`.
- Linux: boots to BusyBox `~ #` prompt on Spike; `uname -a` → `Linux (none) 6.12.0 #1 Fri Jul  3 18:18:10 UTC 2026 riscv64 GNU/Linux`; `echo boothello` → `boothello`; transcript pinned in `harness/boot/linux.transcript`.

**Targets assembled (charter §8 step 5)** — all pinned in `targets/PINS.md`:
- xv6-riscv `1982fd1` built rv64imac soft-float with recorded adaptations to the pinned Spike platform (each found by evidence, not guesswork): embedded-ramdisk fs (charter forbids a block device), UART0_IRQ 10→1 (Spike's PLIC wiring), PTE_A|PTE_D at mapping time (Spike implements Svade: silent trap-loop at paging-on otherwise), synchronous UART TX with RX-only IER (Spike's level-triggered THR-empty interrupt otherwise livelocks xv6, and with TX interrupts off, this xv6's tx_busy handshake sleeps forever).
- Linux v6.12 (GitHub mirror tag; kernel.org CDN unreachable from this network — recorded, not blocked) + BusyBox 1.36.1 static soft-float via a musl-cross-make rv64imac/lp64 toolchain (Debian's riscv64 cross-gcc is rv64gc hard-float; its BusyBox died on the first FP instruction) + OpenSBI 1.6 fw_payload with pinned DTB (Sv39 forced via `no4lvl no5lvl`), initramfs from a cpio file-list carrying `/dev/console`+`/dev/ttyS0` nodes.

**What changed since project start:** everything (first report). Emulator core exists: full rv64imac_zicsr_zifencei decode/execute incl. C-expansion, M/S/U CSRs, traps/interrupts, CLINT with Spike's instret-derived mtime, triggers (tselect/mcontrol), HTIF, ELF loading, Spike-compatible reset ROM, canonical trace emission. Three emulator bugs found by the harness and fixed (above), plus two budget-semantics bugs (writable minstret defeating the run budget; trap loops retiring nothing) — budget now bounds steps, matching Spike's `--instructions`.

**Freeze:** `harness/` and `targets/` recipes are frozen as of this report (chmod -R a-w on harness/; pins recorded in targets/PINS.md). Nothing in either was weakened to obtain any number above; every FAIL listed is reported as-is.

**Blocked items:** none. (Two decisions taken autonomously and flagged for review rather than blocking: (1) `rv64ui-p-ma_data` counted as reference-identical-fail, since the pinned Spike fails it the same way and RISCOF's misalign tests require trap behavior; (2) the xv6 target carries the four Spike-platform adaptations listed above — all are target-recipe changes recorded pre-freeze, not harness changes.)

---

## 2026-07-03 — GATE A REPORT (formal)

**Gate A criteria:** machine-mode-only emulator, no address translation; executes rv64imac+Zicsr; traps/exceptions and timer interrupts via CLINT; rv64ui/um/ua/uc/mi green; RISCOF green for base and M/A/C.

**Layer 1 — riscv-tests (fresh run at this report):**
- rv64ui: **53/54 -p**. FAIL `rv64ui-p-ma_data` — fails identically on the pinned Spike (`tohost=668` both); reference-identical, documented in process/failures.md, still executed and counted every run.
- rv64um: **13/13 -p**. rv64ua: **19/19 -p**. rv64uc: **1/1 -p**. rv64mi: **17/17**.
- All 54 `-v` variants fail as expected: they require the Sv39 MMU, which Gate A explicitly excludes ("no address translation yet"). They are Gate B exit criteria.
- rv64si: 5/7 (`dirty`, `icache-alias` need Sv39 — Gate B).

**Layer 2 — RISCOF: 136/136 PASS** (I 51, M 13, A 18, C 35, Zifencei 1, privilege 18) — rvemu signatures match the pinned Spike on the full rv64imac_zicsr_zifencei suite.

**Layer 3 — lockstep:** clean on spot-checked binaries (rv64ui-p-add end-to-end; clui-01 post-fix); three divergences found and fixed during the gate, logged with opcode-level evidence in process/divergences.md.

**Layer 4 — boot:** n/a at Gate A (Gate B/C criteria); golden transcripts already pinned from the reference.

**CLINT/timer status:** implemented (msip/mtimecmp/mtime, mtime = 1 tick per 100 retired instructions matching Spike; mip.MTIP/MSIP composition; Sstc stimecmp for the OS targets). Machine-timer delivery is exercised implicitly by rv64mi and will be exercised hard by xv6/Linux under lockstep in Gate B.

**Gate A verdict: PASS** with the one reference-identical exception named above.

**Process files updated this period:** `process/divergences.md` (+6 entries: planted bug, 3 lockstep-found bugs, 2 budget bugs), `process/failures.md` (+5 sections incl. the ma_data temptation entry), `process/timeline.md` (day-0 backfill). Repo pushed to https://github.com/vonwao/rvemu (private) with full history.

**Blocked items:** none.

**Next:** Gate B — Sv39 translation with Spike-matching Svade semantics, PLIC, 16550 UART, MPRV/SUM/MXR access checks; exit on rv64si + all `-v` variants green, RISCOF still green, xv6 boot conformance + lockstep.

---

## 2026-07-03 — GATE B REPORT (formal)

**Gate B criteria:** supervisor mode, Sv39 MMU, PLIC, 16550 UART; full RISCOF rv64imac green; rv64si green; xv6-riscv boots to its shell prompt and runs a scripted command sequence with expected output.

**Layer 1 — riscv-tests** (full run on the final Gate B tree): rv64ui **106/108**, rv64um **26/26**, rv64ua **38/38**, rv64uc **2/2**, rv64mi **17/17**, rv64si **7/7**. The two failures are `ma_data` (-p and -v): the -p variant is confirmed reference-identical (pinned Spike fails with the same tohost code 668); the -v variant is the same test under the paging environment. All 54 `-v` virtual-memory variants otherwise pass — the Gate A deferral is closed.

**Layer 2 — RISCOF: 136/136 PASS** after the Gate B MMU/PLIC/UART work (run 7). One architectural change landed after that run (the medeleg WARL mask 0xb3ff→0xb3fe, itself a lockstep-certified move toward the reference); the two later changes are trace-emission and RTC-timing only. A confirming re-run is queued as a residual (environment is currently killing long-running jobs — see below).

**Layer 3 — lockstep vs pinned Spike on the xv6 boot: PREFIX-CLEAN over 423,107,530 instructions, zero divergences** (reference ended at its step budget). Coverage includes the Spike-compatible reset ROM, M-mode boot, delegation setup, Sv39 paging-on with Svade A/D semantics, S-mode kernel execution, user processes, syscalls, and timer-interrupt delivery with cycle-exact `time` CSR values. Three divergences were found and fixed on the way (all emulator-side; evidence in process/divergences.md): medeleg WARL bit 0 (instruction 33), sie/sip dual-token commit logging (instruction 37), and — after 410.8M identical instructions — Spike's RTC quantum model (mtime advances +50 per completed 5000-retired-instruction slice; traps, interrupt deliveries, and wfi consume the slice remainder; idle quanta bump minstret), reconstructed from the vendored Spike source and matched exactly.

**Layer 4 — boot conformance: BOOT-OK.** `harness/boot/run-boot.sh xv6` (frozen expect script, exact-match): banner → `init: starting sh` → prompt → `ls` with the exact 23-line pinned listing → `echo boothello` → prompt. Certified on the Gate B tree prior to the final timing-model commits; the 423M-instruction lockstep run on the final tree covers the identical boot path instruction-for-instruction.

**Gate B verdict: PASS.**

**Process files updated this period:** `process/divergences.md` +5 Gate B entries; `process/failures.md` +1 (the RTC-quantum wrong-models entry); `process/timeline.md` Gate B section.

**Residuals (not blocked items):** (1) re-run RISCOF and the boot layer on the exact final tree for belt-and-braces re-certification — queued (the transient command-killing issue is resolved; see process/failures.md); (2) ~~confirm v-ma_data on Spike~~ **closed with stronger evidence than planned**: a bounded lockstep run shows both simulators executing an identical 4,569-instruction prefix into the same infinite failure loop (the test's fail path, code 0x349, trap-loops in the v environment; uncapped Spike hangs on it forever). Reference-identical, instruction-for-instruction. Also recorded: capped Spike exits 0 on budget exhaustion — a footgun documented in process/failures.md; no harness layer relied on it.

**Blocked items:** none.

**Next:** Gate C — C1 (Linux banner + fs mount, lockstep-clean), C2 (BusyBox shell + scripted commands via the frozen linux.expect), C3 (wasm build in a browser).

---

## 2026-07-04 — GATE C REPORT (formal)

**C1 — Linux boots lockstep-clean: EXCEEDED.** Requirement: banner + filesystem mount without a wrong-instruction divergence. Result: **PREFIX-CLEAN over 317,547,717 instructions with zero divergences** (the pinned Spike ended at its 900M-step budget; Linux's trap/interrupt density means quantum-skips, so 900M reference steps ≈ 317.5M commits). Deterministic replay of rvemu to exactly that instruction count shows the console already at the BusyBox prompt — the clean region covers OpenSBI, the kernel banner, initramfs mount, init, and the shell. Ten divergences were found and fixed on the way (instruction 4 through 21.7M; all emulator-side; full chain with evidence in process/divergences.md).

**C2 — BusyBox shell with scripted commands: PASS.** Frozen boot layer `harness/boot/run-boot.sh linux` → **BOOT-OK**: OpenSBI banner, kernel banner, `initramfs: init complete`, `~ #` prompt, `uname -a` matching the pinned string exactly (`Linux (none) 6.12.0 #1 Fri Jul  3 18:18:10 UTC 2026 riscv64 GNU/Linux`), `echo boothello` → `boothello`.

**C3 — WebAssembly: substance PASS, browser click pending.** `crates/rvemu-wasm` (hand-rolled extern "C" exports; no new dependencies) compiled for wasm32-unknown-unknown boots the same pinned fw_payload.elf to the BusyBox shell and executes a typed command — verified by a Node harness driving the module through the identical JS interface the page uses (`WASM-SHELL-OK`, ~400M instructions retired). The browser demo is `web/index.html` + `web/serve.sh`; the literal "in a browser tab" confirmation is one click away for the operator.

**Regressions on the final tree:** riscv-tests rv64ui 106/108 (the reference-identical ma_data pair only), rv64um 26/26, rv64ua 38/38, rv64uc 2/2, rv64mi 17/17, rv64si 7/7. **RISCOF 136/136.**

**Charter milestone ladder status: Gate A PASS, Gate B PASS, C1 PASS, C2 PASS, C3 functionally complete pending the browser click.** Non-goals honored throughout: interpreter only, single hart, no F/D, no networking, no block device, harness untouched since freeze.

**Process files:** divergences.md +10-entry Gate C chain; timeline.md Gate C section; failures.md silent-patch-no-op repeat noted (edit scripts now assert anchors).

**Blocked items:** none.

---

## 2026-07-04 — Perf-change certification (translation cache + LTO)

A Spike-equivalent TLB (flushed on satp writes, sfence.vma, mstatus/sstatus writes, trap entries, mret/sret) and LTO were added for demo responsiveness (4 → 16 native MIPS). Full re-certification on the changed tree: riscv-tests 106/108 (ma_data pair only), RISCOF **136/136**, xv6 and Linux frozen boot layers both **BOOT-OK**, wasm node smoke **WASM-SHELL-OK**, and the Linux lockstep re-ran **PREFIX-CLEAN over the identical 317,547,717 instructions** — the cache is architecturally invisible, byte-for-byte.

---

## 2026-07-06 — Extras-change certification (virtio-net in core bus)

The extras networking step added a virtio-mmio net device to `rvemu-core` (opt-in `enable_net()`; only the wasm demo build calls it — certified targets, harness, CLI and lockstep paths never instantiate it) plus a JS user-mode gateway on the page. Because shared core files (`bus.rs`, new `virtio.rs`) changed, the battery re-ran on the changed tree: virtio ring unit tests 4/4, riscv-tests 106/108 (the reference-identical ma_data pair only), **RISCOF 136/136**, xv6 and Linux frozen boot layers **BOOT-OK**, demo verification **DEMO-VERIFIED** (Tetris paced + fb dark-at-boot/pixels-on-write), networking **NET-VERIFIED** (hermetic stubbed wget, length-checked; `--live` fetched this repo's README from real raw.githubusercontent.com from inside the guest), and a bounded 30M-instruction Linux lockstep vs the pinned Spike ran **PREFIX-CLEAN** (comparator rc=3, Spike budget exhausted first).

Extras non-determinism note: rx frame delivery timing is host-driven, so guest runs *with network traffic* are not replay-deterministic; certified images carry no NIC and are unaffected. The charter non-goal "no networking" remains honored on every certified path.

---

## 2026-07-06 — Extras-change certification (virtio-input in core bus)

virtio-input (evdev keyboard/mouse for the demo page) added behind opt-in `enable_input()`; certified paths never instantiate it. Battery on the changed tree: unit tests 5/5, riscv-tests 106/108 (reference-identical ma_data pair only), **RISCOF 136/136**, xv6 and Linux frozen boot layers **BOOT-OK**, **DEMO-VERIFIED**, **NET-VERIFIED** (incl. live fetch), **DOOM-VERIFIED** (device bound, events byte-exact in-guest, rich frame drawn), bounded 30M Linux lockstep **PREFIX-CLEAN**.

---

## 2026-07-06 — Extras image change (curl in demo userland)

curl 8.10.1 (pinned sha256) added to the demo image as a static musl binary, HTTP-only (`--without-ssl` — the page gateway terminates TLS; guest port 80 → real `fetch()`). No emulator code changed — image-only, certified targets/harness untouched. Verification on the rebuilt image: **NET-VERIFIED** with new **CURL-STUB-OK / CURL-STUB-LENGTH-OK** checks (hermetic stubbed download, exact 40,023-byte length match) plus the live README fetch; **DEMO-VERIFIED** and **DOOM-VERIFIED** re-run green on the new artifact (DOOM frame metrics identical to the v5 certification). Build fix worth recording: libtool swallows plain `-static`; the final link needs libtool's `-all-static` (first attempt produced a dynamic binary, caught by the layer's static-link assertion).

---

## 2026-07-06 — Extras-change certification (links2 + curl, userland only)

The graphical browser (links2 2.30 on the framebuffer, with pinned zlib/libpng/gpm) and curl 8.10.1 were added to the demo image. These are userland additions only: `git diff` over `crates/` is empty since the virtio-input commit (73ba0b7), so the emulator core binary is unchanged and the input build's **RISCOF 136/136** and bounded-lockstep **PREFIX-CLEAN** certification carries without re-running. Re-run on this image: unit tests 5/5, riscv-tests 106/108 (reference-identical ma_data pair only), xv6 + Linux frozen boot layers **BOOT-OK**, **DEMO-VERIFIED**, **DOOM-VERIFIED**, **NET-VERIFIED** (incl. curl arm + live README fetch), **LINKS-VERIFIED** (page fetched through the JS gateway and rendered to the framebuffer: 928,437 nonzero bytes). Charter non-goal "no networking" remains honored on every certified path; the NIC/input/fb devices are opt-in and wasm-only.
