# Indexing contention measurements — 2026-10-04

Point-in-time macOS/aarch64 headless observations. The tested source is base `8a452b3858c80f70a5f3064834c8add4d9b1d347` plus the exact 213-file [source overlay and hashes](indexing-perf-2026-10-04/current-e2efece3-source-overlay.json), stable map `e2efece38146877a8f47269aa01374acec9d2d468207ec51ebaaa1b183fed07d`. This source includes the ordered Active/Warm batch handoff repair exposed by the new oracle. Historical adapters retain their original production behavior.

## Method and interpretation

Rust1.97.1, unchanged Cargo.lock/default features, local filesystem,12 logical CPUs, release profile,16ms production update/render cadence,100,000 fixture entries,7 AB/BA pairs after untimed warmup. Settings use the existing500,000 Walker cap,12 search threads and25,000 search parallel threshold. The Walker uses its existing adaptive defaults; the search thread setting does not set its traversal concurrency. Fixture generation, startup, seed indexing, source-preserving empty setup, oracle construction and teardown are outside each measured request. Elapsed runner time includes them and is not an indexing duration.

[TC-228–232 procedures](../testplan/indexing-contention.md) own the fixed inputs, workload, latest-generation, actual-worker and independent-oracle checks. t1 includes data publication and producer Full waits; t2 is request-owned snapshot/debt settlement; t3 also settles latest results. Worker-only parser termination is separate. Identical-condition AA profiles measure variability; operation/settings AB profiles measure feature cost. Comparisons between tags use the identical condition, not a ratio between different logical work or sources. Seven pairs support median/maximum observations, not a meaningful p95 estimate. Time thresholds remain observational until host variability and healthy-version behavior justify calibration; correctness, actual overlap and finite settlement are asserted.

Extension `max_no_work_progress_ms` is the interval without a change in the recorded request/phase/stage high-water marks, emitted counts or successful auxiliary/search work. Replaying a filter below its earlier high-water mark can perform real work without advancing that metric. It does not measure zero CPU activity, event-loop suspension or native input latency. `max_ingest_gap_ms` separately tracks increases in the owned ingested count; `max_frame_ms` measures the admitted production-handler/update/render interval. These signals help locate a slowdown without proving its causal contribution.

## Final-source basic observations

All84 raw records are correct/eligible with exact7pair/6cell keys, AB/BA order and t1≤terminal≤t2≤t3. Observer controls are14 correct/full-profile records; median paired on/off t3 ratio1.041 is observational. Observer records have no separate META, so their command/settings provenance comes from the matching source-verified sidecar. [Accepted evidence manifest](indexing-perf-2026-10-04/current-e2efece3-common-evidence.json) retains logs, source/environment sidecars, independent checks and exact raw JSON values.

| Source | Case | Case t2 median / max | Case t3 median / max | Median paired case/B0 t2 ratio |
| --- | --- | --- | --- | --- |
|FileList|S1-selective|272.8 /288.0ms|296.7 /307.3ms|1.081|
|FileList|S1-dense|279.0 /293.0ms|303.5 /317.2ms|1.126|
|FileList|S2|268.5 /299.7ms|268.5 /299.7ms|1.032|
|Walker|S1-selective|1005.3 /1032.8ms|1049.3 /1077.8ms|1.012|
|Walker|S1-dense|1006.6 /1042.7ms|1047.3 /1089.7ms|1.019|
|Walker|S2|1010.8 /1035.8ms|1010.8 /1035.8ms|1.003|

The [earlier frozen four-tag basic study](indexing-perf-baseline-2026-10-03.md) remains unchanged. Its v0.27/28/29 empty-startup cold search conditions are NON_ELIGIBLE because substantial search waits for the committed candidates; those observations do not reproduce or refute the reported v29 slowdown. Stable Active-search/Warm-index profiles establish an available100k search candidate set for the expanded comparison.

## Current extension observations

The complete44-cell profile produced616 correct/eligible records (100k,7 pairs), and the separate500001-input/500000-cap truncated profile produced14. Both actual runners passed exactly one test; the immutable source stayed unchanged. [Complete evidence and independent checks](indexing-perf-2026-10-04/current-e2efece3-full-evidence.json) retain the logs, raw rows, source/environment sidecars and strict collector summaries. The full runner took2508.89s and truncation162.38s including untimed setup/teardown; these are not request indexing durations.

AB ratios below measure the declared operation or setting cost against its matched control. Tab operations include multiple requests and their finalization/physical settlement; their ratios cannot be described as a single100k request throughput regression. AA ratios describe repeated-condition variability. Worker parser endpoints are consumer-terminal delivery, separate from GUI t2/t3.

| Case | Source | Pair | Endpoint | Condition median / max | Median paired condition/control |
| --- | --- | --- | --- | --- | --- |
|F1-filelist-parser-files|FileList|AA|worker terminal|243.7 /256.2ms|0.992|
|F1-filelist-parser-folders|FileList|AA|worker terminal|223.5 /225.1ms|1.035|
|F1-files|Walker|AA|GUI t2|775.8 /812.8ms|1.002|
|F1-folders|Walker|AA|GUI t2|219.3 /227.7ms|1.015|
|F1-ignore-case|FileList|AB|GUI t2|322.3 /368.8ms|1.221|
|F1-ignore-case|Walker|AB|GUI t2|968.2 /1008.1ms|0.996|
|F1-ignore-list|FileList|AB|GUI t2|276.1 /340.5ms|1.090|
|F1-ignore-list|Walker|AB|GUI t2|939.1 /969.5ms|1.001|
|F1-mid-ignore|FileList|AB|GUI t2|400.7 /447.6ms|1.522|
|F1-mid-ignore|Walker|AB|GUI t2|1075.1 /1103.8ms|1.157|
|H1-early|FileList|AA|GUI t2|795.2 /821.9ms|1.033|
|H1-late|FileList|AA|GUI t2|818.3 /853.5ms|1.033|
|O1-modified-all|FileList|AB|GUI t2|295.1 /309.7ms|1.161|
|O1-modified-all|Walker|AB|GUI t2|958.1 /1009.3ms|1.027|
|O1-modified-shown|FileList|AB|GUI t2|257.7 /269.1ms|1.006|
|O1-modified-shown|Walker|AB|GUI t2|942.9 /967.2ms|0.993|
|O1-name-all|FileList|AB|GUI t2|291.8 /315.7ms|1.163|
|O1-name-all|Walker|AB|GUI t2|973.9 /994.2ms|1.030|
|O1-name-shown|FileList|AB|GUI t2|257.7 /264.0ms|1.035|
|O1-name-shown|Walker|AB|GUI t2|950.7 /975.9ms|1.013|
|P1-preview|FileList|AB|GUI t2|1323.5 /1348.0ms|0.984|
|P1-preview|Walker|AB|GUI t2|2069.9 /2111.5ms|0.990|
|R1-natural|FileList|AA|GUI t2|277.0 /285.0ms|1.011|
|R1-natural|Walker|AA|GUI t2|1030.3 /1088.4ms|0.999|
|S1-ignore|FileList|AB|GUI t2|305.1 /314.4ms|1.084|
|S1-ignore|Walker|AB|GUI t2|972.2 /984.7ms|1.012|
|S2-files|Walker|AB|GUI t2|782.9 /799.4ms|1.023|
|T1-A-B-C-A|FileList|AB|GUI t2|1450.0 /1495.9ms|5.397|
|T1-A-B-C-A|Walker|AB|GUI t2|2324.7 /2357.2ms|2.226|
|T1-S1-dense|FileList|AB|GUI t2|1298.1 /1348.1ms|0.997|
|T1-S1-dense|Walker|AB|GUI t2|2069.1 /2099.6ms|1.024|
|T1-S1-selective|FileList|AB|GUI t2|1315.1 /1345.1ms|1.011|
|T1-S1-selective|Walker|AB|GUI t2|2065.6 /2143.7ms|1.014|
|T1-S2|FileList|AB|GUI t2|1305.0 /1332.4ms|1.009|
|T1-S2|Walker|AB|GUI t2|2054.5 /2066.9ms|1.001|
|T1-active-warm|FileList|AB|GUI t2|1322.3 /1336.7ms|5.100|
|T1-active-warm|Walker|AB|GUI t2|2032.3 /2091.3ms|2.034|
|T1-natural-reclaim|FileList|AB|GUI t2|1352.8 /1359.6ms|4.961|
|T1-natural-reclaim|Walker|AB|GUI t2|2084.0 /2104.2ms|2.044|
|T1-promotion|FileList|AB|GUI t2|1356.2 /1378.5ms|5.092|
|T1-promotion|Walker|AB|GUI t2|2031.3 /2062.0ms|1.981|
|W1-deep|Walker|AA|GUI t2|1024.8 /1056.5ms|1.004|
|W1-follow-links|Walker|AA|GUI t2|2118.4 /2153.6ms|0.992|
|W1-wide|Walker|AA|GUI t2|1033.3 /1051.6ms|0.990|
|W1-truncated|Walker|AA|GUI t2|5121.3 /5154.1ms|1.005|

The largest operation-cost ratios are the multi-tab FileList cases (about5.0–5.4×) and multi-tab Walker cases (about2.0–2.2×). These include extra indexing requests and tab coordination; historical comparisons must use the same operation and initial state. A finite mid-index Ignore List edit also adds a refresh (FileList median paired t2 ratio1.522). These observations support covering tab coordination, reclamation and filter edits alongside search. They do not establish a version regression.

The largest observed headless full-frame maxima were55.349ms for follow-links,44.708ms for deep Walker and39.689ms for wide Walker. The16ms cadence schedules frames but does not assert a16ms frame ceiling. These maxima are separate from the existing TC-154 coordinator-only p95 fixture and from unobserved native input latency.

Fixed TabChain produced13 intentionally canceled B victims and one actual stale-Full-aborted Failed B victim. In that latter Walker record,26,624 entries were emitted; its declared last-good seed had100,000 entries and passed the full post-t3 oracle. Actual earliest Warm removal preceded the captured Full data-send stale decision, Failed offer/publication and worker-body return; physical mailbox close occurred later. Required latest A/C generations still succeeded. The partial B work is not100k completed indexing throughput.

Truncation validates exactly500000 unique allowed entries and its terminal reason. Its two successful untimed warmup calls and RuntimeConfig RAII restoration are source-inspected provenance; there are no warmup markers or independent before/after restoration readback in that log.

## GUI validation scope

GSM-001, GSM-007 and GSM-010 have automated/simulated evidence from headless production updates with actual workers. These observations do not constitute a native GUI test result.

| Flow | Confirmed behavior | Retained evidence |
|---|---|---|
| GSM-001 indexing settlement | Required latest source/root/request, independent membership/kind/order/query, actual request-body return and zero outstanding debt; parser delivery has its own endpoint. | [Current common](indexing-perf-2026-10-04/current-e2efece3-common-evidence.json), [full630](indexing-perf-2026-10-04/current-e2efece3-full-evidence.json) |
| GSM-007 tab handoff | Actual switch acknowledgements and ownership; required final A/C succeed. Retained B has13 Canceled outcomes and1 specifically witnessed stale-Full Failed outcome, with its independently checked100k seed. | [Full630](indexing-perf-2026-10-04/current-e2efece3-full-evidence.json), [stale-Full guards](indexing-perf-2026-10-04/stale-full-component-evidence.json) |
| GSM-010 bounded indexing response | Bounded-drain/Full/ownership guards and constructive-progress/debt checks. The TC-154 coordinator-only fixture has50 samples/p95=0.001ms below its existing50ms ceiling; frame maxima and native input latency are separate. | [Component guards](indexing-perf-2026-10-04/stale-full-component-evidence.json), [selected VM](indexing-perf-2026-10-04/current-e2efece3-selected-vm-evidence.json) |

The actual request-body return hook does not prove an individual worker-thread join. Current truncation's warmup calls and configuration restoration responsibility are source-confirmed; its log has neither explicit warmup markers nor independent post-restoration runtime readback. Native interaction/screenshots, Windows/Linux execution, individual App worker joins, external-signal cleanup and weekly activation remain unverified.

## Historical diagnostic excluded from comparisons

A v0.27.0 expanded diagnostic produced no raw timing samples. Its first15 cells ended in constructive-stall failures even though their workers had finished and cleanup was confirmed: the test adapter lost its Started-root callback at the projection wrapper, so the owned source/root proof remained false. Root callback forwarding was absent in v0.27–29; nested-input callback forwarding was absent in all four adapters. v0.30 observes its Started root through a different native send path. The diagnostic was stopped during the16th cell;28 cells and the cap profile were not run. Its actual process exit was−15, with physical process termination confirmed, but cleanup of the interrupted cell was not proved and11 owned root candidates were retained. [Excluded diagnostic evidence](indexing-perf-2026-10-04/historical-v27-invalid-observer-evidence.json) preserves the raw outcomes and source identities. This evidence supplies no version-speed judgment or product signal-cleanup result. The filtered Walker diagnostic uses a default-filter quiet oracle and cannot by itself establish a failure of the active filter oracle.

## Historical extended collection

The repaired R5 adapters forward only test observation callbacks and preserve each tag's production behavior, original Cargo.toml/lockfile and scheduling. [Four-tag source and139 actual guards](indexing-perf-2026-10-04/historical-expanded-r5-guard-evidence.json) retain original archives, replayable patches, exact source maps and cleanup guards. The separate [R3 historical collector](indexing-perf-2026-10-04/historical-validator-r3-evidence.json) preserves the actual whole-test status and admits only complete, sealed PASS cells with actual cleanup/root restoration. All failed-cell rows are excluded. Collector success validates provenance and selection; it does not turn an actual FAILED test into PASS.

| Tag | Ordinary44-cell test | Accepted ordinary samples | Separate cap test | Retained evidence |
|---|---|---|---|---|
|v0.27.0|FAILED, exit101;33PASS/11FAIL|462, with0 failed-cell partial rows|PASS, exit0;14 samples|[Actual v27](indexing-perf-2026-10-04/historical-v27-r5-live-evidence.json)|
|v0.28.0|FAILED, exit101;37PASS/7FAIL|518, with0 failed-cell partial rows|PASS, exit0;14 samples|[Actual v28](indexing-perf-2026-10-04/historical-v28-r5-live-evidence.json)|
|v0.29.0|FAILED, exit101;39PASS/5FAIL|546, with0 failed-cell partial rows|PASS, exit0;14 samples|[Actual v29](indexing-perf-2026-10-04/historical-v29-r5-live-evidence.json)|
|v0.30.0|FAILED, exit101;43PASS/1FAIL|602, with0 failed-cell partial rows|PASS, exit0;14 samples|[Actual v30](indexing-perf-2026-10-04/historical-v30-r5-live-evidence.json)|

These accepted records retain100k fixture inputs and7 pairs, but the logical work depends on the condition. In particular, Files-only Walker AA pairs both select80k files and Folders-only pairs both select20k folders. Their repeated-condition ratios describe variability, not the cost of enabling a filter. Compare the same source, condition, output count and operation across tags. A generic FAIL remains FAIL; neither unsupported nor NON_ELIGIBLE can be inferred from it. Physical process reap is separate from the historical App worker/writer guards and from untested product signal cleanup. The11 roots retained from the interrupted diagnostic above remained untouched during these completed attempts.

The old-version failed cells stop in untimed condition warmup and provide no paired timing rows. Their actual failures have distinct meanings:

| Failure evidence | v0.27.0 cells | v0.28.0 cells | v0.29.0 cells | v0.30.0 cells | Interpretation |
|---|---|---|---|---|---|
|Ignore Case change: visible-result oracle mismatch|2|2|0|0|All100k unique paths and kinds match; the final visible count differs from the common oracle. Exact differing visible paths were not logged.|
|Mid-index Ignore edit: expected refresh allocation absent|2|0|0|0|v27's native action refilters without the refresh required by this trace. This is a native-operation/trace precondition mismatch, not proof of wrong final membership.|
|Sort: required successful worker delivery during GUI indexing unproved|6|4|4|0|The overlap assertion fails. A100k empty-query sort can legitimately report0 search evaluations; that alone does not prove0 sort work or unsupported functionality.|
|FileList promotion: line-order oracle mismatch|1|1|1|1|All100k paths/kinds match, but position16384 contains item32768 instead of item16384. The current ordered-handoff repair is not backported.|

The [v29 failure/work addendum](indexing-perf-2026-10-04/historical-v29-failure-work-addendum-evidence.json) checks all three versions' common33 ordinary PASS cells: declared source, settings, logical work, normalized initial seed and finite actions match in each A/B condition. v28/v29 share37 PASS cells; v29 additionally passes both Ignore Case cells. The two mid-Ignore and two Modified-Shown cells have no corresponding accepted v27 speed comparison, and Ignore Case has no accepted v27/v28 speed comparison. Tab operations can emit different partial work for intentionally retired requests, which remains separately recorded. Old-version intended specifications were not independently audited: a mismatch against this common oracle is not automatically an assertion that the old release violated its own specification.

## Four-tag condition comparison

Each value is the median / maximum of the seven accepted **B condition** samples in milliseconds. Compare a row across versions. For AA, B repeats A's setting; it is not an enable-filter cost. Parser rows use worker consumer-terminal delivery; other rows use GUI t2. FAIL has no timing value. A/control, t1/t3, Full-wait and all seven values remain in the retained JSON/CSV. These numbers are not paired A/B ratios.

| Condition | Source | v0.27.0 | v0.28.0 | v0.29.0 | v0.30.0 |
|---|---|---|---|---|---|
|F1-filelist-parser-files|FileList|246.9 / 257.2|256.9 / 260.4|251.2 / 258.8|251.5 / 264.5|
|F1-filelist-parser-folders|FileList|222.2 / 228.2|219.0 / 229.8|222.3 / 228.7|222.0 / 226.2|
|F1-files|Walker|793.6 / 848.5|817.3 / 834.7|65778.4 / 66085.8|791.7 / 808.2|
|F1-folders|Walker|223.2 / 227.4|221.5 / 266.4|3771.8 / 3869.5|220.8 / 223.3|
|F1-ignore-case|FileList|FAIL|FAIL|62217.4 / 68763.1|434.6 / 497.4|
|F1-ignore-case|Walker|FAIL|FAIL|90701.3 / 95673.1|992.7 / 1000.5|
|F1-ignore-list|FileList|259.1 / 272.2|283.7 / 329.6|59610.0 / 61911.2|335.9 / 389.3|
|F1-ignore-list|Walker|979.3 / 992.0|978.7 / 985.1|93270.4 / 95624.2|947.9 / 994.2|
|F1-mid-ignore|FileList|FAIL|387.0 / 424.3|60348.8 / 64489.2|418.3 / 514.5|
|F1-mid-ignore|Walker|FAIL|1100.1 / 1134.3|91769.5 / 95705.7|1089.6 / 1126.5|
|H1-early|FileList|562.1 / 605.0|767.4 / 793.5|790.5 / 830.4|790.9 / 819.8|
|H1-late|FileList|588.3 / 599.5|802.5 / 845.3|827.4 / 829.8|808.4 / 846.3|
|O1-modified-all|FileList|FAIL|FAIL|FAIL|299.0 / 309.2|
|O1-modified-all|Walker|FAIL|FAIL|FAIL|964.4 / 980.9|
|O1-modified-shown|FileList|FAIL|264.9 / 271.0|259.9 / 264.8|252.1 / 276.7|
|O1-modified-shown|Walker|FAIL|965.2 / 979.1|961.3 / 988.9|958.4 / 979.3|
|O1-name-all|FileList|FAIL|FAIL|FAIL|299.5 / 305.7|
|O1-name-all|Walker|FAIL|FAIL|FAIL|980.6 / 986.6|
|O1-name-shown|FileList|255.3 / 279.5|262.5 / 271.2|261.5 / 264.9|251.6 / 275.5|
|O1-name-shown|Walker|948.5 / 969.5|960.0 / 1012.0|967.5 / 1008.3|975.8 / 980.1|
|P1-preview|FileList|1303.4 / 1332.5|1227.9 / 1250.4|1316.4 / 1361.3|1293.6 / 1330.2|
|P1-preview|Walker|2056.7 / 2129.1|1909.2 / 1942.3|2069.7 / 2092.2|2028.9 / 2090.9|
|R1-natural|FileList|277.5 / 307.5|275.3 / 282.9|276.4 / 283.8|278.4 / 291.1|
|R1-natural|Walker|1006.0 / 1057.6|1035.8 / 1060.3|1040.0 / 1083.6|1004.1 / 1046.1|
|S1-ignore|FileList|284.8 / 307.0|303.4 / 325.8|1891.6 / 1990.3|340.2 / 359.7|
|S1-ignore|Walker|947.1 / 971.8|968.7 / 1002.9|3069.4 / 3264.3|1004.1 / 1013.8|
|S2-files|Walker|814.2 / 845.0|820.3 / 830.5|50067.6 / 50668.7|799.5 / 843.4|
|T1-A-B-C-A|FileList|1439.4 / 1488.7|1447.4 / 1470.9|1457.1 / 1467.7|1468.4 / 1503.8|
|T1-A-B-C-A|Walker|2314.9 / 2327.0|2302.5 / 2336.7|2322.0 / 2370.4|2310.8 / 2388.3|
|T1-S1-dense|FileList|1299.2 / 1365.1|1300.5 / 1362.1|1319.7 / 1341.7|1303.3 / 1346.2|
|T1-S1-dense|Walker|2064.2 / 2100.0|2093.6 / 2167.3|2084.8 / 2167.3|2076.7 / 2088.0|
|T1-S1-selective|FileList|1312.0 / 1329.3|1300.9 / 1381.3|1298.2 / 1338.8|1315.7 / 1341.0|
|T1-S1-selective|Walker|2049.1 / 2063.1|2067.4 / 2106.2|2089.0 / 2136.3|2089.6 / 2121.6|
|T1-S2|FileList|1315.5 / 1342.6|1317.2 / 1364.4|1296.6 / 1336.6|1292.8 / 1338.9|
|T1-S2|Walker|2053.0 / 2179.0|2074.5 / 2107.5|2086.5 / 2093.7|2073.3 / 2123.6|
|T1-active-warm|FileList|1317.0 / 1367.1|1331.9 / 1365.2|1282.7 / 1353.2|1339.0 / 1361.7|
|T1-active-warm|Walker|2075.0 / 2093.7|2043.1 / 2096.2|2050.3 / 2091.8|2049.6 / 2072.4|
|T1-natural-reclaim|FileList|1323.3 / 1362.3|1336.0 / 1358.3|1342.3 / 1363.5|1344.2 / 1374.5|
|T1-natural-reclaim|Walker|2052.0 / 2103.8|1960.8 / 2085.0|2056.5 / 2096.9|2092.0 / 2108.3|
|T1-promotion|FileList|FAIL|FAIL|FAIL|FAIL|
|T1-promotion|Walker|2040.0 / 2154.0|2035.6 / 2079.3|2025.3 / 2139.4|2045.5 / 2073.0|
|W1-deep|Walker|1003.0 / 1018.9|982.5 / 1031.7|992.8 / 1033.3|1006.1 / 1018.6|
|W1-follow-links|Walker|2120.9 / 2147.0|2028.9 / 2051.8|2135.7 / 2162.3|2131.0 / 2159.8|
|W1-wide|Walker|1023.8 / 1057.9|995.8 / 1015.5|1020.2 / 1046.4|1011.1 / 1046.2|

[Reproducible arithmetic and four-tag work/failure audit](indexing-perf-2026-10-04/four-tag-comparison-evidence.json) bind these values to stored input SHA256, exact peeled tags and sealed statuses. The calculation verifies all retained input assets, seven unique pair IDs per half, finite metrics and consistent declared contracts. The independent work audit checks identical normalized seeds and finite actions for the common33 cells; v28/v29 and v28/v30 share37, and v29/v30 share39. Actual checkpoint overshoot, retired partial work and native implementations can differ.

The major observed non-search regression is GUI filter ingestion on v29. Files-only Walker (100k fixture,80k logical output) takes65.78s versus0.817s on v28 and0.792s on actual v30. Ignore List takes59.61s FileList /93.27s Walker on v29 versus0.284s /0.979s on v28 and0.336s /0.948s on v30. These empty-query profiles demonstrate that search is not necessary for the slowdown. Files-only v29 median producer Full wait is61.41s and median ingestion gap is3.44s, compared with0.757s and0.028s on v30. They identify backpressure and ingestion pauses; they do not measure filter restart counts or causal fractions.

The matched stable Active-search/Warm-index T1-S1/S2 flat-file profiles do not reproduce a similar v29 slowdown in this host/fixture. S1-ignore and S2-files combine search with filtering, so their version differences cannot be assigned solely to search. Preview, reclamation, tab coordination, nested overrides and traversal shapes are retained as distinct future regression signals; their differing operation or physical work must stay explicit.

### Calibration decision

Keep TC-228–232 locally runnable/ignored performance profiles with mandatory correctness, actual-work/overlap and finite-settlement guards. Prioritize F1-files/folders, Ignore List, Ignore Case, mid-index edits and the search/filter combinations for timing regression monitoring, alongside the stable Active-search/Warm-index controls. This single macOS host and seven pairs show a clear v29/v30 separation but do not justify a portable absolute ceiling or an arbitrary2× gate. Before adding a timing gate, repeat the same accepted profiles on the intended stable runner, retain healthy-version variability and observer overhead, and choose per-profile baselines/margins from that evidence. Weekly/trusted-policy activation is a separate operation and was not performed.

## Filter-path source inspection

[Retained source and failure diagnostics](indexing-perf-2026-10-04/historical-source-failure-diagnostics-evidence.json) separate native-source facts from causal inferences. The v28/v29 index worker, bounded mailbox, Walker adaptive scheduler and index-response effects have identical source bytes. In v29, incremental empty-query result updates can request an active filter of the growing prefix; that filter advances512 candidates per frame, and normal queued ingestion returns while it is pending. Repeated prefix filtering can therefore delay consumption and increase producer Full waits. This is a source-based explanation consistent with the measured pauses, not a direct measurement of filter restart counts or their causal share. Reclaimer pressure is another possible contributor; its actual Full contribution was not observed in those diagnostics.

The actual v0.30.0 tag already contains the incremental filter identity and live empty-query fast path, with known-candidate ingestion bounded by32768 entries/4ms and unknown-candidate filtering by512 entries. These native tag functions were checked against the original archive, independently of the current overlay. The current overlay additionally repairs Active/Warm batch order; its results cannot substitute for actual v30 measurements.

## Reproduction and limits

Use the exact base and verified source overlay with its original lockfile, then the commands in the linked procedure from `rust/`. To reproduce these records, replay the matching retained sidecar command and its exact `environment_overrides`, including search threads/threshold and Walker cap; generic procedure commands otherwise use that host's configured defaults. Preserve each process exit status and complete log; missing, duplicate, incorrect, noneligible or partial records cannot be accepted as faster work. [Warm-removal component evidence](indexing-perf-2026-10-04/warm-removal-component-evidence.json) binds actual earliest invalidation to the fixed switch's actual acknowledgement; [target/pilot evidence](indexing-perf-2026-10-04/current-e2efece3-pilot-evidence.json) is separately identified diagnostic1pair coverage, not7pair performance evidence. Canceled or exactly witnessed stale-Full-aborted Warm-victim emitted counts describe partial work; retained last-good100k membership is independently checked and does not turn either terminal into completed100k throughput. The [stale-Full component evidence](indexing-perf-2026-10-04/stale-full-component-evidence.json) records the narrowly typed exception and its real mailbox decision witness. The [earlier failed full run](indexing-perf-2026-10-04/current-3cf46b60-full-failure-evidence.json) remains rejected partial evidence; no later witness is assigned to those old records.

Native input/product-process liveness, Windows/Linux timing, individual current worker-thread joins, external SIGINT/SIGKILL cleanup and weekly CI activation are NOT RUN. Historical native App join/owned-writer observations have their separately retained source/guard evidence; current request-body return and zero sender debt do not prove individual native thread joins. Headless rendering/actual-worker evidence does not certify those unrun axes. No dependency, workflow, release or external action is part of these observations.

## Selected local validation

The final selector chooses VM-001/002/003/004/008/009. The213-file source hash remains identical to the accepted full Cargo/component/performance runs; subsequent work changes evidence calculations and documentation only. Reuse those exact source-bound runs rather than adding overlapping counts or repeating unchanged heavy measurements.

| Surface | Actual result and evidence | Scope limitation |
|---|---|---|
| Cargo / warnings | Default-parallel locked/offline host Cargo:1548 library PASS/19 ignored,8 main,3 architecture,47 CLI,2 path PASS. Focused normal63/search9/order10/TC20782/pipeline152 and fmt/clippy PASS, warnings0. [Common](indexing-perf-2026-10-04/current-e2efece3-common-evidence.json), [component](indexing-perf-2026-10-04/stale-full-component-evidence.json). | The child leaf1 and focused/module counts overlap the enclosing library; they are not additive. |
| Owner / lifecycle / search | TC150/151/152/153/154/207/209–211, tab ownership/lifecycle/background/cache/session/FileList lifecycle, TC155/057B/163 covered by actual passing tests. [Selected module readback](indexing-perf-2026-10-04/integration-e2efece3-selected-normal-coverage.json). | No named TC203–206/208 test execution is inferred; their guards are traced through actual owner/pipeline modules. |
| Existing indexing perf gates | FileList30000 entries ratio2.13≥1.20 and Walker33025 ratio6.12≥1.25, each exact1 PASS. [Selected VM evidence](indexing-perf-2026-10-04/current-e2efece3-selected-vm-evidence.json). | Existing fixture thresholds only; do not transfer them to new contention profiles. |
| Existing search / coordinator gates | TC15611 shapes maximum16ms<250ms; TC1851m candidates,2 shapes×7, p50/p95/p99 and4 RSS phases; TC15450 coordinator samples p95=0.001ms<50ms. Same selected VM evidence. | RSS remains observational. Coordinator cost does not certify whole-frame or native input cost. |
| Documentation / repository contracts | Fresh selected plan,19 Python contract/selector tests PASS, repository contract PASS;32 retained manifests/907 asset references,213 source files,14 changed Markdown documents/170 local links verified before final review. [Integration verification](indexing-perf-2026-10-04/final-integration-evidence.json). | Point-in-time checksum/reference check; final independent review and closure have their own record. |

FileList decoder/read buffer/write/rollback, Walker adaptive/frontier budgets, production persistence/history/session merge, render facade, observable window trace, dependencies, workflow pins/audit/required checks and release behavior are unchanged. Their conditional validation branches are not applicable. New profiles run locally as ignored tests; correctness smokes and ordering guards remain in normal Cargo tests. No CI activation, PR/push or release operation is included.

The [fresh independent final review](indexing-perf-2026-10-04/final-review-evidence.json) verified the complete frozen local implementation, SDD and evidence range with no blocking/major/minor findings. Its exact report and main disposition are retained. This review permits the planned local commit and durable-checkout closure; it does not assert that closure, CI activation or any unrun platform/native test occurred.
