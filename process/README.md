# Process instrumentation

Descriptive record of how this project was built, maintained continuously from the day-0 freeze through Gate C. Three files:

- `timeline.md` — dated entries of what happened, in order. A "green" claim here means nothing unless the harness certified it; every pass/fail statement must be traceable to a harness run. This record never becomes a second source of truth: `REPORTS.md` and the harness outputs are authoritative, this is narration of them.
- `divergences.md` — every lockstep divergence (and equivalent harness catch) with its evidence: instruction index, opcode, reference-vs-dut state, and what the fix was. This is the harness doing its job.
- `failures.md` — honest to the point of unflattering: wrong turns, dead ends, misdiagnoses, and explicitly any moment where the easy path was to weaken a test or comparison and what was done instead.

Rule inherited from the charter: nothing in `harness/` or the target pins changes to make an entry here look better.
