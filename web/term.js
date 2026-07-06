// Minimal VT100/ANSI screen model — enough for BusyBox vi/top over a serial
// console: cursor addressing, erase, insert/delete line, SGR (colors,
// inverse), scroll region, and the ESC[6n cursor-position report (vi uses it
// to size the terminal). No DOM in this file so it runs under Node for
// headless verification; rendering lives in index.html.
'use strict';

class Screen {
  constructor(cols = 80, rows = 24) {
    this.cols = cols;
    this.rows = rows;
    this.reset();
    this.replies = []; // bytes the terminal sends back (e.g. CPR)
  }

  reset() {
    this.grid = [];
    for (let r = 0; r < this.rows; r++) this.grid.push(this.blankRow());
    this.cx = 0;
    this.cy = 0;
    this.sgr = { fg: null, bg: null, bold: false, inv: false };
    this.top = 0;
    this.bot = this.rows - 1;
    this.state = 'norm';
    this.params = '';
    this.cursorVisible = true;
    this.utfNeed = 0; // pending UTF-8 continuation bytes
    this.utfCp = 0;
  }

  blankCell() { return { ch: ' ', fg: null, bg: null, bold: false, inv: false }; }
  blankRow() { const r = []; for (let c = 0; c < this.cols; c++) r.push(this.blankCell()); return r; }

  scrollUp() { this.grid.splice(this.top, 1); this.grid.splice(this.bot, 0, this.blankRow()); }
  scrollDown() { this.grid.splice(this.bot, 1); this.grid.splice(this.top, 0, this.blankRow()); }

  put(ch) {
    if (this.cx >= this.cols) {
      this.cx = 0;
      this.cy++;
      if (this.cy > this.bot) { this.cy = this.bot; this.scrollUp(); }
    }
    const cell = this.grid[this.cy][this.cx];
    cell.ch = ch;
    cell.fg = this.sgr.fg;
    cell.bg = this.sgr.bg;
    cell.bold = this.sgr.bold;
    cell.inv = this.sgr.inv;
    this.cx++;
  }

  lf() { if (this.cy === this.bot) this.scrollUp(); else if (this.cy < this.rows - 1) this.cy++; }

  csi(finalByte) {
    const p = this.params.split(';').map(s => (s === '' ? NaN : parseInt(s, 10)));
    const n = isNaN(p[0]) ? 1 : p[0];
    const m = isNaN(p[1]) ? 1 : p[1];
    switch (finalByte) {
      case 'A': this.cy = Math.max(this.top, this.cy - n); break;
      case 'B': this.cy = Math.min(this.bot, this.cy + n); break;
      case 'C': this.cx = Math.min(this.cols - 1, this.cx + n); break;
      case 'D': this.cx = Math.max(0, this.cx - n); break;
      case 'G': this.cx = Math.min(this.cols - 1, Math.max(0, n - 1)); break;
      case 'd': this.cy = Math.min(this.rows - 1, Math.max(0, n - 1)); break;
      case 'H': case 'f':
        this.cy = Math.min(this.rows - 1, Math.max(0, n - 1));
        this.cx = Math.min(this.cols - 1, Math.max(0, m - 1));
        break;
      case 'J': {
        const mode = isNaN(p[0]) ? 0 : p[0];
        if (mode === 0) {
          for (let c = this.cx; c < this.cols; c++) this.grid[this.cy][c] = this.blankCell();
          for (let r = this.cy + 1; r < this.rows; r++) this.grid[r] = this.blankRow();
        } else if (mode === 1) {
          for (let c = 0; c <= this.cx; c++) this.grid[this.cy][c] = this.blankCell();
          for (let r = 0; r < this.cy; r++) this.grid[r] = this.blankRow();
        } else {
          for (let r = 0; r < this.rows; r++) this.grid[r] = this.blankRow();
        }
        break;
      }
      case 'K': {
        const mode = isNaN(p[0]) ? 0 : p[0];
        if (mode === 0) for (let c = this.cx; c < this.cols; c++) this.grid[this.cy][c] = this.blankCell();
        else if (mode === 1) for (let c = 0; c <= this.cx; c++) this.grid[this.cy][c] = this.blankCell();
        else this.grid[this.cy] = this.blankRow();
        break;
      }
      case 'L': for (let i = 0; i < n; i++) { this.grid.splice(this.bot, 1); this.grid.splice(this.cy, 0, this.blankRow()); } break;
      case 'M': for (let i = 0; i < n; i++) { this.grid.splice(this.cy, 1); this.grid.splice(this.bot, 0, this.blankRow()); } break;
      case 'P': { // delete chars
        const row = this.grid[this.cy];
        row.splice(this.cx, n);
        while (row.length < this.cols) row.push(this.blankCell());
        break;
      }
      case '@': { // insert chars
        const row = this.grid[this.cy];
        for (let i = 0; i < n; i++) row.splice(this.cx, 0, this.blankCell());
        row.length = this.cols;
        break;
      }
      case 'r':
        this.top = Math.max(0, (isNaN(p[0]) ? 1 : p[0]) - 1);
        this.bot = Math.min(this.rows - 1, (isNaN(p[1]) ? this.rows : p[1]) - 1);
        this.cy = this.top; this.cx = 0;
        break;
      case 'm': {
        const ps = this.params === '' ? [0] : this.params.split(';').map(s => (s === '' ? 0 : parseInt(s, 10)));
        for (const v of ps) {
          if (v === 0) this.sgr = { fg: null, bg: null, bold: false, inv: false };
          else if (v === 1) this.sgr.bold = true;
          else if (v === 7) this.sgr.inv = true;
          else if (v === 22) this.sgr.bold = false;
          else if (v === 27) this.sgr.inv = false;
          else if (v >= 30 && v <= 37) this.sgr.fg = v - 30;
          else if (v === 39) this.sgr.fg = null;
          else if (v >= 40 && v <= 47) this.sgr.bg = v - 40;
          else if (v === 49) this.sgr.bg = null;
        }
        break;
      }
      case 'n':
        if (n === 6) {
          const rep = `\x1b[${this.cy + 1};${this.cx + 1}R`;
          for (const ch of rep) this.replies.push(ch.charCodeAt(0));
        }
        break;
      case 'h': case 'l':
        if (this.params === '?25') this.cursorVisible = finalByte === 'h';
        break;
      default: break; // ignore the rest
    }
  }

  write(bytes) {
    for (const b of bytes) {
      if (this.state === 'norm') {
        if (this.utfNeed > 0 && b >= 0x80 && b < 0xc0) {
          this.utfCp = (this.utfCp << 6) | (b & 0x3f);
          if (--this.utfNeed === 0 && this.utfCp >= 0x20) this.put(String.fromCodePoint(this.utfCp));
          continue;
        }
        this.utfNeed = 0;
        if (b === 0x1b) this.state = 'esc';
        else if (b === 0x0a) this.lf();
        else if (b === 0x0d) this.cx = 0;
        else if (b === 0x08) this.cx = Math.max(0, this.cx - 1);
        else if (b === 0x09) this.cx = Math.min(this.cols - 1, (this.cx & ~7) + 8);
        else if (b === 0x07) { /* bell */ }
        else if (b >= 0xf0 && b < 0xf8) { this.utfNeed = 3; this.utfCp = b & 0x07; }
        else if (b >= 0xe0) { this.utfNeed = 2; this.utfCp = b & 0x0f; }
        else if (b >= 0xc0) { this.utfNeed = 1; this.utfCp = b & 0x1f; }
        else if (b >= 0x20 && b < 0x80) this.put(String.fromCharCode(b));
      } else if (this.state === 'esc') {
        if (b === 0x5b /* [ */) { this.state = 'csi'; this.params = ''; }
        else if (b === 0x4d /* M reverse index */) { if (this.cy === this.top) this.scrollDown(); else this.cy--; this.state = 'norm'; }
        else if (b === 0x28 || b === 0x29) this.state = 'charset';
        else this.state = 'norm';
      } else if (this.state === 'charset') {
        this.state = 'norm';
      } else if (this.state === 'csi') {
        const ch = String.fromCharCode(b);
        if ((b >= 0x30 && b <= 0x3f)) this.params += ch;
        else if (b >= 0x20 && b <= 0x2f) { /* intermediate, ignore */ }
        else { this.csi(ch); this.state = 'norm'; }
      }
    }
  }

  takeReplies() { const r = this.replies; this.replies = []; return r; }

  toText() {
    return this.grid.map(row => row.map(c => c.ch).join('').replace(/\s+$/, '')).join('\n');
  }
}

if (typeof module !== 'undefined') module.exports = { Screen };
