#!/bin/bash
# Harness self-test (charter §8 step 4): prove each harness layer catches a
# known-bad input before any of it is trusted. Exit 0 iff every check passes.
#
# Checks:
#  1  comparator: identical traces -> OK (exit 0)
#  2  comparator: single wrong instruction in DUT trace -> DIVERGENCE at
#     exactly that instruction index (exit 1)
#  3  comparator: truncated DUT trace -> PREFIX-CLEAN early end (exit 3)
#  4  lockstep.sh FIFO plumbing end to end with a replaying fake emulator
#  5  layer-1 runner: correct pass counting on a real group (executor: Spike
#     behind the rvemu CLI), and a corrupted binary reported FAIL
#
# The remaining charter check — a real rvemu build patched with a deliberate
# one-instruction bug being flagged at exactly that instruction — runs as
# soon as the first rvemu exists (Gate A start) and its output is recorded in
# REPORTS.md. Nothing in the harness changes for it.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
SPIKE="$ROOT/targets/vendor/spike/install/bin/spike"
DIFF="$ROOT/harness/lockstep/diff/target/release/lockstep-diff"
ISA_DIR="$ROOT/targets/vendor/riscv-tests/isa"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/selftest.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
fails=0

check() { # name expected_rc actual_rc extra_ok
  local name="$1" want="$2" got="$3" ok="${4:-1}"
  if [ "$got" = "$want" ] && [ "$ok" = "1" ]; then
    echo "SELFTEST PASS: $name"
  else
    echo "SELFTEST FAIL: $name (exit $got, wanted $want; predicate=$ok)"
    fails=$((fails+1))
  fi
}

# --- Produce a reference commit log and its canonical replica.
"$SPIKE" --isa=rv64imac_zicsr_zifencei --log-commits --log="$WORK/ref.log" "$ISA_DIR/rv64ui-p-add" >/dev/null 2>&1
"$DIFF" --canonicalize "$WORK/ref.log" > "$WORK/dut-good.trace"
NLINES=$(wc -l < "$WORK/dut-good.trace" | tr -d ' ')

# 1: identical traces.
"$DIFF" "$WORK/ref.log" "$WORK/dut-good.trace" --ref-format spike > "$WORK/out1"; rc=$?
check "identical traces -> OK" 0 $rc

# 2: single wrong instruction at line 40 (simulated one-instruction emulator bug).
awk 'NR==40{$0=$0" x31=0xdeadbeefdeadbeef"}1' "$WORK/dut-good.trace" > "$WORK/dut-bad.trace"
"$DIFF" "$WORK/ref.log" "$WORK/dut-bad.trace" --ref-format spike > "$WORK/out2"; rc=$?
grep -q "DIVERGENCE at instruction 40:" "$WORK/out2"; loc=$((1-$?))
check "wrong instruction flagged at exact index 40" 1 $rc $loc

# 3: truncated DUT trace.
head -n $((NLINES/2)) "$WORK/dut-good.trace" > "$WORK/dut-trunc.trace"
"$DIFF" "$WORK/ref.log" "$WORK/dut-trunc.trace" --ref-format spike > "$WORK/out3"; rc=$?
check "truncated dut trace -> prefix-clean early end" 3 $rc

# 4: lockstep.sh end-to-end with the replaying fake emulator (FIFO plumbing).
REPLAY_TRACE="$WORK/dut-good.trace" RVEMU="$HERE/fake-rvemu-replay.sh" \
  "$ROOT/harness/lockstep/lockstep.sh" "$ISA_DIR/rv64ui-p-add" > "$WORK/out4" 2>&1; rc=$?
check "lockstep.sh FIFO plumbing (identical replay)" 0 $rc

REPLAY_TRACE="$WORK/dut-bad.trace" RVEMU="$HERE/fake-rvemu-replay.sh" \
  "$ROOT/harness/lockstep/lockstep.sh" "$ISA_DIR/rv64ui-p-add" > "$WORK/out4b" 2>&1; rc=$?
grep -q "DIVERGENCE at instruction 40:" "$WORK/out4b"; loc=$((1-$?))
check "lockstep.sh flags bad replay at index 40" 1 $rc $loc

# 5a: layer-1 runner counts a fully passing group.
RVEMU="$HERE/spike-as-rvemu.py" "$ROOT/harness/run-riscv-tests.sh" rv64um > "$WORK/out5" 2>&1; rc=$?
grep -q "rv64um: 26/26" "$WORK/out5"; loc=$((1-$?))
check "runner: rv64um 26/26 via reference executor" 0 $rc $loc

# 5b: corrupted test binary reported FAIL. Two corruptions: a flipped
# instruction word deep in .text, and a truncated ELF.
mkdir -p "$WORK/isa-corrupt"
python3 - "$ISA_DIR/rv64ui-p-add" "$WORK/corrupt-flip" <<'EOF'
import sys
data = bytearray(open(sys.argv[1], 'rb').read())
# .text starts at file offset 0x1000 in these ELFs; corrupt a word well past
# the prologue, inside the test cases.
off = 0x1000 + 0x200
data[off] ^= 0xff
open(sys.argv[2], 'wb').write(bytes(data))
EOF
head -c 200 "$ISA_DIR/rv64ui-p-add" > "$WORK/corrupt-trunc"
"$HERE/spike-as-rvemu.py" --max-insns 10000000 "$WORK/corrupt-flip" >/dev/null 2>&1; rc1=$?
"$HERE/spike-as-rvemu.py" --max-insns 10000000 "$WORK/corrupt-trunc" >/dev/null 2>&1; rc2=$?
ok=1; [ $rc1 -eq 0 ] && ok=0; [ $rc2 -eq 0 ] && ok=0
check "corrupted binaries reported FAIL (flip rc=$rc1, trunc rc=$rc2)" 1 1 $ok

echo
if [ $fails -eq 0 ]; then
  echo "SELFTEST: all checks passed"
  exit 0
else
  echo "SELFTEST: $fails check(s) FAILED"
  exit 1
fi
