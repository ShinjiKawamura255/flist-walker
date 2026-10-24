# Native macOS TC-207 trace evidence

The subsequent [natural full-suite campaign](../natural-reproduction/README.md) preserves6 new traced suites and a natural promotion100ms assertion failure, with source-order bounds on Preempted contribution. The results below retain their earlier identities.

The original TC-207 cause remains **UNKNOWN**. Strict wall `<100ms` remains the only acceptance rule; CPU time and trace classifications do not exempt a failure. This follow-up to [the earlier diagnosis](../diagnosis.md) records actual native scheduling states, separately from the earlier state snapshots and WSL guest observations. No production path, finalization work budget, dependency, CI policy or specification changes.

## Method and identity

Base `5247237f94629e32f742a76204163599bdc2e509` (merged PR #206), macOS27.0.1 arm64, 12 logical CPUs, Rust1.97.1 debug test profile. Test-only schema3 adds raw `mach_absolute_time` ticks, timebase and enclosing nanosecond windows. Clock reads and diagnostic serialization remain outside the original strict wall interval. A PID-owned JSONL file serializes complete records under one test-only mutex after measurement, avoiding parallel libtest output corruption. Missing/failed output fails collection; it never falls back to partial stderr.

Xcode27.1 (27A9275) was installed from Apple's App Store, and the user explicitly accepted the displayed SDK agreement. Required initial components completed. Installation changed the automatic developer-directory fallback from Command Line Tools to Xcode; no `xcode-select --switch` or persistent selection symlink was created. Rust build/validation commands explicitly use `DEVELOPER_DIR=/Library/Developer/CommandLineTools`; trace commands use the Xcode directory. No SIP, tracing privilege, privacy, kernel or repository setting was changed.

`record.py` directly owns the test and tracer. `clock-anchor.c` validates the expected parent/nonce, removes injection variables before children can inherit them, and stops its own test process before measurements. The parent registers a unique notification, attaches xctrace, waits for the actual tracing-start notification, then resumes that PID. Process-start/exit Points of Interest anchors bracket clock conversion. The startup stop is outside every measured frame. Cleanup resumes/kills/reaps only direct owned handles with bounded waits and retains failure receipts; SIGKILL, host or storage failure cannot guarantee cleanup.

The final profile is Time Profiler + Thread Activity + Points of Interest, context-switch sampling enabled, kernel/waiting-thread sampling and high-frequency mode disabled, OS logs excluded. No rolling `--window` is used. The earlier full System Trace profile produced excessive data; it is retained as a separate protocol, not silently replaced.

## Outcomes

Final protocol4 uses binary SHA-256 `1c0b649e7ed52ff16623d5338f4e23fd240118124c1a2cddd016bb21a68ba78d`, clock dylib `524464244dbb826902a0e94633762b9d6bdef436af2068b2b715046af1068ac8`. The predeclared four captures each ran once. Values are milliseconds for the **same maximum-wall frame**; blank states were absent in its conservative inner window, not proven zero across the entire wall interval.

| Capture | Frames | Max wall | Running | Blocked | Runnable | Preempted | Interrupted | Test / tracer exit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| CPU control | 1 | 122.243250 | 119.382740 | — | — | 2.125956 | 0.665220 | 101 / 0, expected assertion FAIL |
| Sleep control | 1 | 155.104084 | 0.076917 | 154.992124 | 0.005125 | — | — | 101 / 0, expected assertion FAIL |
| Four natural cases, serial | 521 | 1.031000 | 0.964871 | — | — | 0.059250 | 0.002710 | 0 / 0, 4 PASS |
| Complete lib suite, default parallelism | 1,616 | 4.152542 | 0.651332 | — | — | 3.489625 | 0.003917 | 0 / 0, 1,659 PASS / 20 ignored |

All final protocol4 clock offsets are admitted. Every frame has a nonempty conservative attribution window with no UNKNOWN coverage gaps. The full-suite frame records cover10 source-owned cases and12 complete dumps; they are distinct from the1,659 libtest test count. No natural strict wall failure occurred. The full maximum was `tc_207_background_finalization_tracks_ignore_policy_changes_both_directions`, frame334 of its first dump. These are observations under tracing, not proof of the original intermittent cause.

The controls demonstrate on-CPU execution versus blocking and a small ready-to-run interval in the sleep control. A natural frame includes a substantial Preempted interval; its mechanism is not narrowed to involuntary run-queue starvation. CPU samples/transition stacks and every numeric state interval are retained for inspection. The original old failure lacked contemporaneous clocks/events, so no production optimization is justified by these successful runs alone.

| Earlier protocol | Preserved result and limitation |
| --- | --- |
| v1, original schema2 binary | CPU/sleep controls failed as intended. Four natural cases passed, but MONOTONIC anchor intervals disagreed by2167ns; correlation UNKNOWN. Full suite NOT RUN after failed admission. |
| v2, native schema3 | CPU/sleep controls and618-frame serial capture admitted; serial max5.730375ms. Full suite1,658 PASS, but10GB trace exceeded45s finalization limit, tracer killed/reaped with exit-9. Mixed stdout/stderr also corrupted frame JSON; no complete full attribution. |
| v3, reduced profile / separate stderr | Controls and534-frame serial capture admitted, serial max1.063750ms. Full suite1,658 PASS and trace saved, but parallel stderr still had1,642 markers with1,637 complete JSON lines,4 malformed lines and1 incomplete marker. Whole full frame-dump correlation INVALID. |
| v4 initial analysis | Dedicated full JSONL was complete; parser rejected legitimate repeated drives. Repaired analysis of the same raw data succeeded. Original parser and rejection retained. |

No failure was erased, reconstructed into a success, pooled with a different protocol or replaced by a passing recollection. Compiler/source/binary differences and extra diagnostic I/O prevent a capacity comparison across these protocols. Raw failed collections remain private local material. Available log/export hashes, bounded result records, plans and exact helper versions are in the retained archive; `record.trace` bundles remain local and are not hash-inventoried.

## Interpretation

Both anchor offset intervals must intersect. Native Mach conversion uses integer arithmetic with the observed125/3 timebase and42ns tick-ceiling allowance; schema3 cannot fall back to MONOTONIC. The v1 MONOTONIC2167ns offset disagreement is rejected. Each frame's conservative inner window subtracts the full outer measurement overhead and both clock-bound uncertainties. Very short windows can remain unattributable. State totals describe that inner window and do not sum to the entire strict wall duration.

The initial parser rejected the full-suite file because two tests legitimately execute multiple complete drives with the same test/request/TID. The original parser/rejection are retained. A parser-only repair recognizes a new frame0 only after the prior declared dump completed; it rejects overlap, partial dumps, missing/extra cases and incorrect dump counts. Ten source-owned full-suite cases require12 complete dumps. This expectation is derived from the frozen test source during analysis, not represented as an additional pre-run declaration. Independent review also required rejection of empty/noisy logs and negative/inconsistent wall/overhead/clock windows; regression tests were RED before these repairs. Reanalysis uses the same captured raw data without rerunning the test or recording.

Running is observed CPU execution, Blocked is off-core waiting, and Runnable is ready but waiting for CPU. Preempted is retained separately: it can include involuntary preemption or voluntary spin-lock yield, so it is not all asserted to be scheduler starvation. Interrupted is also separate. Only the primary Running time-profile table contributes CPU sample counts; supplementary context-switch samples are not added as independent CPU samples. Gaps become UNKNOWN; overlapping state rows, missing frame sequences, foreign PID/TID, malformed JSON and clock disagreement fail admission.

Apple's [System Trace explanation](https://developer.apple.com/videos/play/wwdc2016/411/) defines these states; the [Xcode resources](https://developer.apple.com/xcode/resources/) describe Instruments. This experiment correlates exported kernel states to exact test threads and intervals. It does not reconstruct unavailable scheduling events from CPU subtraction alone.

## Reproduction

Build the current test-only source with the locked toolchain and retain Cargo JSON, compiler version, binary/source hashes and all exits. Select the actual libtest executable from the Cargo JSON rather than guessing its hash suffix. Use a new private output directory per attempt; retain every declared result, including failures.

```sh
DEVELOPER_DIR=/Library/Developer/CommandLineTools cargo test --locked --offline --lib --no-run --message-format=json
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer clang -dynamiclib -O2 \
  docs/history/tc207-timing-2026-10-24/system-trace/clock-anchor.c -o "$TRACE_DIR/clock-anchor.dylib"
python3 docs/history/tc207-timing-2026-10-24/system-trace/record.py \
  --binary "$TEST_BINARY" --library "$TRACE_DIR/clock-anchor.dylib" \
  --output "$TRACE_DIR/full-lib" --child-timeout 240 --trace-seconds 300 \
  --finalize-timeout 180 --template 'Time Profiler' \
  --instrument 'Thread Activity' --instrument 'Points of Interest' \
  --recording-options "$TRACE_DIR/scheduling-options.json" --dedicated-frames -- --nocapture
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun xctrace export \
  --input "$TRACE_DIR/full-lib/record.trace" --toc --output "$TRACE_DIR/full-lib/toc.xml"
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun xctrace export \
  --input "$TRACE_DIR/full-lib/record.trace" \
  --xpath '/trace-toc/run[@number="1"]/data/table[@schema="thread-state" or @schema="context-switch" or @schema="context-switch-sample" or @schema="os-signpost-arg" or @schema="time-profile"]' \
  --output "$TRACE_DIR/full-lib/events.xml"
python3 docs/history/tc207-timing-2026-10-24/system-trace/analyze.py "$TRACE_DIR/full-lib" \
  > "$TRACE_DIR/full-lib/normalized.json"
```

Run Cargo from `rust/`, then return to the repository root for the helper paths. Recording options and the exact four-case serial/control arguments are retained in the campaign plans. Child240s/trace300s/finalize180s are collection resource limits, independent of the unchanged100ms product assertion. Controls deliberately fail the original assertion with exit101; verify the assertion text, not exit101 alone.

Raw traces/XML/TOC can contain environment, device and neighboring-process information. They remain private local material and are not publication artifacts. Publish only the target-PID numeric states, frame records, target stack symbols, bounded receipts and hashes. `replay.py` recomputes clock admission and every inner-window state integral from these sanitized inputs without Xcode. Available log/export hashes provide identity, not independent proof of the decoder's correctness; re-exporting raw data still requires the local private material and matching Xcode schema.

## Retained evidence and validation

[evidence.tar.gz](evidence.tar.gz) contains final target-thread inputs and summaries, all campaign plans, every owned capture receipt, raw-file hashes, bounded runner/assertion summaries, earlier method versions, frozen captured Rust test sources, parser RED/GREEN logs and source provenance. The projection retains complete state rows intersecting the measured threads' outer windows; it excludes unrelated target threads as well as other processes. Each recomputed frame integral is checked against the full local normalized analysis before retention. `sha256.json` inventories archive members; `manifest.json` describes the projection and capture statuses.

Archive SHA-256: `b4f235f7634827f0ea3beb75db5aa72085a7dfdb98855d118796dae03ee5928e` (1,479,871 bytes).

Extract into a new private directory, verify every member hash, then replay each `v4-*/inputs.json` using the adjacent helpers. Compare to its retained `summary.json`; do not overwrite the retained summary.

```sh
python3 docs/history/tc207-timing-2026-10-24/system-trace/replay.py "$EVIDENCE_DIR/v4-natural-full-lib/inputs.json" > "$EVIDENCE_DIR/replayed-full.json"
diff "$EVIDENCE_DIR/v4-natural-full-lib/summary.json" "$EVIDENCE_DIR/replayed-full.json"
```

VM-001/VM-002: locked/offline full `cargo test` passed (lib1,659 PASS,20 ignored; integration/quit-menu contract also PASS). Locked/offline all-target clippy with `-D warnings`, format check, repository-contract/quick validation, diagnostic Python21 tests and diff/reference checks passed. The added Rust clock/logging guards and Python parser repairs had focused failing tests before implementation; evidence-only documentation uses hash/reference/replay checks instead of product TDD. Independent read-only review found3 major parser-admission defects, all repaired and rechecked; its minor sequence-fixture issue was corrected to reach the intended guard.

This change does not alter rendering, dialogs, focus, scheduling, shutdown or production responsiveness. Native GUI operations, Windows performance/clipboard, real OS theme notification and real share disconnect/recovery were NOT RUN here; prior release-specific acceptance does not promote them to PASS. Full WSL scheduler-wakeup and Windows host-vCPU tracing remain NOT RUN. Xcode native trace execution fills this campaign's macOS tracing gap, while a naturally failing100ms frame and its contemporaneous causal evidence are still missing. Retain the strict gate and capture the first future failure with this bounded method; do not infer a runtime fix from successful repeats.
