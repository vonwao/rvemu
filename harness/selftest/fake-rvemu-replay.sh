#!/bin/bash
# Self-test scaffolding only: presents the rvemu CLI contract but "executes"
# by replaying a pre-recorded canonical trace (REPLAY_TRACE) into the --trace
# path, so lockstep.sh's FIFO plumbing can be verified end to end.
set -u
trace_out=""
while [ $# -gt 1 ]; do
  case "$1" in
    --trace) trace_out="$2"; shift 2;;
    --max-insns) shift 2;;
    *) shift;;
  esac
done
cat "${REPLAY_TRACE:?}" > "$trace_out"
exit 0
