# extras/ — post-charter demo extensions

The charter's gate ladder completed 2026-07-04 (see REPORTS.md). Work here extends the demo beyond the charter's scope under these rules, agreed with the operator:

- The certified target images and their recipes in `targets/` are never modified; every charter certification (riscv-tests, RISCOF, lockstep, boot layers) remains re-runnable against them at any time.
- `harness/` stays frozen. Extras get their own verification scripts here — additive checks, never replacements.
- Extras images are separate pinned recipes (Docker layers on top of the pinned base image where possible) with their own version pins recorded.
- Where an extra makes Spike lockstep impossible in principle (e.g., devices Spike doesn't model), that loss is stated in the recipe header rather than papered over.
