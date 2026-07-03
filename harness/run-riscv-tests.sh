#!/bin/bash
# Layer 1: run riscv-tests ISA binaries under rvemu, report X/Y per group.
# Usage: run-riscv-tests.sh [group...]   (default: the six charter groups)
# Exit 0 iff every test in every requested group passes.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ISA="$ROOT/targets/vendor/riscv-tests/isa"
RVEMU="${RVEMU:-$ROOT/target/release/rvemu}"
GROUPS_LIST=("${@:-rv64ui rv64um rv64ua rv64uc rv64mi rv64si}")
[ $# -eq 0 ] && GROUPS_LIST=(rv64ui rv64um rv64ua rv64uc rv64mi rv64si)

overall=0
for g in "${GROUPS_LIST[@]}"; do
  pass=0; total=0; fails=()
  for t in "$ISA/$g"-p-* "$ISA/$g"-v-*; do
    [ -f "$t" ] || continue
    case "$t" in *.dump) continue;; esac
    total=$((total+1))
    if "$RVEMU" --max-insns 10000000 "$t" >/dev/null 2>&1; then
      pass=$((pass+1))
    else
      fails+=("$(basename "$t")")
    fi
  done
  echo "$g: $pass/$total"
  for f in "${fails[@]:-}"; do [ -n "$f" ] && echo "  FAIL $f"; done
  [ "$pass" -ne "$total" ] && overall=1
done
exit $overall
