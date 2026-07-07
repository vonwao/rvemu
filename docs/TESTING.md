# Testing & running the demos

Two audiences for this doc: someone who wants to **try the demos** (browser or headless), and someone who wants to **verify a change** before shipping it. Everything here runs from the repo root.

## Prerequisites

- Rust toolchain with the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`).
- Node 20+ (developed on 22.19). The headless verifiers are plain ESM, no npm install.
- For the OS-image and lockstep tests: Docker (image builds) and the vendored Spike (built by `targets/`).

## Build the pieces

```
cargo build --release                                             # native rvemu CLI
cargo build --release --target wasm32-unknown-unknown -p rvemu-wasm  # the wasm the page loads
./extras/build-linux-demo.sh                                      # the demo Linux image (tetris/doom/links/net)
```

The demo image lands at `extras/vendor/linux-demo/fw_payload.elf`. The certified (no-extras) image is `targets/vendor/linux-build/fw_payload.elf` and is what all harness/lockstep tests use — never overwrite it with a demo build.

## Try it in a browser

```
./web/serve.sh          # copies the current wasm + demo image next to index.html, serves :8000
```

Open http://localhost:8000/, click the terminal, wait for the `~ #` prompt, then:

| Command | What happens |
|---|---|
| `uname -a` | Linux 6.12 on riscv64 — proves it's a real kernel |
| `tetris` | Plays at human speed; `j`/`l` move, `k` rotate, space drop, `q` quit |
| `doom` | fbDOOM draws to the canvas; double-click the canvas for fullscreen (keyboard routes to the guest there) |
| `dd if=/dev/urandom of=/dev/fb0 bs=1600 count=100` | Fills the framebuffer canvas with static — makes the "monitor" appear |
| `ifconfig eth0` / `ping 10.0.2.2` | The virtio NIC and the in-page JS gateway |
| `wget -O- http://raw.githubusercontent.com/vonwao/rvemu/main/README.md` | The closed loop — fetches from the real internet through the gateway |
| `curl -s http://raw.githubusercontent.com/vonwao/rvemu/main/README.md` | Same, via curl |
| `browse` | **The graphical browser** — links2 renders a web page onto the framebuffer canvas |

### Testing `links2` specifically

`browse [url]` is a wrapper around `links -g`. Two things make it non-obvious:

1. **It needs a real virtual terminal.** The shell runs on the serial console (`ttyS0`), but `links -g` grabs a VT to drive the framebuffer. The wrapper runs it on `/dev/tty1` via `setsid` — that's why plain `links -g http://...` from the prompt fails with "Could not get VT mode", but `browse` works. If you invoke links yourself, replicate that: `setsid sh -c 'links -g URL </dev/tty1 >/dev/tty1 2>&1'`.
2. **Only CORS-open hosts resolve.** The guest speaks plain `http://`; the page gateway upgrades it to a real HTTPS `fetch()`. Browsers block cross-origin fetches unless the host sends permissive CORS headers, so `browse http://raw.githubusercontent.com/...` works but `browse http://google.com` will hang/fail. This is browser security, not an emulator limit. See `docs/FUTURE-WORK.md` for the full analysis.

In the browser, `browse` with no argument opens this repo's README. Navigation keys are links2's own (arrows, Enter to follow a link, `q` to quit). The mouse works in fullscreen (a `gpm` daemon bridges `/dev/input/mice` to links).

To watch it headlessly (no browser), see the verifier below — it drives the real wasm module and asserts a rich frame was drawn.

## Verify a change (headless, the honest way)

Each extra has a Node verifier that boots the **actual wasm build** against the **actual demo image** and asserts observable behavior — not "the binary exists", but "the guest did the thing". Run from repo root after building the wasm and the image:

```
node extras/verify-demo.mjs     # tetris paced + framebuffer dark-at-boot then pixels-on-write
node extras/verify-doom.mjs     # virtio-input bound, injected key read back in-guest, DOOM draws a rich frame
node extras/verify-net.mjs        # hermetic: ping + wget + curl a stubbed URL through the gateway
node extras/verify-net.mjs --live # additionally fetches this repo's README over the real internet
node extras/verify-links.mjs    # browse fetches a stubbed page through the gateway, links renders it to the framebuffer
```

Each prints tagged checkpoints and ends in `*-VERIFIED` (exit 0) or `*-VERIFY-FAIL` (exit 1). The framebuffer checks read VRAM directly and count nonzero bytes + distinct byte values, so a blank or garbage screen fails.

**Why the verifiers pace the guest clock** (`run_paced` with a wall-time ceiling): a single wasm `run()` call fast-forwards through the guest's idle/wait periods internally. An unpaced test races the guest through its own network timeouts and game timers before any host reply or human-speed frame exists — you get false failures that look like device bugs. Networking and interactive checks *must* be paced; this is the same physics that made early Tetris unplayably fast. Don't "fix" a flaky network verifier by removing the pacing.

## Verify the emulator core (the frozen harness)

Only needed when you change `crates/rvemu-core` or `crates/rvemu-cli` (the interpreter/CSRs/MMU/devices). Pure userland or web changes don't touch the certified core — confirm with `git diff --stat <last-core-cert-commit> HEAD -- crates/` being empty, and you can skip this battery.

```
cargo test -p rvemu-core                       # unit tests (incl. virtio ring tests)
harness/run-riscv-tests.sh                     # riscv-tests -p/-v (expect 106/108; the ma_data pair is Spike-identical, see failures.md)
harness/riscof/run-riscof.sh                   # RISCOF architectural suite (expect 136/136)
harness/boot/run-boot.sh xv6                   # xv6 boots to shell (BOOT-OK)
harness/boot/run-boot.sh linux                 # Linux boots to BusyBox (BOOT-OK)
LOCKSTEP_ISA=rv64imac_zicsr_zifencei_zicntr_sstc \
  harness/lockstep/lockstep.sh \
  targets/vendor/linux-build/fw_payload.elf 30000000   # instruction-lockstep vs Spike; rc=3 = prefix-clean
```

**The rule:** if a harness test fails, fix the emulator — never edit the harness to pass. `harness/` and `targets/` are frozen (chmod `a-w`). The whole point of the project is that this line is never crossed; `process/failures.md` records the times it was tempting.

Long-running note: the lockstep run takes many minutes and produces no output until the end. If you background it, read its stderr before assuming it was "killed" — a wrong ELF path crashes both simulators instantly and leaves the comparator blocked on a pipe, which looks identical to a hang. (This bit us once; see `process/failures.md` 2026-07-06.)
