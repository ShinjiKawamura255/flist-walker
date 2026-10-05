# Indexing speed comparison calibration, 2026-10-05

This record retains a rejected null comparison and its sampling refinement. Current operation belongs to [CI Operations](../CI_OPERATIONS.md); [TC-233](../testplan/indexing-contention.md#same-job-reference-comparison-tooling-tc-233) owns the comparison contract. Numeric enforcement remains inactive at this measurement checkpoint.

The original candidate source is `2b11f361fcfc9ce0f7f7ad7e2728b2a3b1ae0a55`, fixed healthy reference `afdd0c4e6b4c97a270e737ed9e14a2a694db2d7e`. Rust/build inputs are byte-identical for the null construction. Execution workflow/job/image identities remain those of the candidate collector; measured reference source is separate. The original seven-pair R1→C→R2 protocol has17cells,714 raw leg rows and136 numeric decisions. Its other27 extension cells and native input/frame behavior are separate coverage.

[Run37245833397](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37245833397) completed successfully as comparison-calibration collection. Every source/work/process/fixture admission passed. Numeric proposal outcomes were f1 **indeterminate**, matched **PASS**, stable **PASS**. Collection success is not numeric acceptance. The whole null session is rejected; reserved sessions2–5 were not dispatched and none of these values calibrates the revised sampling design.

| Original group | Decisions | Worst candidate/slower-reference ratio | Worst reference drift | Proposal |
| --- | ---: | ---: | ---: | --- |
| f1 |64|1.091|1.277|indeterminate|
| matched |24|1.001|1.096|PASS|
| stable |48|1.001|1.017|PASS|

The rejected f1 key is F1-ignore-list/FileList/condition, both index-ready and results-ready median: R1=292.159ms, C=293.064ms, R2=372.947ms. B0 medians stay near211ms and condition producer publication near205ms. Condition completion spans17–30 simulated GUI frames, with the R2 median five frames above R1/C. Sampling and condition-specific GUI work can explain the difference; these observations do not identify the internal cause or prove CPU drift. No candidate timing key exceeded its1.50 limit.

The predeclared engineering predicate uses the slower bracket reference: candidate strictly above1.50×that statistic fails; symmetric reference drift strictly above1.25 is indeterminate. Equality passes. Median represents sustained completion and maximum represents tail behavior. B0 and condition are checked independently, so shared slowdown cannot cancel. This is no percentile or false-positive guarantee. Candidate-only host interference remains a possible failure cause.

The refinement keeps100k, all cells/roles/arms, ABBA, oracles, runtime defaults and those limits. Every f1 cell now selects21 pairs through the existing Rust test selector; matched/stable keep7. Both median and maximum include all21 samples, including the later14. The new protocol is `same-job-RCR-f1-21-v2`:17cells,1386raw leg rows and136 decisions. The entire three-exploratory/two-held-out reservation restarts on validated revised source before new outcomes. Larger samples do not guarantee stability and can increase maximum variation.

Current f1 measurements took230.1/228.1/229.2seconds. A conservative threefold planning estimate totals34.4minutes; shared45-minute measurement,30-minute build and90-minute job bounds remain unchanged. This estimate is not an upper-bound proof. Local synthetic regressions first failed3 tests on the old cadence, then all102 Python tests passed after the repair. They verify fixed group cadence, rejection of wrong counts and later-pair median/maximum contribution; invented rows do not claim actual measured performance.

Pre-layout [pilot37243826563](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37243826563), source `8b394b738a9810e87abf98be6154c355542813d7`, was excluded before timing outcomes because source-role paths had unequal lengths. Its three groups later passed collection and numeric proposal, but it remains diagnostic and does not enter calibration. Equal-length opaque paths keep that bias out of the source2b attempt.

[The original-byte manifest](indexing-speed-gate-2026-10-05/seven-pair-evidence-manifest.json) binds142 archived files, including both original attempts, reservations, stopped status, component receipts, raw/build/discovery/setup/summary logs and source-matching Python contracts. The [archive](indexing-speed-gate-2026-10-05/seven-pair-campaign-original-artifacts.tar.gz) was roundtrip verified. [Original-contract replay](indexing-speed-gate-2026-10-05/seven-pair-roundtrip-replay.json) independently reproduces f1 indeterminate and other groups PASS. Extract to a fresh directory and run the archived `source-contract/scripts/indexing_perf.py` on the corresponding artifact folder. Calibration `validate` exit0 retains `timing_gate=null`; `compare-proposal` returns1 for f1. Old seven-pair data is not reinterpreted as21 pairs. Committed archives preserve evidence beyond GitHub's14-day artifact retention.

Revised-cadence hosted measurements, held-out acceptance, numeric activation and native GUI tests are **NOT RUN** at this checkpoint. Product Rust, workflows, trusted checker and repository settings are unchanged by this sampling repair.
