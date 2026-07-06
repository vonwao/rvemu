# extras/ — post-charter demo extensions

The charter's gate ladder completed 2026-07-04 (see REPORTS.md). Work here extends the demo beyond the charter's scope under these rules, agreed with the operator:

- The certified target images and their recipes in `targets/` are never modified; every charter certification (riscv-tests, RISCOF, lockstep, boot layers) remains re-runnable against them at any time.
- `harness/` stays frozen. Extras get their own verification scripts here — additive checks, never replacements.
- Extras images are separate pinned recipes (Docker layers on top of the pinned base image where possible) with their own version pins recorded.
- Where an extra makes Spike lockstep impossible in principle (e.g., devices Spike doesn't model), that loss is stated in the recipe header rather than papered over.

## Current extras

- **Micro Tetris** (pinned commit in `linux-demo/Dockerfile`), with in-loop guest-time pacing so it plays at human speed (`Machine::run_paced`). Verified by `verify-demo.mjs`.
- **Framebuffer**: simple-framebuffer at 0x90000000, `/dev/fb0` only — no fbcon/logo, so the page's canvas stays hidden until a program draws real graphics. Not Spike-comparable (fb region doesn't exist there); device is opt-in (`enable_vram`) and never present on certified paths. Verified by `verify-demo.mjs` (dark at boot, pixels after an fb write).
- **Networking**: virtio-mmio NIC (`crates/rvemu-core/src/virtio.rs`, opt-in via `enable_net`, wasm-only) bridged to a user-mode JS gateway (`web/net.js`) that answers ARP/DNS/ICMP and terminates guest TCP port 80 into browser `fetch()` — the guest speaks plain HTTP, the real wire is HTTPS, so only CORS-permissive hosts (e.g. raw.githubusercontent.com) are reachable and TCP to other ports gets an RST. Non-determinism note: rx frame delivery timing is host-driven, so runs with network traffic are not replay-deterministic; the certified images have no NIC and are unaffected. Verified by `verify-net.mjs` (hermetic stubbed-fetch wget + ping; `--live` adds a real fetch of this repo's README).
