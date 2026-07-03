#!/bin/bash
# Layer 3: run the pinned Spike and rvemu on the same ELF, compare per-retired-
# instruction state via lockstep-diff. See harness/README.md for exit codes.
# Usage: lockstep.sh <elf> [max-insns]
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SPIKE="$ROOT/targets/vendor/spike/install/bin/spike"
RVEMU="${RVEMU:-$ROOT/target/release/rvemu}"
DIFF="$ROOT/harness/lockstep/diff/target/release/lockstep-diff"
ISA=rv64imac_zicsr_zifencei

ELF="${1:?usage: lockstep.sh <elf> [max-insns]}"
MAX="${2:-100000000}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/lockstep.XXXXXX")"
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$WORK"' EXIT
mkfifo "$WORK/ref.fifo" "$WORK/dut.fifo"

"$SPIKE" --isa=$ISA --instructions="$MAX" --log-commits --log="$WORK/ref.fifo" "$ELF" >"$WORK/ref.console" 2>"$WORK/ref.err" &
SPIKE_PID=$!
"$RVEMU" --max-insns "$MAX" --trace "$WORK/dut.fifo" "$ELF" >"$WORK/dut.console" 2>"$WORK/dut.err" &
RVEMU_PID=$!

"$DIFF" "$WORK/ref.fifo" "$WORK/dut.fifo" --ref-format spike
RESULT=$?

# The comparator exiting can leave a producer blocked on a full FIFO; reap it.
kill $SPIKE_PID $RVEMU_PID 2>/dev/null
wait $SPIKE_PID 2>/dev/null; SPIKE_RC=$?
wait $RVEMU_PID 2>/dev/null; RVEMU_RC=$?

echo "spike exit: $SPIKE_RC  rvemu exit: $RVEMU_RC  comparator exit: $RESULT"
[ -s "$WORK/dut.err" ] && { echo "rvemu stderr:"; cat "$WORK/dut.err"; }
[ -s "$WORK/ref.err" ] && { echo "spike stderr:"; cat "$WORK/ref.err"; }
exit $RESULT
