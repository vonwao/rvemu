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
