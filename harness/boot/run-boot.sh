#!/bin/bash
# Layer 4: boot conformance. Boots targets/<name>.image under rvemu and
# checks the console transcript against boot/<name>.expect (an expect(1)
# script performing exact prompt/output matching, driving scripted input).
# Usage: run-boot.sh <name>   Exit 0 iff the full scripted sequence matches.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
NAME="${1:?usage: run-boot.sh <name>}"
RVEMU="${RVEMU:-$ROOT/target/release/rvemu}"
SCRIPT="$HERE/$NAME.expect"
[ -f "$SCRIPT" ] || { echo "no expect script $SCRIPT"; exit 2; }
exec expect -f "$SCRIPT" "$RVEMU" "$ROOT"
