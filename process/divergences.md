# Lockstep divergences and harness catches

Each entry: what the harness reported (verbatim where captured), the diagnosis, the fix. Emulator-side fixes only — the comparator was never adjusted.

## 2026-07-03 — Planted bug (harness self-test, charter §8 step 4)

A deliberately wrong rvemu build (`add` computing `a+b+1`) was run under lockstep on `rv64ui-p-add` to prove the harness catches a known bug before being trusted:

```
DIVERGENCE at instruction 80:
  reference: p0 000000008000019c 00c58733 x14=0x0000000000000000
  dut:       p0 000000008000019c 00c58733 x14=0x0000000000000001
```

Instruction 80 is the first `add` (opcode `00c58733`, add x14,x11,x12) the test executes; the off-by-one is visible in the x14 writeback. Exactly the planted location. The clean binary was rebuilt and re-verified (test exits PASS) afterwards.

## 2026-07-03 — Load-trace token order (real bug #1)

First-ever real lockstep run (`rv64ui-p-add`):

```
DIVERGENCE at instruction 4:
  reference: p3 000000000000100c 0182b283 x5=0x0000000080000000 m:0x0000000000001018
  dut:       p3 000000000000100c 0182b283 m:0x0000000000001018 x5=0x0000000080000000
```

Spike's commit log emits the register writeback before the memory-address annotation on loads; rvemu emitted them in the reverse order. Fix: trace the load address after the register write in the load/LR paths.

## 2026-07-03 — mret/sret mstatus commit annotation (real bug #2)

Same test, next divergence:

```
DIVERGENCE at instruction 76:
  reference: p3 000000008000018c 30200073 c:mstatus=0x0000000a00000080
  dut:       p3 000000008000018c 30200073
```

Spike logs the mstatus side-effect of `mret` (opcode `30200073`) as a CSR commit — unlike trap *entry*, which produces no line at all. rvemu now emits `c:mstatus=` from mret/sret. (The value `0xa00000080` also confirmed UXL/SXL read-only-64 bits and MPIE behavior matched.)

## 2026-07-03 — c.lui with rd=x0 (real bug #3, found via RISCOF then pinned by lockstep)

RISCOF flagged `rv64i_m/C/src/clui-01.S` as the only C-extension signature mismatch. Lockstep on the compiled test:

```
LOCKSTEP PREFIX-CLEAN: dut ended after 180 instructions; reference continues:
  p3 0000000080000274 00006005
```

Opcode `6005` is `c.lui x0, 1`. rvemu treated it as reserved/illegal (rd=0), so the test trapped and wrote a FAIL tohost early; the pinned Spike executes it as a HINT (retires, writes nothing). Fix: expand c.lui with rd=x0 to `lui x0` (retires as a no-op write). RISCOF then went 136/136.

## Non-lockstep harness catches worth recording

- **Budget bug A:** an arch-test writing `minstret` defeated `--max-insns` (budget compared against the now-writable CSR). Found because a RISCOF DUT run burned 5+ CPU-minutes on a 30M budget. Fix: budget counts retirements independently of the CSR.
- **Budget bug B:** a trap loop (traps retire nothing) never decremented a retirement-based budget, hanging a RISCOF run. Fix: budget bounds *steps* (attempted instructions), matching Spike's `--instructions` semantics.

# Gate B (2026-07-03, afternoon)

## medeleg WARL mask (lockstep, instruction 33)

First xv6 lockstep run:

```
DIVERGENCE at instruction 33:
  reference: p3 000000008000008e 30279073 c:medeleg=0x000000000000b3fe
  dut:       p3 000000008000008e 30279073 c:medeleg=0x000000000000b3ff
```

Spike's medeleg write mask excludes bit 0 (instruction-address-misaligned — unreachable with C). Fix: mask 0xb3ff → 0xb3fe. Neither riscv-tests nor RISCOF caught this; only lockstep did.

## sie writes log two tokens (lockstep, instruction 37)

```
  reference: p3 000000008000009e 10479073 c:sie=0x...220 c:mie=0x...220
  dut:       p3 000000008000009e 10479073 c:mie=0x...220
```

Spike logs sie/sip writes as the alias view followed by the backing register; sstatus logs only the backing mstatus. Trace emission matched to the observed convention.

## mstatus.FS writable without F (lockstep on rv64ui-v-add, instruction 115)

```
  reference: ... c:mstatus=0x8000000a00006000
  dut:       ... c:mstatus=0x0000000a00000000
```

Spike keeps FS writable (and derives SD) even on an FP-less core; rvemu had FS hardwired 0. Fixed, with SD = (FS == dirty).

## tohost matched on virtual, not physical, address (lockstep on rv64ui-v-add)

Prefix-clean 8207 instructions, then dut kept spinning at the test's final `j .` while the reference exited: the -v environment stores to tohost through a virtual mapping and rvemu compared the pre-translation address. HTIF match moved to the translated physical address.

## The RTC quantum (lockstep, instruction 410,787,976)

After 410.8M identical instructions:

```
  reference: p1 0000000080002468 c01027f3 x15=0x00000000003eae7c
  dut:       p1 0000000080002468 c01027f3 x15=0x00000000003eae67
```

A `csrr time` in xv6's first clockintr, off by 21 ticks. Root cause dug out of the vendored Spike source (sim.cc/execute.cc): mtime does not advance per-instruction — it advances **+50 per completed 5000-instruction quantum**, where only retired instructions consume quantum units, and **any trap, interrupt delivery, or wfi ends the processor's slice early while the sim loop counts the full slice** (so a trap effectively consumes the quantum remainder). Idle wfi quanta also bump minstret by 1 each (a Spike quirk, replicated). Two iterations were needed: per-retirement quanta alone still read 50 low at the same instruction — the missing piece was the first timer interrupt (delivered ~200 instructions earlier) consuming its slice remainder.

Result: the next run was **prefix-clean over 423,107,530 instructions** (reference ended at its step budget, zero divergences), covering M/S transitions, Sv39 translation with Svade, timer interrupt delivery, and the full xv6 boot path.
