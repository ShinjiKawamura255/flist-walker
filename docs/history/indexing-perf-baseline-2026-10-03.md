# Indexing performance baseline — 2026-10-03

Point-in-time macOS/arm64 headless observations; this does not certify current HEAD, native input, or Windows/Linux. [Runner and interpretation](../testplan/indexing-contention.md).

## Method

Rust1.97.1, original tag Cargo.lock/default features, independent target directories, local warm-cache filesystem,16ms production update/render cycles,100,000 real-file candidates per source,7 AB/BA pairs after warmup. Cold tracev4 installs a held query at10% ingested; S2 edits at25% and clears at50%. Fixture/oracle construction is outside t0. Test-only adapters do not backport production fixes.

Full contention requires successful request-owned settlement, complete membership/latest results, and actual evaluated search overlap (two distinct requests for S2). Queued/pre-canceled requests do not qualify.

## Tagged basic results

| Tag | Normal guards | Cold query conditions (6) | Retained raw | B0 t2 median FileList / Walker |
| --- | --- | --- | ---: | --- |
|v0.27.0|7 PASS /2 ignored|0 PASS /6 NON_ELIGIBLE|28|253.9 /993.7ms|
|v0.28.0|7 PASS /2 ignored|0 PASS /6 NON_ELIGIBLE|28|256.3 /992.8ms|
|v0.29.0|7 PASS /2 ignored|0 PASS /6 NON_ELIGIBLE|28|253.7 /979.3ms|
|v0.30.0|7 PASS /2 ignored|6 PASS /0 NON_ELIGIBLE|84|257.1 /1001.4ms|

v0.27/28/29 defer nonempty search until the committed candidate Arc is available. The empty-startup cold trace produces no substantive worker overlap. Eligibility checks exit101 and are **NON_ELIGIBLE**, not indexing-speed failures/improvements. Their B0 controls are seven AA pairs per source (both positions B0). v30 has21 B0 samples per source across three comparisons; these aggregates are descriptive, not paired version ratios. This profile does not reproduce or refute the reported v29 slowdown.

## Eligible observations

| Revision | Source | Case | Case t2 median / max | Case t3 median / max | Median paired t2 ratio |
| --- | --- | --- | --- | --- | --- |
|v0.30.0|FileList|S1-selective|279.1 /319.4ms|303.4 /362.7ms|1.103|
|v0.30.0|FileList|S1-dense|277.1 /285.9ms|302.6 /307.6ms|1.102|
|v0.30.0|FileList|S2|272.2 /282.6ms|272.2 /282.6ms|1.018|
|v0.30.0|Walker|S1-selective|1003.7 /1039.0ms|1048.3 /1078.9ms|1.002|
|v0.30.0|Walker|S1-dense|1016.7 /1054.9ms|1065.9 /1099.9ms|1.021|
|v0.30.0|Walker|S2|1007.4 /1023.7ms|1007.4 /1023.7ms|1.038|
|current-base-8a452b3|FileList|S1-selective|268.4 /290.4ms|289.2 /311.5ms|1.014|
|current-base-8a452b3|FileList|S1-dense|273.0 /293.7ms|297.4 /315.4ms|1.079|
|current-base-8a452b3|FileList|S2|272.7 /274.7ms|272.7 /274.7ms|1.063|
|current-base-8a452b3|Walker|S1-selective|1010.1 /1031.7ms|1048.0 /1076.8ms|1.005|
|current-base-8a452b3|Walker|S1-dense|1013.4 /1031.9ms|1051.5 /1077.1ms|1.021|
|current-base-8a452b3|Walker|S2|1017.3 /1043.0ms|1017.3 /1043.0ms|1.009|

Ratios are feature-cost observations. No uncalibrated2x/fixed-ms gate is introduced. Current observer controls (7 on/off pairs) have median ratio0.956 and maximum1.046; variation does not prove improvement. Raw data retains frames, progress gaps, producer waits and phase proof.

## Evidence and reproduction

- [252 raw records](indexing-perf-2026-10-03/basic-raw.jsonl):168 tagged +84 current; failed/noneligible runs excluded from timing aggregates.
- [Per-case medians/maxima and all paired ratios](indexing-perf-2026-10-03/basic-comparisons.json), [14 observer controls](indexing-perf-2026-10-03/basic-observer.jsonl).
- [Matrix/commands/diagnostics](indexing-perf-2026-10-03/basic-matrix.json), [environment/current harness identity](indexing-perf-2026-10-03/basic-environment.json), [tag/lock/adapter identities](indexing-perf-2026-10-03/basic-identities.json).
- Exact test-only patches: [v27](indexing-perf-2026-10-03/v0.27.0-basic.patch), [v28](indexing-perf-2026-10-03/v0.28.0-basic.patch), [v29](indexing-perf-2026-10-03/v0.29.0-basic.patch), [v30](indexing-perf-2026-10-03/v0.30.0-basic.patch).

Export the recorded tag with `git archive`, apply its patch with `patch -p1`, and run the matrix commands from that export’s `rust/` with its own target. Preserve Cargo.lock; use `cargo +1.97.1 fetch --locked` if pinned crates are uncached. Full command: `cargo +1.97.1 test --release --locked --offline --lib app::tests::indexing_perf::perf_indexing_contention_paired -- --exact --ignored --nocapture --test-threads=1`. For old-tag AA controls set `FW_INDEX_PERF_CASES=B0`. Zero tests/missing rows do not certify a profile.

v27 uses owned precreated UI/roots files and a thread-local test settings override; v27/28 omit the unavailable active-filter observation. Wrong-toolchain/shared-target runs, the insufficient earlier oracle and interrupted stages were retired. Only strengthened identified samples are aggregated.

Native input, Windows/Linux, and weekly trusted-policy activation are **NOT RUN**. Non-search and stableActive-search/Warm-index results are separately identified profiles.
