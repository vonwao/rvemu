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
    e.run(1000000n); drain();
    if (pred()) return true;
    const ahead = (Number(e.mtime()) - m0) / 10000 - (Date.now() - wall0);
    if (ahead > 5) sleepMs(Math.min(ahead, 100));
  }
  return false;
}

if (!runUntil(() => raw.includes('~ #'), 240000)) { console.log('DEMO-NO-PROMPT'); process.exit(1); }
console.log('DEMO-PROMPT-OK');
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
const ok = inGame && back;
console.log(ok ? 'DEMO-VERIFIED' : 'DEMO-FAIL');
process.exit(ok ? 0 : 1);
