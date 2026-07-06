// User-mode network gateway: terminates the guest's ethernet traffic in
// JavaScript and re-issues HTTP requests through fetch(). slirp-style
// addressing: guest 10.0.2.15/24, gateway 10.0.2.2, DNS 10.0.2.3. DNS A
// queries get a fake IP per name; TCP port 80 to any such IP (or with a Host
// header) is parsed as HTTP and fetched over real HTTPS by the browser, so
// the guest speaks plain HTTP while the wire outside is TLS. Anything else
// gets an RST. No DOM and injectable fetch, so it runs under Node for
// headless verification.
'use strict';

const GW_MAC = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];
const GW_IP = [10, 0, 2, 2];
const DNS_IP = [10, 0, 2, 3];
const MSS = 1460;

function csum16(bytes, start, len, init = 0) {
  let s = init;
  for (let i = 0; i < len - 1; i += 2) s += (bytes[start + i] << 8) | bytes[start + i + 1];
  if (len & 1) s += bytes[start + len - 1] << 8;
  while (s > 0xffff) s = (s & 0xffff) + (s >>> 16);
  return (~s) & 0xffff;
}

const ipEq = (b, off, ip) => b[off] === ip[0] && b[off + 1] === ip[1] && b[off + 2] === ip[2] && b[off + 3] === ip[3];
const ipStr = (b, off) => `${b[off]}.${b[off + 1]}.${b[off + 2]}.${b[off + 3]}`;

class NetGateway {
  constructor(sendFrame, fetchFn) {
    this.sendFrame = sendFrame;      // (Uint8Array) => void, delivers to guest
    this.fetch = fetchFn;            // fetch-compatible
    this.guestMac = null;            // learned from first frame
    this.dnsNames = new Map();       // "a.b.c.d" -> hostname
    this.dnsIps = new Map();         // hostname -> [a,b,c,d]
    this.nextIp = 1;                 // allocator in 10.1.0.0/16
    this.conns = new Map();          // "srcport:dstip:dstport" -> conn
    this.log = () => {};             // set to console.log for debugging
  }

  onFrame(f) {
    if (f.length < 14) return;
    this.guestMac = Array.from(f.slice(6, 12));
    const etype = (f[12] << 8) | f[13];
    if (etype === 0x0806) this.handleArp(f);
    else if (etype === 0x0800) this.handleIp(f);
  }

  eth(etype) {
    const b = [];
    b.push(...this.guestMac, ...GW_MAC, (etype >> 8) & 0xff, etype & 0xff);
    return b;
  }

  handleArp(f) {
    // Request for any 10.0.2.x other than the guest: it's ours.
    const oper = (f[20] << 8) | f[21];
    const target = f.slice(38, 42);
    if (oper !== 1 || (target[3] === 15 && ipEq(target, 0, [10, 0, 2, 15]))) return;
    const r = this.eth(0x0806);
    r.push(0, 1, 8, 0, 6, 4, 0, 2);            // htype/ptype/hlen/plen/oper=reply
    r.push(...GW_MAC, ...target);              // sender = us at the asked IP
    r.push(...f.slice(22, 28), ...f.slice(28, 32)); // target = guest
    this.sendFrame(new Uint8Array(r));
  }

  // Build ethernet+IPv4 packet toward the guest.
  ipPacket(proto, srcIp, payload) {
    const total = 20 + payload.length;
    const p = new Uint8Array(14 + total);
    p.set(this.eth(0x0800), 0);
    const ip = [0x45, 0, (total >> 8) & 0xff, total & 0xff, 0, 0, 0x40, 0, 64, proto, 0, 0,
                ...srcIp, 10, 0, 2, 15];
    p.set(ip, 14);
    const c = csum16(p, 14, 20);
    p[24] = c >> 8; p[25] = c & 0xff;
    p.set(payload, 34);
    return p;
  }

  handleIp(f) {
    const ihl = (f[14] & 0xf) * 4;
    const proto = f[23];
    const src = f.slice(26, 30), dst = f.slice(30, 34);
    const data = f.subarray(14 + ihl);
    if (proto === 17) this.handleUdp(dst, data);
    else if (proto === 6) this.handleTcp(src, dst, data, f, 14 + ihl);
    else if (proto === 1 && data[0] === 8) {
      // ICMP echo: any destination answers (fake ~0ms internet).
      const r = new Uint8Array(data);
      r[0] = 0; r[2] = 0; r[3] = 0;
      const c = csum16(r, 0, r.length);
      r[2] = c >> 8; r[3] = c & 0xff;
      this.sendFrame(this.ipPacket(1, dst, r));
    }
  }

  handleUdp(dstIp, u) {
    const dport = (u[2] << 8) | u[3];
    if (dport !== 53 || !ipEq(dstIp, 0, DNS_IP)) return;
    const q = u.subarray(8);
    // Parse one question: name, type, class.
    let i = 12; const labels = [];
    while (i < q.length && q[i] !== 0) { labels.push(String.fromCharCode(...q.subarray(i + 1, i + 1 + q[i]))); i += 1 + q[i]; }
    const name = labels.join('.').toLowerCase();
    const qtype = (q[i + 1] << 8) | q[i + 2];
    const qend = i + 5;
    let ip = this.dnsIps.get(name);
    if (!ip) {
      ip = [10, 1, (this.nextIp >> 8) & 0xff, this.nextIp & 0xff];
      this.nextIp++;
      this.dnsIps.set(name, ip);
      this.dnsNames.set(ip.join('.'), name);
    }
    this.log(`dns: ${name} -> ${ip.join('.')} (qtype ${qtype})`);
    const r = [q[0], q[1], 0x81, 0x80, 0, 1, 0, qtype === 1 ? 1 : 0, 0, 0, 0, 0,
               ...q.subarray(12, qend)];
    if (qtype === 1) r.push(0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, ...ip);
    const resp = new Uint8Array(8 + r.length);
    resp[0] = u[2]; resp[1] = u[3]; // src = 53
    resp[2] = u[0]; resp[3] = u[1]; // dst = guest's ephemeral port
    resp[4] = (resp.length >> 8) & 0xff; resp[5] = resp.length & 0xff;
    resp.set(r, 8); // UDP checksum 0 = none
    this.sendFrame(this.ipPacket(17, DNS_IP, resp));
  }

  // --- TCP ---

  tcpSend(conn, flags, payload = new Uint8Array(0), opts = []) {
    const hlen = 20 + opts.length;
    const t = new Uint8Array(hlen + payload.length);
    t[0] = conn.ourPort >> 8; t[1] = conn.ourPort & 0xff;
    t[2] = conn.guestPort >> 8; t[3] = conn.guestPort & 0xff;
    const seq = conn.sndNxt >>> 0, ack = conn.rcvNxt >>> 0;
    t[4] = seq >>> 24; t[5] = (seq >>> 16) & 0xff; t[6] = (seq >>> 8) & 0xff; t[7] = seq & 0xff;
    t[8] = ack >>> 24; t[9] = (ack >>> 16) & 0xff; t[10] = (ack >>> 8) & 0xff; t[11] = ack & 0xff;
    t[12] = (hlen / 4) << 4; t[13] = flags;
    t[14] = 0xff; t[15] = 0xff; // our receive window: plenty
    t.set(opts, 20);
    t.set(payload, hlen);
    // Checksum with IPv4 pseudo-header.
    const ph = new Uint8Array(12 + t.length);
    ph.set(conn.dstIp, 0); ph.set([10, 0, 2, 15], 4);
    ph[9] = 6; ph[10] = (t.length >> 8) & 0xff; ph[11] = t.length & 0xff;
    ph.set(t, 12);
    const c = csum16(ph, 0, ph.length);
    t[16] = c >> 8; t[17] = c & 0xff;
    this.sendFrame(this.ipPacket(6, conn.dstIp, t));
    conn.sndNxt = (conn.sndNxt + payload.length + ((flags & 0x03) ? 1 : 0)) >>> 0; // SYN/FIN consume a seq
  }

  rst(srcIp, dstIp, t) {
    // "Connection refused": RST|ACK acknowledging whatever arrived.
    const seq = ((t[4] << 24) | (t[5] << 16) | (t[6] << 8) | t[7]) >>> 0;
    const ack = ((t[8] << 24) | (t[9] << 16) | (t[10] << 8) | t[11]) >>> 0;
    const conn = {
      dstIp: Array.from(dstIp),
      ourPort: (t[2] << 8) | t[3], guestPort: (t[0] << 8) | t[1],
      sndNxt: ack, rcvNxt: (seq + 1) >>> 0,
    };
    this.tcpSend(conn, 0x14); // RST|ACK
  }

  handleTcp(srcIp, dstIp, t, frame, tcpOff) {
    const srcPort = (t[0] << 8) | t[1], dstPort = (t[2] << 8) | t[3];
    const flags = t[13];
    const seq = ((t[4] << 24) | (t[5] << 16) | (t[6] << 8) | t[7]) >>> 0;
    const ack = ((t[8] << 24) | (t[9] << 16) | (t[10] << 8) | t[11]) >>> 0;
    const window = (t[14] << 8) | t[15];
    const doff = (t[12] >> 4) * 4;
    const payload = t.subarray(doff);
    const key = `${srcPort}:${ipStr(dstIp, 0)}:${dstPort}`;

    if (flags & 0x02) { // SYN
      if (dstPort !== 80) { this.log(`tcp: RST non-http port ${dstPort}`); this.rst(srcIp, dstIp, t); return; }
      const conn = {
        dstIp: Array.from(dstIp), ourPort: dstPort, guestPort: srcPort,
        rcvNxt: (seq + 1) >>> 0, sndNxt: 1000, sndUna: 1000,
        window, req: [], sendBuf: null, sendOff: 0, finSent: false, dispatched: false,
      };
      this.conns.set(key, conn);
      this.tcpSend(conn, 0x12, new Uint8Array(0), [2, 4, MSS >> 8, MSS & 0xff]); // SYN/ACK + MSS
      return;
    }
    const conn = this.conns.get(key);
    if (!conn) { if (!(flags & 0x04)) this.rst(srcIp, dstIp, t); return; }
    conn.window = window;
    if (flags & 0x04) { this.conns.delete(key); return; } // RST from guest
    if (flags & 0x10) conn.sndUna = ack;
    if (payload.length && seq === conn.rcvNxt) {
      conn.rcvNxt = (conn.rcvNxt + payload.length) >>> 0;
      conn.req.push(...payload);
      this.tcpSend(conn, 0x10); // ACK
      this.maybeDispatch(key, conn);
    } else if (payload.length) {
      this.tcpSend(conn, 0x10); // dup/ooo: re-ACK current rcvNxt
    }
    if (flags & 0x01) { // FIN
      conn.rcvNxt = (conn.rcvNxt + 1) >>> 0;
      this.tcpSend(conn, 0x10);
      if (conn.finSent) this.conns.delete(key);
    }
    this.pumpSend(key, conn);
  }

  maybeDispatch(key, conn) {
    if (conn.sendBuf || conn.dispatched) return;
    const req = String.fromCharCode(...conn.req);
    const headEnd = req.indexOf('\r\n\r\n');
    if (headEnd < 0) return;
    const lines = req.slice(0, headEnd).split('\r\n');
    const [method, path] = lines[0].split(' ');
    let host = this.dnsNames.get(conn.dstIp.join('.'));
    for (const l of lines.slice(1)) {
      const m = l.match(/^host:\s*(.+?)(:\d+)?$/i);
      if (m && !host) host = m[1];
    }
    if (!method || !path || !host) return;
    conn.dispatched = true;
    const url = `https://${host}${path}`;
    this.log(`http: ${method} ${url}`);
    this.fetch(url, { method: method === 'HEAD' ? 'HEAD' : 'GET', redirect: 'follow' })
      .then(async (resp) => {
        const body = new Uint8Array(await resp.arrayBuffer());
        const ct = resp.headers.get('content-type') || 'application/octet-stream';
        const head = `HTTP/1.0 ${resp.status} ${resp.statusText || 'OK'}\r\ncontent-type: ${ct}\r\ncontent-length: ${body.length}\r\nconnection: close\r\n\r\n`;
        const hb = new TextEncoder().encode(head);
        const buf = new Uint8Array(hb.length + body.length);
        buf.set(hb, 0); buf.set(body, hb.length);
        conn.sendBuf = buf;
        this.pumpSend(key, conn);
      })
      .catch((err) => {
        this.log(`http error: ${err}`);
        const msg = new TextEncoder().encode(`HTTP/1.0 502 Bad Gateway\r\ncontent-type: text/plain\r\nconnection: close\r\n\r\ngateway fetch failed: ${err}\r\n`);
        conn.sendBuf = msg;
        this.pumpSend(key, conn);
      });
  }

  pumpSend(key, conn) {
    if (!conn.sendBuf) return;
    // Respect the guest's advertised window (no scaling was negotiated).
    while (conn.sendOff < conn.sendBuf.length) {
      const inflight = (conn.sndNxt - conn.sndUna) >>> 0;
      if (inflight >= conn.window) return;
      const n = Math.min(MSS, conn.sendBuf.length - conn.sendOff, conn.window - inflight);
      if (n <= 0) return;
      const chunk = conn.sendBuf.subarray(conn.sendOff, conn.sendOff + n);
      conn.sendOff += n;
      const last = conn.sendOff >= conn.sendBuf.length;
      this.tcpSend(conn, last ? 0x19 : 0x18, chunk); // PSH|ACK (+FIN on last)
      if (last) conn.finSent = true;
    }
  }
}

if (typeof module !== 'undefined') module.exports = { NetGateway };
