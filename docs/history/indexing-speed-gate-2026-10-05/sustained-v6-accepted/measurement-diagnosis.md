# Accepted sustained indexing calibration

Fixed source `f60dc4c09a264167c3b4d1d9a80c96a44c5c5a20`, healthy reference `ce2e9a54b6d448170cf692a54f06d66c4a3c958e`. Candidate medians must not exceed 1.50 times the slower same-job reference median; symmetric reference median drift must not exceed 1.25. MAX is diagnostic.

| Session | Role | Whole workflow |
| --- | --- | --- |
| 1 | Exploratory | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37396215246) |
| 2 | Exploratory | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37399375018) |
| 3 | Exploratory | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37403280562) |
| 4 | Held-out | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37406663374) |
| 5 | Held-out | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37410061779) |

All five attempt-1 runs completed all three groups. The first three whole passes were frozen before the two pre-reserved held-out sessions; no threshold, cadence, case, sample or outcome was refit. The 17 cells retain 8,190 raw leg rows, 680 observations (340 mandatory medians and 340 MAX diagnostics), 136 families with five observations each, and 45 distinct component UUIDs.

The largest candidate median ratio was 1.0804054967690284; the largest reference median drift was 1.1762021965319318. Candidate MAX ratio 1.4977394445506165 and reference tail ratio 1.5743504449359067 are diagnostic and do not change acceptance. Ratios describe individual comparisons; absolute timings from different runners are not pooled. No p95 or false-positive guarantee is inferred.

Source-before/after, execution context, work, request/body, oracle, process reaping/group absence and fixture cleanup passed. Across 15 jobs, maximum shared build was 713.42 seconds, shared measurement 2,035.06 seconds and job duration 2,758 seconds, below fixed 1,800/2,700/5,400 second limits. The serial campaign completed within 21,600 seconds.

The archive preserves 384 original files in 20,102,054 compressed bytes; every member was hash-checked after round-trip. The original F60 collector/contract/comparator reconstructed all 15 group comparisons with 30 CLI calls, retaining every sample and both statistic roles. [Manifest](evidence-manifest.json), [original-source replay](original-source-replay.json), [descriptive ratios](descriptive-summary.json) and [independent acceptance](calibration-acceptance-review.md) bind these results.

F60 was admitted only after [ordinary CI attempt 2](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37366863724/attempts/2) and [Guardian attempt 2](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37366753997/attempts/2) succeeded. Linux 1,588, macOS 1,579, Windows 1,611 and coverage 1,588 library tests had zero failures; integrations, native builds, Clippy and GNU updater E2E passed. Cargo audit correctly skipped unchanged Cargo inputs; the reference checkpoint's local audit remains separate evidence.

F60 attempt 1 failed before Detect Changes and Guardian acquired hosted runners; CI Gate failed closed on abandoned Detect Changes. [Original runner failure](../unmeasured-v6-pin-runner-ci/evidence-manifest.json) preserves 96 files, 77 reviewed subjects and three Git Python exports. The expired 60-minute recovery watch remains in [bounded stop](../unmeasured-v6-pin-runner-recovery-stop/evidence-manifest.json). A later user resume observed official Actions operational after that deadline; independent review allowed one full rerun of each exact source workflow. [Recovered-source evidence](../sustained-v6-recovered-source-ci/evidence-manifest.json) retains attempt 2, 33 reviewed subjects, three Git Python exports and source admission. No failed-only or third attempt, dummy commit, setting change or performance retry was used.

Acceptance covers this fixed source, workload and policy. Earlier rejected campaigns retain their original outcomes. Original C7 history, 7ED index-debt and D9 writer/helper causes remain unknown; successful diagnostics, native checks and calibration do not establish causal repair or absence of latent bugs. Normal-path test-writer observations are shared by all new RCR roles; original-AFDD full test-program/binary equivalence is not claimed.

Default activation is a separate code/TDD checkpoint. Exact published CI, active hosted proof, overall review, protected merge and master-source proof are tracked in the PR and subsequent exact runs; this calibration archive remains inactive original evidence.
