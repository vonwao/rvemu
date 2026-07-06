// Extras verification: boot the demo image in the wasm build, reach the
// shell, run tetris, assert the playfield renders, quit cleanly.
import { readFile } from 'fs/promises';
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const { Screen } = require('../web/term.js');

const wasm = await WebAssembly.instantiate(await readFile('target/wasm32-unknown-unknown/release/rvemu_wasm.wasm'), {});
const e = wasm.instance.exports;
const img = await readFile('extras/vendor/linux-demo/fw_payload.elf');
const ptr = e.image_alloc(img.length);
new Uint8Array(e.memory.buffer, ptr, img.length).set(img);
if (e.boot(256) !== 0) { console.log('DEMO-BOOT-PARSE-FAIL'); process.exit(1); }

const screen = new Screen(80, 24);
let raw = '';
function drain() {
  const n = Number(e.console_out_len());
  if (n) {
    const b = new Uint8Array(e.memory.buffer, Number(e.console_out_ptr()), n).slice();
    screen.write(b); raw += Buffer.from(b).toString('latin1'); e.console_out_clear();
    for (const r of screen.takeReplies()) e.console_in(r);
  }
}
const send = s => { for (const ch of s) e.console_in(ch.charCodeAt(0)); };
function runUntil(pred, ms) { const dl = Date.now() + ms; while (Date.now() < dl) { e.run(30000000n); drain(); if (pred()) return true; } return false; }
// Paced variant mirroring the browser page: guest RTC capped at wall time,
// so interactive programs run at human speed during the check.
const sleepMs = ms => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
function runPaced(pred, ms) {
  const wall0 = Date.now(), m0 = Number(e.mtime());
  const dl = Date.now() + ms;
  while (Date.now() < dl) {
    const ceil = BigInt(m0) + BigInt((Date.now() - wall0 + 50) * 10000);
    e.run_paced(2000000n, ceil); drain();
    if (pred()) return true;
    const ahead = (Number(e.mtime()) - m0) / 10000 - (Date.now() - wall0);
    if (ahead > 5) sleepMs(Math.min(ahead, 100));
  }
  return false;
}

function fbNonzero() {
  const vlen = Number(e.vram_len());
  if (!vlen) return -1;
  const vram = new Uint8Array(e.memory.buffer, Number(e.vram_ptr()), vlen);
  let nz = 0;
  for (let i = 0; i < vlen; i++) if (vram[i] !== 0) nz++;
  return nz;
}

if (!runUntil(() => raw.includes('~ #'), 240000)) { console.log('DEMO-NO-PROMPT'); process.exit(1); }
console.log('DEMO-PROMPT-OK');
// Single-screen UX invariant: no fbcon/logo, so the framebuffer must still be
// all-black at the prompt (the page keeps the canvas hidden until the first
// nonzero pixel).
const bootNz = fbNonzero();
const darkOk = bootNz === 0;
console.log(`FB nonzero bytes at prompt: ${bootNz}`);
console.log(darkOk ? 'FB-DARK-AT-BOOT-OK' : 'FB-NOT-DARK-AT-BOOT');
send('ls -la /bin/tetris\r');
if (!runUntil(() => /tetris/.test(screen.toText()) && raw.includes('-rwxr-xr-x'), 15000)) { console.log('TETRIS-MISSING'); console.log(screen.toText()); process.exit(1); }
console.log('TETRIS-PRESENT');
send('tetris\r');
// Let ~4 wall-seconds of properly-paced game time elapse, then snapshot:
// a running fullscreen game means the shell prompt is gone and the screen
// has substantial drawn content.
runPaced(() => false, 4000);
const mid = screen.toText();
console.log('--- tetris mid-game screen ---');
console.log(mid);
const inGame = !mid.includes('~ #') && mid.split('\n').filter(l => l.trim().length > 0).length >= 4;
console.log(inGame ? 'TETRIS-RUNNING-FULLSCREEN' : 'TETRIS-NOT-RUNNING');
send('q');
const back = runPaced(() => screen.toText().split('\n').some(l => l.includes('~ #')), 20000);
console.log(back ? 'QUIT-TO-PROMPT-OK' : 'QUIT-FAIL');
// Framebuffer: a guest program writing /dev/fb0 must produce visible pixels
// (this is the path a graphical app draws through).
send('dd if=/dev/urandom of=/dev/fb0 bs=1600 count=100 2>/dev/null; echo FBWRITE-$?\r');
runUntil(() => raw.includes('FBWRITE-0'), 20000);
const nonzero = fbNonzero();
const fbOk = nonzero > 10000;
console.log(`FB nonzero bytes after /dev/fb0 write: ${nonzero}`);
console.log(fbOk ? 'FB-PIXELS-OK' : 'FB-BLANK');
const ok = darkOk && inGame && back && fbOk;
console.log(ok ? 'DEMO-VERIFIED' : 'DEMO-FAIL');
process.exit(ok ? 0 : 1);
