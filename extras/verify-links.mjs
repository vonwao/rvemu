// Extras verification: links2 graphical browser. Boot the demo image with
// the JS gateway attached (stubbed fetch, hermetic), run `browse`, and
// assert (1) the gateway served links' HTTP request, (2) links rendered a
// rich frame to the framebuffer.
import { readFile } from 'fs/promises';
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const { NetGateway } = require('../web/net.js');

const wasm = await WebAssembly.instantiate(await readFile('target/wasm32-unknown-unknown/release/rvemu_wasm.wasm'), {});
const e = wasm.instance.exports;
const img = await readFile('extras/vendor/linux-demo/fw_payload.elf');
const ptr = e.image_alloc(img.length);
new Uint8Array(e.memory.buffer, ptr, img.length).set(img);
if (e.boot(256) !== 0) { console.log('LINKS-BOOT-PARSE-FAIL'); process.exit(1); }

const PAGE = '<html><body><h1>rvemu closed loop</h1><p>' + 'rendered by links2 on the emulated framebuffer. '.repeat(30) + '</p></body></html>';
let served = 0;
const stubFetch = async (url) => {
  served++;
  console.log(`gateway fetch #${served}: ${url}`);
  return {
    status: 200, statusText: 'OK',
    headers: { get: () => 'text/html' },
    arrayBuffer: async () => new TextEncoder().encode(PAGE).buffer,
  };
};
const gateway = new NetGateway((frame) => {
  const p = Number(e.net_rx_alloc(frame.length));
  new Uint8Array(e.memory.buffer, p, frame.length).set(frame);
  e.net_rx_push();
}, stubFetch);

let raw = '';
function drain() {
  const n = Number(e.console_out_len());
  if (n) {
    raw += Buffer.from(new Uint8Array(e.memory.buffer, Number(e.console_out_ptr()), n)).toString('latin1');
    e.console_out_clear();
  }
  const nn = Number(e.net_tx_len());
  if (nn) {
    const buf = new Uint8Array(e.memory.buffer, Number(e.net_tx_ptr()), nn).slice();
    e.net_tx_clear();
    let i = 0;
    while (i + 2 <= buf.length) { const len = buf[i] | (buf[i + 1] << 8); gateway.onFrame(buf.subarray(i + 2, i + 2 + len)); i += 2 + len; }
  }
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

check(await runUntil(() => raw.includes('~ #'), 240000), 'LINKS-PROMPT-OK');
check(raw.includes('eth0 up'), 'LINKS-ETH0-OK');

send('links -version; echo LV-DONE\r');
check(await runPaced(() => raw.includes('LV-DONE'), 20000) && /Links 2\.30/i.test(raw), 'LINKS-BINARY-OK');

// Launch the graphical browser through the `browse` wrapper (tty1 + gateway).
send('browse http://demo.test/page.html\r');
check(await runPaced(() => served > 0, 90000), 'LINKS-FETCHED-VIA-GATEWAY');
// Give the renderer paced time to draw, then snapshot the framebuffer.
await runPaced(() => false, 8000);
const { nz, distinct } = fbStats();
console.log(`FB: ${nz} nonzero bytes, ${distinct} distinct byte values`);
check(nz > 100000 && distinct > 16, 'LINKS-FRAME-RENDERED');

console.log(fails ? 'LINKS-VERIFY-FAIL' : 'LINKS-VERIFIED');
process.exit(fails ? 1 : 0);
