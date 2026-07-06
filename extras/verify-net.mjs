// Extras verification: networking. Boot the demo image in the wasm build,
// bring up the JS gateway, then from inside the guest: ping the gateway and
// wget a URL. Hermetic by default (fetch is stubbed with known content so
// the check needs no real network); `--live` additionally wgets the
// project's own README over the real fetch().
import { readFile } from 'fs/promises';
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const { NetGateway } = require('../web/net.js');

const live = process.argv.includes('--live');

const wasm = await WebAssembly.instantiate(await readFile('target/wasm32-unknown-unknown/release/rvemu_wasm.wasm'), {});
const e = wasm.instance.exports;
const img = await readFile('extras/vendor/linux-demo/fw_payload.elf');
const ptr = e.image_alloc(img.length);
new Uint8Array(e.memory.buffer, ptr, img.length).set(img);
if (e.boot(256) !== 0) { console.log('NET-BOOT-PARSE-FAIL'); process.exit(1); }

const STUB_BODY = 'hello-from-the-gateway-' + 'y'.repeat(40000); // multi-segment
const stubFetch = async (url, opts) => {
  if (url === 'https://stub.test/hello') {
    return {
      status: 200, statusText: 'OK',
      headers: { get: () => 'text/plain' },
      arrayBuffer: async () => new TextEncoder().encode(STUB_BODY).buffer,
    };
  }
  return fetch(url, opts); // --live path
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
    while (i + 2 <= buf.length) {
      const len = buf[i] | (buf[i + 1] << 8);
      gateway.onFrame(buf.subarray(i + 2, i + 2 + len));
      i += 2 + len;
    }
  }
}
const send = s => { for (const ch of s) e.console_in(ch.charCodeAt(0)); };
// Async so gateway fetch() promises can resolve between run slices.
async function runUntil(pred, ms) {
  const dl = Date.now() + ms;
  while (Date.now() < dl) {
    e.run(30000000n);
    drain();
    if (pred()) return true;
    await new Promise(r => setImmediate(r));
  }
  return false;
}
// Paced like the browser page. Networking REQUIRES this: gateway replies can
// only be injected between run calls, so an unpaced run fast-forwards the
// guest through its ARP/TCP timeouts before any reply exists (the same
// wall-vs-guest-time physics as interactive input).
async function runPaced(pred, ms) {
  const wall0 = Date.now(), m0 = Number(e.mtime());
  const dl = Date.now() + ms;
  while (Date.now() < dl) {
    const ceil = BigInt(m0) + BigInt((Date.now() - wall0 + 50) * 10000);
    e.run_paced(2000000n, ceil);
    drain();
    if (pred()) return true;
    await new Promise(r => setImmediate(r));
  }
  return false;
}

let fails = 0;
const check = (ok, tag) => { console.log(`${ok ? '' : 'FAIL '}${tag}`); if (!ok) fails++; };

check(await runUntil(() => raw.includes('~ #'), 240000), 'NET-PROMPT-OK');
check(raw.includes('initramfs: eth0 up (10.0.2.15)'), 'ETH0-UP-OK');

send('ping -c 1 10.0.2.2; echo PING-$?\r');
check(await runPaced(() => raw.includes('PING-0'), 30000), 'PING-GATEWAY-OK');

send('wget -qO- http://stub.test/hello > /h; echo WGET-$?; head -c 23 /h; echo; echo SIZE-$(wc -c < /h)\r');
const wgetOk = await runPaced(() => raw.includes('WGET-0') && raw.includes('hello-from-the-gateway-') && /SIZE-\d/.test(raw), 60000);
check(wgetOk, 'WGET-STUB-OK');
check(raw.includes(`SIZE-${STUB_BODY.length}`), 'WGET-STUB-LENGTH-OK');

if (live) {
  send('wget -qO- http://raw.githubusercontent.com/vonwao/rvemu/main/README.md | head -1; echo LIVE-DONE\r');
  const liveOk = await runPaced(() => raw.includes('LIVE-DONE'), 120000);
  check(liveOk && /rvemu/i.test(raw.slice(raw.indexOf('LIVE-DONE') - 500)), 'WGET-LIVE-OK');
}

console.log(fails ? 'NET-VERIFY-FAIL' : 'NET-VERIFIED');
process.exit(fails ? 1 : 0);
