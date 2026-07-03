# Failures, wrong turns, and temptations declined

The unflattering record. Everything here is as it happened; nothing was smoothed over afterwards.

## 2026-07-03 — xv6 would not boot on the reference: four separate root causes

xv6 (pinned upstream HEAD) printed its banner on Spike and then went silent. Getting it to a shell took most of day 0 and four distinct target-recipe adaptations. The sequence, including the misses:

1. **First miss:** ran Spike with an ISA string missing `_zicntr`; xv6's `r_time()` trapped with mtvec=0 into a silent trap loop *before* the banner. Burned time on a wrong theory before spotting the commit-log ending at the mcounteren write.
2. **Wrong-theory detour:** a debug-mode sample at 1.5B instructions showed pc=0x1000 (the reset vector) and I chased a "kernel restarts itself" theory for a while, including misreading two concatenated log views as evidence of a double boot. A later `until pc==0x1000` probe never triggered — the sighting was almost certainly a user-mode pc or sampling artifact. Lesson recorded: concatenated tool output is not evidence.
3. **Real cause #1 — Svade:** the stall was at paging-on. Spike faults on access to PTEs with A=0 (or stores with D=0); qemu (xv6's home platform) updates A/D in hardware. With `stvec` still unset at that point in `main()`, the fault became a silent trap loop. Proven by patching PTE_A|PTE_D into `mappages` on a scratch copy: boot advanced through paging/plic/userinit markers. Adopted into the recipe.
4. **Real cause #2 — UART interrupt storm:** the first S-mode trap after `intr_on()` was SEIP, not the long-pending timer. Spike's ns16550 asserts a *level-triggered* THR-empty interrupt (`(ier & THRI) && (lsr & TEMT)` in the vendored source); xv6 never clears it (tuned to qemu's edge behavior) → interrupt livelock. Fix: RX-only IER + synchronous TX.
5. **Self-inflicted regression:** the first version of the synchronous-TX patch targeted an older xv6 uart.c (`uartputc`/buffer) and silently didn't apply — this revision uses a `tx_busy`/`tx_chan` handshake in `uartwrite`. Result: with TX interrupts disabled, the *second* console character slept forever on `tx_chan` (found by a `sleep chan=` marker resolving to that symbol). The patch was rewritten against the actual source. Lesson: verify a text patch matched, don't assume.
6. Also fixed en route: `UART0_IRQ` 10→1 (Spike's PLIC wiring differs from qemu-virt) and the embedded-ramdisk fs (the charter forbids a block device; xv6 normally requires virtio-blk).

Diagnosis method throughout: marker printks in a scratch copy of the target (never the pinned tree), one bisection layer per run, on the reference simulator.

## 2026-07-03 — Linux init died twice before reaching a shell

1. **Hard-float userland:** first image booted the kernel fine, then init died with SIGILL (`cause: 2` at a compressed FP opcode). Debian's `gcc-riscv64-linux-gnu` targets rv64gc/lp64d; a "static" BusyBox from it still contains FP instructions. Fix: build a dedicated rv64imac/lp64 musl toolchain (musl-cross-make, pinned commit) inside the Docker recipe — which itself took three attempts (old GCC rejecting `_zicsr` in `-march`; shared-libgcc `R_RISCV_JAL` link failure → `--disable-shared`; missing kernel uapi headers → installed from the pinned kernel tree). A guard step now objdumps the final BusyBox and fails the build if any FP instruction is present.
2. **Missing /dev/console:** second image's init exited(1) — the initramfs had no console/ttyS0 device nodes and devtmpfs isn't automounted for initramfs. Fix: cpio file-list initramfs (declares the nodes without root) + devtmpfs mount in init.
3. **Misleading guard:** my own FP-check step once reported "FP instructions found" when the actual failure was BusyBox's `tc` applet failing to compile against 6.12 headers (CBQ removal). The check was restructured into its own build step so failures can't be mislabeled. `tc` is disabled (no networking in scope anyway).

## 2026-07-03 — ma_data: the temptation entry

`rv64ui-p-ma_data` fails under rvemu (tohost=0x539, test 668). Early in Gate A, I "fixed" it by making rvemu handle misaligned accesses in hardware — which made the test pass and instantly broke all 8 RISCOF privilege misalign tests, because the pinned Spike *traps* on misaligned accesses and RISCOF compares signatures against Spike. The correct resolution was to restore trap behavior and then check the premise: **Spike itself fails ma_data with the identical tohost code (668)**. The test requires hardware misaligned support the reference build doesn't have. It is reported as a reference-identical failure in REPORTS.md — not excluded from the runner, not special-cased in the harness — so the rv64ui line reads 53/54 rather than a clean sweep. The runner still executes it every run and will still count it as FAIL.

## 2026-07-03 — Infrastructure friction (recorded so the wall-clock cost is honest)

- kernel.org's CDN 403/404s from this network (even the URL its own releases.json advertises); the kernel pin moved to the GitHub mirror tag `v6.12`.
- Spike's `--log-commits` through a FIFO into a 56GB tail pipeline was abandoned as a diagnosis tool (too slow under CPU contention); debug-mode `rs`+CSR dumps and marker builds were faster.
- Multiple background-job pipelines ate their own output (`grep | head` buffering, concatenated views); switched to plain files.

## 2026-07-03 (Gate B) — Two hours inside Spike's timekeeping

The 21-tick `time` skew at lockstep instruction 410,787,976 produced two wrong models before the right one: (1) mtime = retired/100 — refuted by the same read being 50 low; (2) a constant startup offset — refuted by xv6's *first* time read (instruction ~40) having matched at mtime 0. The correct model (quantum slices; traps consume the remainder) came from reading sim.cc's step loop and execute.cc's catch(trap_t) `n = instret` early-exit, not from guessing. Recorded because the temptation at each wrong model was to "just nudge the tick rate until the number matches" — which would have fit one read and diverged on the next. Nothing in the harness or its counter-CSR exclusion list was touched; `time` reads remain fully compared.
