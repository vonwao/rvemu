#!/usr/bin/env python3
# Self-test scaffolding only: presents the rvemu CLI contract but executes on
# the pinned Spike, so the layer-1 runner's counting/reporting logic can be
# verified before the real emulator exists. A hung (corrupted) binary is
# killed after a timeout and reported as failure, mirroring the contract's
# budget-exhaustion exit.
import subprocess
import sys
import os

args = sys.argv[1:]
elf = args[-1]
spike = os.path.join(os.path.dirname(__file__), "..", "..", "targets", "vendor", "spike", "install", "bin", "spike")
try:
    r = subprocess.run(
        [spike, "--isa=rv64imac_zicsr_zifencei", elf],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=20,
    )
    sys.exit(r.returncode)
except subprocess.TimeoutExpired:
    sys.exit(2)
