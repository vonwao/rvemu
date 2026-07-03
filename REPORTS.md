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
