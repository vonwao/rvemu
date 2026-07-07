# Future work — handoff plan

This is a handoff document. The charter gates (A/B/C) are complete and certified; the extras track (pacing, framebuffer, networking, DOOM, graphical browser) is complete and live at https://vonwao.github.io/rvemu/. What follows is a roadmap another engineer — or another model — can pick up cold. Read the three governance rules first; they're the whole reason this project is trustworthy, and every item below inherits them.

## Governance (read this first — non-negotiable)

1. **The harness is frozen.** `harness/` and `targets/` are read-only after day 0 (chmod `a-w`). When a test fails, you fix the emulator, never the test. If a change makes a milestone impossible without touching the harness, that's a blocked item for the human, not license to edit the harness. `process/failures.md` is the honest log of every time weakening a test was the easy path and wasn't taken — keep adding to it.
2. **Extras never touch the certified core to earn their keep.** New demo capability goes in opt-in devices (`enable_*` on the bus, wasm-only) and in the demo image (`extras/linux-demo/`), which is a Docker layer on the *pinned* base image. Certified `targets/` recipes and the base image are not modified. If an extra changes `crates/rvemu-core` at all, you owe the full re-certification battery (see `docs/TESTING.md`) — but most extras are userland-only and the core stays byte-identical, in which case the last core certification carries (prove it with an empty `git diff --stat <cert-commit> HEAD -- crates/`).
3. **Pin everything, verify observably.** Every external source (git commit, tarball) is pinned by commit hash or sha256. Every capability gets a `verify-*.mjs` that boots the real wasm against the real image and asserts the guest *did the thing* (framebuffer bytes drawn, bytes read back in-guest), not that a file exists. Copy an existing verifier as the template.

## The CORS constraint (governs every networking idea)

The in-page network gateway (`web/net.js`) terminates the guest's TCP:80 into a browser `fetch()`. The guest speaks plain `http://`; the real wire is HTTPS. **A browser `fetch()` can only read a cross-origin response if the host sends permissive CORS headers.** So:

- **Reachable:** `raw.githubusercontent.com`, most CDNs that set `Access-Control-Allow-Origin: *` (many static/asset hosts do).
- **Not reachable from a browser tab:** `google.com`, most websites, and — critically for the item below — the official Alpine/Debian package mirrors. They don't send permissive CORS headers, so `fetch()` gets the bytes blocked.
- **The only escapes** are (a) host content on a CORS-open origin you control (GitHub raw, an S3/R2 bucket with CORS enabled), or (b) stand up a small CORS-proxy relay server — which breaks the "no server, it's all in the tab" story and should be a deliberate, labeled choice, not a default.

Do not design any networking feature that assumes arbitrary hosts work. The clever move is almost always "host what you need on a CORS-open origin."

---

## Item 1 (headline): apk package support — install packages at runtime

**Goal:** `apk add <pkg>` works inside the running guest, so the demo isn't limited to what was baked into the initramfs.

**The catch, up front:** Alpine's real mirrors (`dl-cdn.alpinelinux.org`) are HTTPS and **not CORS-open**, so `apk` pointed at them will fail in the browser for the reason above. Don't spend a day discovering this. The design has to route apk through a CORS-open origin.

**Recommended design — a self-hosted apk repo on a CORS-open origin:**

1. Alpine has a real `riscv64` port (edge, and 3.20+). apk-tools has a static build (`apk.static`). Add `apk.static` (pinned) to the demo image, plus `/etc/apk/keys/` with the Alpine signing keys, and an `/etc/apk/repositories` pointing at **an http URL on a CORS-open host you control**, not dl-cdn.
2. Build a **small curated apk mirror** — a directory with an `APKINDEX.tar.gz` and the `.apk` files for a handful of demo-worthy packages — and host it where `fetch()` can read it: either `raw.githubusercontent.com/<you>/rvemu-apk-repo/...` (simplest; CORS-open) or an R2/S3 bucket with CORS enabled. Pin the package versions.
3. The guest does `apk add --repository http://raw.githubusercontent.com/<you>/rvemu-apk-repo/edge/main <pkg>`. This flows through the existing gateway exactly like the wget/links demos.

**Subtleties to expect:**
- apk over plain http needs either signed indexes (ship the keys) or `--allow-untrusted` (simpler for a demo; state it in the recipe).
- Our gateway's TCP is minimal (see `web/net.js`): single-connection HTTP/1.0-ish, `Connection: close`. apk may open several requests; confirm the gateway handles back-to-back connections cleanly. If apk uses HTTP keep-alive or ranges, the gateway may need a small extension — that's real emulator-adjacent work, test it with a `verify-apk.mjs`.
- musl vs glibc: Alpine is musl, which matches our toolchain — good. But packages pulled in may want kernel features we disabled (see the demo kernel config); enable per-package as needed in the demo image's kernel-config step, never in the certified base.
- Determinism/verification: `verify-apk.mjs` should boot the image, `apk add` a known small package from a **stubbed** gateway (hermetic, like `verify-net.mjs`'s stub path), and assert the binary appears and runs. Add a `--live` arm that hits the real GitHub-hosted repo.

**Effort:** medium-high. The mirror-hosting and gateway-robustness are the real work; apk.static itself is a pinned download. This is genuinely useful — it turns the demo from "fixed toybox" into "a machine you can install software onto," which is a strong story.

**Governance:** userland + demo-image only if the gateway doesn't need changes. If the gateway (`web/net.js`) needs HTTP keep-alive/ranges to satisfy apk, that's page-side JS (still not the certified Rust core), but re-run `verify-net.mjs` and `verify-links.mjs` to confirm you didn't regress the existing HTTP path.

## Item 2: on-page "try these" hint + network activity line

Small, high-leverage UX. First-time visitors (especially from a video) hit a bare prompt.
- Add a cycling hint line under the status bar: `try: tetris · doom · browse · wget …`.
- Add a "last fetched: <url>" line driven by the gateway's existing request log (`web/net.js` already sees every URL). Makes the closed loop *visible* to an audience without them trusting narration.
- Pure `web/index.html` + `web/net.js`; no image rebuild, no core change. Verify by eye in `serve.sh`.

## Item 3: snapshot / instant-boot

Boot takes ~10-20s. A memory snapshot (dump guest RAM + CPU/CSR state after boot, restore on load) would make the demo start instantly.
- Add `snapshot()` / `restore()` to the wasm: serialize `bus.ram`, CPU registers/CSRs, CLINT/PLIC/UART/device state. Ship a post-boot snapshot blob the page loads instead of cold-booting.
- This *does* touch `crates/rvemu-core` (needs (de)serialization of machine state) — but it's additive (new methods), doesn't change execution semantics, and the core diff should be provably behavior-preserving. Re-run the battery anyway; a snapshot that restores wrong state is a correctness bug.
- Verify: snapshot after boot, restore in a fresh instance, assert the prompt is immediately present and a command runs.

## Item 4: the writeup / video (non-code)

- `demo/RUNBOOK.md` is the video shot-list, ready to shoot.
- The engineering-audience post writes itself from `process/`: the frozen-harness rule, the ten-divergence lockstep chain (`process/divergences.md`), and `failures.md` (every wrong theory, including where weakening a test was the easy path). Title candidate: "The frozen harness: letting an agent build a RISC-V emulator it couldn't cheat on." Pairs with the video for an HN/r/rust post.

## Candidate backlog (lower priority, each self-contained)

- **More fb apps:** `mplayer`/`ffmpeg` fb output, an image viewer (`fbi`), `fbterm`. Same pattern as links2: static musl build, pinned, `verify-*.mjs` asserting a rich frame.
- **Sound:** a virtio-sound or simple PCM device → WebAudio. New opt-in device (core change, full battery).
- **Persistence:** a virtio-blk backed by browser IndexedDB, so files survive reloads. New device (core change). Note the charter's "no block device" non-goal is about *certified* scope — an opt-in extras device is fine, but say so explicitly.
- **Multi-arch pages:** the same gateway/framebuffer/input plumbing could host other guests. Out of scope for this repo but the seams are clean.

## Where things live (orientation for whoever picks this up)

- Emulator core: `crates/rvemu-core/src/` — `cpu.rs` (interpreter), `bus.rs` (device dispatch + `enable_*`), `virtio.rs` / `virtio_input.rs` (opt-in devices), `csr.rs`, `machine.rs` (`run_paced`).
- wasm bridge: `crates/rvemu-wasm/src/lib.rs` — the `extern "C"` exports the page calls.
- Page: `web/index.html` (renderer, input, pacing loop), `web/term.js` (VT100), `web/net.js` (gateway).
- Demo image: `extras/linux-demo/Dockerfile` (+ `platform-demo.dts`, `demo-init`), built by `extras/build-linux-demo.sh`. Governance and current-extras list in `extras/README.md`.
- Verifiers: `extras/verify-*.mjs`. Certification reports: `REPORTS.md`. Narrative/process: `process/{timeline,divergences,failures}.md`.
- How to build/run/verify anything: `docs/TESTING.md`.

## Build-gotcha cheat sheet (learned the hard way this session)

Old C software on musl/riscv fails in predictable ways; each below cost real time and is now a pinned fix in the Dockerfile you can pattern-match against:
- **Ancient autoconf** (links2's 2.13) rejects `./configure CC=... VAR=VALUE` — pass them as *environment* prefixes instead: `CC=... ./configure ...`.
- **Flaky source mirrors** (twibright, GNU): pre-fetch the tarball host-side, pin its sha256, `COPY` it into the build; or wrap the build in a small retry loop. A dropped download is not a code failure — check the log before "fixing" anything.
- **musl missing glibc conveniences:** `major()`/`minor()` need `#include <sys/sysmacros.h>`; some sources need `<sys/select.h>` for `fd_set`. Patch with `sed` and an **asserted anchor** (`grep -q` the expected text first) so a silent no-op patch can't slip through — this exact class of silent-patch bug bit the xv6 work too.
- **libtool eats `-static`:** a plain `-static` in `LDFLAGS` never reaches the final link; use `-all-static` (curl) or link the tool's own way.
- **Docker cache corruption** ("failed to prepare extraction snapshot / parent snapshot does not exist"): not your code — `docker builder prune -f` and rebuild.
- **Never let a base-image rebuild clobber certified artifacts.** `targets/vendor/linux-build/fw_payload.elf` is the exact binary every lockstep/boot cert references. If you must rebuild the base, back those files up first and restore them after (this session did; the rebuild is not guaranteed byte-identical). Overwriting them silently invalidates every certification claim.
