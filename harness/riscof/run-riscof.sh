#!/bin/bash
# Layer 2: RISCOF architectural compliance, rvemu (DUT) vs pinned Spike
# (reference), on the pinned riscv-arch-test suite. Prints per-extension
# pass/fail summary. Exit 0 iff every selected test's signature matches.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
VENV="$ROOT/targets/vendor/riscof-venv"
SUITE="$ROOT/targets/vendor/riscv-arch-test/riscv-test-suite"
WORK="$ROOT/work/riscof"
export PATH="$ROOT/targets/vendor/xpack-riscv-none-elf-gcc-15.2.0-1/bin:$PATH"

mkdir -p "$WORK"
cd "$HERE"
"$VENV/bin/riscof" run --config=config.ini --suite="$SUITE" --env="$SUITE/env" \
  --work-dir="$WORK" --no-browser 2>&1 | tee "$WORK/riscof-console.log"
RC=${PIPESTATUS[0]}

# Per-extension summary from the test_list + result report.
python3 - "$WORK" <<'EOF'
import re, sys, os, collections
work = sys.argv[1]
log = open(os.path.join(work, 'riscof-console.log')).read()
results = re.findall(r'(\S+)\s*:\s*(Passed|Failed)', log)
by_ext = collections.defaultdict(lambda: [0, 0])
for name, verdict in results:
    m = re.search(r'/rv64i_m/([^/]+)/', name)
    ext = m.group(1) if m else 'other'
    by_ext[ext][0] += (verdict == 'Passed')
    by_ext[ext][1] += 1
for ext in sorted(by_ext):
    p, t = by_ext[ext]
    print(f"riscof {ext}: {p}/{t} {'PASS' if p == t else 'FAIL'}")
EOF
exit $RC
