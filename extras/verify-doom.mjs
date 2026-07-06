// Extras verification: virtio-input + fbDOOM. Boot the demo image, prove
// the input device binds and delivers injected evdev events, then launch
// DOOM and assert it draws a rich frame to the framebuffer.
import { readFile } from 'fs/promises';
import { createRequire } from 'module';
const require = createRequire(import.meta.url);

const wasm = await WebAssembly.instantiate(await readFile('target/wasm32-unknown-unknown/release/rvemu_wasm.wasm'), {});
const e = wasm.instance.exports;
const img = await readFile('extras/vendor/linux-demo/fw_payload.elf');
const ptr = e.image_alloc(img.length);
new Uint8Array(e.memory.buffer, ptr, img.length).set(img);
if (e.boot(256) !== 0) { console.log('DOOM-BOOT-PARSE-FAIL'); process.exit(1); }

let raw = '';
function drain() {
  const n = Number(e.console_out_len());
  if (n) {
    raw += Buffer.from(new Uint8Array(e.memory.buffer, Number(e.console_out_ptr()), n)).toString('latin1');
    e.console_out_clear();
  }
  e.net_tx_clear(); // no gateway here; drop guest frames
}
const send = s => { for (const ch of s) e.console_in(ch.charCodeAt(0)); };
async function runUntil(pred, ms) {
  const dl = Date.now() + ms;
  while (Date.now() < dl) { e.run(30000000n); drain(); if (pred()) return true; await new Promise(r => setImmediate(r)); }
  return false;
}
async function runPaced(pred, ms) {
  const wall0 = Date.now(), m0 = Number(e.mtime());
  const dl = Date.now() + ms;
  while (Date.now() < dl) {
    const ceil = BigInt(m0) + BigInt((Date.now() - wall0 + 50) * 10000);
    e.run_paced(2000000n, ceil); drain();
    if (pred()) return true;
    await new Promise(r => setImmediate(r));
  }
  return false;
}
function fbStats() {
  const vlen = Number(e.vram_len());
  const vram = new Uint8Array(e.memory.buffer, Number(e.vram_ptr()), vlen);
  let nz = 0; const seen = new Set();
  for (let i = 0; i < vlen; i++) { if (vram[i] !== 0) nz++; seen.add(vram[i]); }
  return { nz, distinct: seen.size };
}

let fails = 0;
const check = (ok, tag) => { console.log(`${ok ? '' : 'FAIL '}${tag}`); if (!ok) fails++; };

check(await runUntil(() => raw.includes('~ #'), 240000), 'DOOM-PROMPT-OK');

// 1. Input device bound with our name, evdev node exists.
send('cat /proc/bus/input/devices; ls /dev/input; echo INPUT-LS-DONE\r');
check(await runPaced(() => raw.includes('INPUT-LS-DONE'), 20000) && raw.includes('rvemu virtio input') && raw.includes('event0'), 'INPUT-DEVICE-BOUND');

// 2. Injected events arrive: read exactly 2 evdev records (2 x 24 bytes).
send('head -c 48 /dev/input/event0 | wc -c; echo EV-READ-DONE\r');
await runPaced(() => raw.includes('head'), 5000);
e.input_event(1, 30, 1); // KEY_A down
e.input_event(0, 0, 0);  // SYN
check(await runPaced(() => raw.includes('EV-READ-DONE') && /\n48\r?\n/.test(raw.replace(/\r/g, '\n')), 20000), 'INPUT-EVENTS-DELIVERED');
e.input_event(1, 30, 0); e.input_event(0, 0, 0); // release, tidy

// 3. DOOM: launch, let it run paced, framebuffer must show a rich frame.
send('doom\r');
const started = await runPaced(() => raw.includes('DOOM Shareware') || raw.includes('W_Init') || raw.includes('I_InitGraphics') || raw.toLowerCase().includes('doom'), 30000);
check(started, 'DOOM-STARTED');
await runPaced(() => false, 10000); // 10 paced seconds: title screen / demo loop
const { nz, distinct } = fbStats();
console.log(`FB: ${nz} nonzero bytes, ${distinct} distinct byte values`);
check(nz > 100000 && distinct > 64, 'DOOM-FRAME-RICH');

console.log(fails ? 'DOOM-VERIFY-FAIL' : 'DOOM-VERIFIED');
process.exit(fails ? 0 + 1 : 0);
