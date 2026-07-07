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

## 2026-07-05 — Extras step 1: time pacing + Tetris, live

- **extras/ track created** with its governance README (certified targets and harness untouched; extras get separate pinned recipes and their own verification).
- **Time pacing**: read-only `mtime` export from the wasm; the page caps the guest RTC at wall time (with bounded catch-up credit), so timing-based programs run at human speed and an idle tab no longer burns a core. Diagnosed en route: without pacing, guest select() waits fast-forward — Tetris played itself to game-over (deterministically scoring 12, twice: the guest clock seeds its RNG identically each run).
- **linux-demo image**: Docker layer on the pinned base — Micro Tetris (troglobit/tetris `aafa95e`) static rv64imac/musl, tty size set at init (serial consoles report 0×0; Tetris refused to start until `stty rows 24 cols 80`).
- **Verification** (`extras/verify-demo.mjs`, paced like the page): boot → prompt → binary present → Tetris running fullscreen mid-game → `q` back to prompt — **DEMO-VERIFIED**.
- Published: gh-pages now serves the pacing page + demo image; served ELF sha256 matches the locally verified build (`ffbfa918…`).

## 2026-07-05 — Extras step 2: framebuffer + real pacing, live

- **Pacing root-caused properly**: page-side sleeps couldn't work — one wasm call fast-forwards through many WFI waits internally, so whole gravity intervals passed inside a single call (operator confirmed Tetris unplayably fast; the unpaced verifier had shown the same signature — instant game-over). Fix moved the cap inside the run loop: `run_paced(steps, max_mtime)` stops when the guest RTC hits a wall-time-derived ceiling; the WFI idle loop honors it too. CLI/certified paths unchanged (`run` = ceiling `u64::MAX`).
- **Framebuffer**: opt-in 1MiB VRAM at 0x90000000 (not part of the certified platform), 800x600 rgb565 `simple-framebuffer` node + fbcon + boot logo in the demo image (kernel config additions in the extras layer only); page blits to a canvas that appears at first pixel.
- **Verification** `DEMO-VERIFIED`: prompt → tetris fullscreen after 4 *paced* seconds (game now runs at human speed) → clean quit → **FB-PIXELS-OK (88,372 nonzero bytes: Tux + console on the canvas)**. Regression battery on the changed tree: riscv-tests 106/108, both frozen boot layers BOOT-OK, lockstep spot-run prefix-clean.
- Published to gh-pages.

## 2026-07-06 — Sprint: single-screen UX + web polish, live

- **UX per operator review** (duplicate boot log on canvas *and* terminal was confusing): kernel log is serial-only again (`console=ttyS0`), fbcon/boot-logo removed from the demo kernel, so the canvas stays hidden until a program draws real pixels to `/dev/fb0` and appears only then. Verifier inverted to assert the new invariant: **FB-DARK-AT-BOOT-OK** (0 nonzero bytes at prompt) and **FB-PIXELS-OK after a guest `/dev/fb0` write** (159,379 nonzero bytes) — **DEMO-VERIFIED**.
- **Web polish**: UTF-8 decode in the terminal (Tetris's `← ↑ → ↓` arrows now render; keyboard input UTF-8-encoded symmetrically), and the MIPS readout is now a 500ms window that reads **idle** when the paced guest sleeps (the old lifetime average decayed misleadingly toward zero at an idle prompt). UTF-8 state machine node-tested including sequences split across writes and invalid-byte recovery.
- Published to gh-pages; served ELF sha256 `0d2f1a5f…` matches the verified build.

## 2026-07-06 — Sprint: networking — the closed loop works

- **virtio-net device** (`crates/rvemu-core/src/virtio.rs`): virtio-mmio v2 transport, split rings, features VERSION_1|MAC, at 0x10001000/PLIC IRQ 2 — opt-in via `enable_net()`, only the wasm demo build calls it; certified targets/harness never see the device. Ring logic unit-tested against hand-built virtqueues (rx single-desc + chained, buffer-starvation retry, tx gather).
- **JS user-mode gateway** (`web/net.js`, DOM-free): ARP responder, DNS server allocating a fake IP per name, ICMP echo, and a minimal TCP that terminates guest port-80 connections, parses HTTP, and re-issues them as browser `fetch()` over real HTTPS (only CORS-permissive hosts reachable; other ports get RST). Unit-tested with a scripted guest: checksums validated, 100KB transfer through a 4KB guest window reassembled byte-exact.
- **Debugging record, honest version**: first live boot had eth0 up, TX flowing, RX stuck at 0 packets. Suspected the ring code; unit tests exonerated it. Device diagnostics (queue-state dump export + counters) showed 265M device ticks with *zero* seeing a pending host frame: the unpaced verifier ran the guest through its full ARP-retry-and-timeout sequence inside a single wasm call, so gateway replies were always injected after the guest had given up — the same wall-vs-guest-time physics as the Tetris speed bug, resurfacing on the network path. Fix was in the *test* (pace the network phase like the real page), not the device; no emulator behavior changed.
- **NET-VERIFIED** (`extras/verify-net.mjs`): boot → eth0 up → `ping 10.0.2.2` → hermetic `wget` of a stubbed URL (40,023 bytes, length-checked) → `--live`: the guest wgot **this repo's own README from real raw.githubusercontent.com** through the gateway. A computer inside a browser tab fetching from the real internet.
- **Regression on the changed tree**: virtio unit tests green, riscv-tests 106/108 (ma_data pair reference-identical as since Gate A), xv6 + linux frozen boot layers BOOT-OK, demo verification (Tetris + fb) green on the networking image, bounded Linux lockstep vs Spike prefix-clean (see REPORTS.md note).

## 2026-07-06 — Sprint: virtio-input + DOOM, live

- **virtio-input device** (`crates/rvemu-core/src/virtio_input.rs`, device id 18): evdev keyboard + relative mouse from the page into the guest; config-space queries (name, dev ids, EV_KEY/EV_REL bitmaps), eventq delivery, statusq drained. Opt-in `enable_input()` — wasm demo build only; ring helpers shared with the net device, whose already-certified code was deliberately left untouched. Unit-tested (config reads + event delivery into a hand-built ring).
- **Page**: the canvas is now a real input device — evdev keymap from `event.code`, key routing to the guest keyboard while fullscreen, relative mouse motion + buttons, double-click for fullscreen (single click belongs to the guest now).
- **Demo image v5**: kernel gains VIRTIO_INPUT/EVDEV/MOUSEDEV; fbDOOM pinned `1728016` built static musl (`NOSDL=1` — first attempt tried to link SDL), shareware doom1.wad pinned by sha256 (`1d7d43be…`), `doom` wrapper. DTS gains the input node at 0x10002000/IRQ 3.
- **DOOM-VERIFIED** (`extras/verify-doom.mjs`): input device bound by name in /proc/bus/input/devices; injected key events read back byte-exact from /dev/input/event0 inside the guest (48 bytes for keydown+SYN); `doom` launches and draws a rich frame (369,724 nonzero fb bytes, 201 distinct values).
- **Battery on the changed tree**: unit tests 5/5, riscv-tests 106/108 (known pair), RISCOF 136/136, xv6 + linux BOOT-OK, DEMO-VERIFIED, NET-VERIFIED (incl. live), bounded Linux lockstep vs Spike PREFIX-CLEAN (rc=3).
- Published to gh-pages; served ELF sha256 `67628090…`.

## 2026-07-06 — Extras: curl in the demo image

- **curl 8.10.1** (pinned sha256 from the GitHub release mirror, matches curl.se's published hash) added to the demo image: static musl, `--without-ssl` (the page gateway terminates TLS; the guest speaks plain HTTP on port 80, same as busybox wget and links), sysroot zlib for gzip transfer decoding.
- **Build gotcha for the record**: curl links through libtool, and libtool silently swallows a plain `-static` (it never reaches the compiler) — the first build produced a dynamically linked binary that failed the layer's own static-link assertion. Fix: `make -C src LDFLAGS="-all-static"`. gpm/links never hit this because their final links don't go through libtool.
- **Verified** (`extras/verify-net.mjs`, now with curl checks): hermetic in-guest `curl -o` of the stubbed URL, exit code 0 and exact length match (40,023 bytes) — **CURL-STUB-OK / CURL-STUB-LENGTH-OK**; full run **NET-VERIFIED** including the live README wget.
- **Image re-verification** (artifact changed — kernel repack): **DEMO-VERIFIED**, **DOOM-VERIFIED** (rich-frame numbers byte-identical to the v5 certification). No Rust changed; certified targets/harness untouched.
- Session coordination note: this build also carries the links2 sprint's browse-on-tty1 fix (committed by the parallel session); links verification and the next gh-pages publish remain with that sprint.

## 2026-07-06 — Sprint: graphical browser (links2) + curl, live

- **links2 2.30** graphics mode rendering onto the framebuffer, static musl with pinned zlib 1.3.1 + libpng 1.6.43 + gpm 1.20.7 (the mouse daemon its fb driver requires). Guest browses over plain HTTP; the page gateway terminates TLS, so CORS-open hosts are reachable. `browse [url]` wrapper runs `links -g` on /dev/tty1 (the shell is on the serial console; links needs a real VT) with a setsid, defaulting to this repo's README.
- **curl 8.10.1** static musl, HTTP-only (`--without-ssl` — same gateway-terminates-TLS deal as wget/links), sysroot zlib for gzip transfer decoding; pinned by curl.se's published sha256.
- **Build archaeology, honest version**: getting 2000s-era C software onto musl/riscv took a chain of small fixes, each its own pinned commit — links2's autoconf-2.13 rejecting `VAR=VALUE` config args; twibright.com unreachable from Docker (tarball pre-fetched host-side, sha256-verified); gpm needing libtool, a `sys/sysmacros.h` include for `major()` on musl, the right make targets, and a sysroot dir that didn't exist yet; libtool swallowing curl's `-static` (needs `-all-static`). Two infrastructure non-failures also cost time and are logged: a GNU-mirror file dropping mid-download (fixed with a retry loop, not a code change) and a Docker build-cache snapshot corruption (fixed with `builder prune`). None were emulator issues.
- **LINKS-VERIFIED** (`extras/verify-links.mjs`): `browse` fetches a stubbed page through the gateway and links renders it to the framebuffer (928,437 nonzero bytes, 161 distinct values — a real rendered page). **NET-VERIFIED** now also covers curl (CURL-STUB byte-length-checked). DOOM and demo re-verified green.
- **Certification note**: the emulator core (rvemu-core/rvemu-wasm) is byte-identical since the virtio-input certification (`git diff` empty over `crates/`); this iteration changed only the demo userland and the page. So the input build's RISCOF 136/136 and prefix-clean lockstep stand unchanged; unit tests 5/5, riscv-tests 106/108, both frozen boot layers BOOT-OK re-run green here.
- Published to gh-pages.
