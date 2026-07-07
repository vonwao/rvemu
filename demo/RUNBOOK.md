# Demo video runbook — "This browser tab is a computer"

Target: 3–5 minutes, screen recording of https://vonwao.github.io/rvemu/ with voiceover. Every shot is a real interaction; nothing staged. Hard-refresh (Cmd+Shift+R) before recording so the newest image loads. Practice once — total live time is under 4 minutes if you don't wander.

## Cold open (0:00–0:20) — the boot

**Do:** Load the page fresh. Let the kernel log scroll in the terminal until `~ #` appears (~10–20s real time).

**Say:** "Everything you're about to see happens inside this one browser tab. No server, no cloud VM. The tab downloads a 20-megabyte Linux image and a CPU emulator compiled to WebAssembly, and boots it. That's a real Linux 6.12 kernel initializing real — well, really emulated — hardware."

**On screen worth pointing at:** the status line ("retired: N instructions"), the OpenSBI banner, `Run /init as init process`.

## Beat 1 (0:20–0:50) — prove it's real

**Do:** Click the terminal. Type `uname -a`, then `cat /proc/cpuinfo`, then `cat /proc/interrupts`.

**Say:** "This is not a terminal theme or a canned recording. It's a RISC-V processor implemented from scratch in Rust — every instruction interpreted one at a time — running an unmodified mainline kernel. Here's the hardware it thinks it has: one CPU core, a serial port, a network card, a keyboard. All of those devices are software in this tab."

## Beat 2 (0:50–1:30) — Tetris

**Do:** Type `tetris`. Play ~20 seconds visibly competently. Quit with `q`.

**Say:** "Games are a timing torture test. Early on, this ran at emulator speed — the game played itself to game-over before a human could react. The emulator now paces the guest's clock to your wall clock, so the machine *idles when idle* — check the status line: it says 'idle' at the prompt, which is your laptop battery being spared."

## Beat 3 (1:30–2:10) — the machine grows a screen

**Do:** Type `dd if=/dev/urandom of=/dev/fb0 bs=1600 count=100`. The canvas pops into view filled with static. Then say the magic word: type `doom`. Double-click the canvas for fullscreen once the title screen is up. Play ~20 seconds (arrows move, Ctrl fires, Enter/Esc menus). Esc out of fullscreen.

**Say:** "The machine has a video card — a framebuffer at a fixed memory address. The page keeps the monitor hidden until the first pixel is drawn. Random bytes make static... and yes, of course, it runs DOOM. The 1993 shareware WAD, drawn by the guest into video memory, blitted onto this canvas. When it's fullscreen, your keyboard becomes a virtual hardware keyboard — key-down and key-up events delivered through a virtio input device, the same mechanism real VMs use."

## Beat 4 (2:10–3:00) — the closed loop (the money shot)

**Do:** Type `ifconfig eth0`, then `ping -c 3 10.0.2.2`, then:

    wget -O- http://raw.githubusercontent.com/vonwao/rvemu/main/README.md | head -20

**Say:** "Here's my favorite part. The machine has a network card. The 'internet provider' it's plugged into is a few hundred lines of JavaScript in this same tab — it answers DNS, speaks TCP, and turns the guest's HTTP requests into browser fetch calls. So watch: Linux, inside the tab, downloads the README of its own emulator from the real internet. The computer in the browser is reading about itself."

**If links2 is live by recording day:** follow with `browse` — the graphical browser renders that same page onto the canvas, with your mouse working inside it.

## Close (3:00–3:45) — the twist

**Do:** Switch briefly to the GitHub repo (github.com/vonwao/rvemu). Show REPORTS.md scrolled to a gate report, then process/failures.md for two seconds.

**Say:** "One more thing. This emulator was built in about four days — by an AI agent, working autonomously. The rule that made it trustworthy: the test harness was written *first*, frozen, and the agent was never allowed to modify it to make a milestone pass. Every instruction was compared against Spike, the official RISC-V reference simulator — hundreds of millions of instructions, bit for bit. The repo is public, including the agent's own failure log — every wrong theory it chased is in there. The demo is linked below. It boots in ten seconds. Type `doom`."

## Short-form cuts (60s verticals)

- **DOOM cut:** cold open 5s (page loads, prompt) → `doom` → fullscreen gameplay → caption: "A browser tab, emulating a RISC-V computer, booting Linux, running DOOM. No server."
- **Closed-loop cut:** prompt → the wget one-liner → README scrolls → caption: "Linux inside a browser tab just downloaded its own source code from the internet. The network card is JavaScript."

## Recording notes

- Browser window ~1200px wide so the canvas scales cleanly; hide bookmarks bar.
- The paced boot takes 10–20s; don't cut it entirely — the scrolling kernel log is credibility. Speed it 2× in the edit at most.
- If a command typo happens, keep it — it proves live. Backspace works.
- Terminal focus matters: click the terminal before typing; DOOM keys need terminal focus (or canvas fullscreen).
- Have a second take of Beat 4 in case raw.githubusercontent.com is slow; the fetch usually completes in ~1s.
