# TC-207 natural full-suite reproduction campaign

This campaign continues [the native macOS trace investigation](../system-trace/README.md). The strict acceptance rule remains **wall <100ms**; thread CPU, OS state and successful later runs never exempt a failure. No Rust source, product behavior, work budget, dependency, CI policy or acceptance criterion changes.

## Fixed source and conditions

Base `9ab1b8be7d23b67706f33e48cc8b4592b5413eb5` (PR #207 rebase-merged), macOS27.0.1 arm64,12 logical CPUs, Xcode27.1, Rust1.97.1 debug libtest. The executable was copied before collection and remains SHA-256 `1c0b649e7ed52ff16623d5338f4e23fd240118124c1a2cddd016bb21a68ba78d`; the test-only clock dylib remains `524464244dbb826902a0e94633762b9d6bdef436af2068b2b715046af1068ac8`. Exact source/compiler/options identities are retained with the pre-run plan. These are the same executable bytes as the preceding final native campaign, not a rebuild relabeled as that binary.

The plan declares3 sequential full-suite runs at libtest's default parallelism, then at most3 full-suite runs with `--test-threads=48`. The latter intentionally increases competition between actual tests on this12-core host; it is a different stress condition from the original failure. `RUST_TEST_THREADS` is unset, and the existing recorder removes any diagnostic CPU/sleep control. There is no per-frame delay injection, external load process, in-frame process stop, affinity, priority or kernel-setting change. Each suite is a single owned PID with its normal background workers; captures do not overlap. The recorder's attach/startup stop precedes every measured frame.

The first strict TC-207 wall failure stops additional planned attempts; its full trace is finalized and analyzed before any further investigation. A recording/storage failure that prevents sound tracing also stops the campaign. Remaining planned attempts are explicitly NOT RUN. There are no replacement retries. Each test result, including unrelated failures, is separate from recording validity and frame/clock admission.

The existing `system-trace/record.py` owns/reaps the test and tracer with finite waits. Child240s, trace300s and finalization240s are resource limits, not acceptance thresholds. Time Profiler + Thread Activity + Points of Interest uses the same retained scheduling options as before and no rolling window. Raw traces remain private local material; no raw environment/device/neighbor information is published.

## Frame and failure admission

`project.py` reuses the preceding frozen parser's native clock, frame-integrity and state-integration functions. It validates the exact complete-suite argv and ten source-owned measured cases. Passed cases require all expected dumps (12 total for a passing suite). A failed case may stop before its later drive; each missing failed-case dump is listed explicitly. Missing passed-case dumps, extra cases/dumps, malformed JSON, overlapping windows, inconsistent wall/bracket values and incomplete frame sequences are rejected.

A wall-failing measured frame must belong to a runner-reported FAILED case and be its last frame/dump. Its100ms failure remains in the result. The original assertion is CONFIRMED only if a unique panic header with the exact test name immediately precedes the request/frame assertion; missing, mixed or duplicate context is UNKNOWN and is never repaired from another case. The deliberate low-CPU101ms guard's caught panic is not a natural reproduction: its test passes and it emits no corresponding measured failing frame.

Both process clock anchors must match the owned PID/nonce/native thread and actual Points of Interest events. Both native Mach offset intervals must agree; no MONOTONIC fallback or averaged inconsistent clocks. Conservative per-frame inner windows subtract the complete measurement bracket and clock uncertainty. Running, Blocked, Runnable, Preempted and Interrupted remain separate. Preempted can include voluntary spin-lock yield, so it is not all asserted to be involuntary scheduler starvation. UNKNOWN gaps and unresolvable short windows remain explicit.

## Projection and reproduction

A full raw export contains many unrelated test-worker samples. The new projector uses3 passes: collect referenced IDs while a numeric-ID bitset rejects every duplicate; retain only referenced definitions; resolve rows against that complete index, including forward references. Row wrappers are detached during each pass so the entire XML row tree is not retained. The byte cap is8GiB, the referenced-definition cap4,000,000 and the numeric ID range128,000,000 (about16MB bitset); exceeding a cap fails with original files retained. An initial full-ID dictionary reached its4,000,000-entry cap on both the reference XML and final new capture. Those analysis failures are retained; a failing regression preceded the referenced-only repair. The repaired projector exactly matches all7 numeric/state/stack fields of the preceding frozen1,616-frame full-suite input. Only the measured PID/TIDs, complete state rows intersecting their outer windows, numeric switches, primary/supplementary CPU samples and target stack symbols reach the public inputs. Full raw XML and duplicated per-frame sample arrays are not serialized into the shared artifact.

Use the preceding recorder/export recipe with a fresh private output directory per declared attempt. Pass `-- --nocapture` for the default condition and `-- --nocapture --test-threads=48` for the stress condition. Export `thread-state`, `context-switch`, `context-switch-sample`, `os-signpost-arg` and `time-profile` tables. From the repository root:

```sh
python3 docs/history/tc207-timing-2026-10-24/natural-reproduction/project.py "$CAPTURE_DIR"
```

Outputs `projected-inputs.json` and `projected-summary.json` are created exclusively and never overwrite earlier analysis. A test exit101 remains a failed test result even when recording and correlation succeed. An export/projector error retains its exit/stderr and raw receipt; no numeric attribution is claimed for that missing axis.

## Natural failure and source-order attribution

`threads48-3` naturally failed `tc_207_background_finalization_survives_promotion_to_active` with the unchanged `started.elapsed() <100ms` assertion. This path has no detailed frame-clock dump. Its exact wall duration, precise frame boundaries and total thread CPU remain unavailable; it is **not** the91.551834ms maximum of the separate clocked paths. The test's source performs `Instant::now → poll_index_response_with_budget_for_test → immediate strict assertion`; a failed assertion exits the test without a later poll.

The retained target-function stack samples identify one case thread: PID7884/TID25870676. The case-owned panic message confirms the strict promotion failure. `promotion.py` separately projects that thread and replays a source-order containment inference. Let S be the failed frame's start, R its timer read and T the thread's kernel-recorded termination time. `R−S≥100ms` and `R≤T` imply `S≤T−100ms`. A preceding poll must return before S, so its off-core interval would have to end at or before `T−100ms`. An interval with a case-owned in-poll stack at switch-out and a later end cannot belong to a preceding call. The immediate panic excludes a subsequent call; it therefore lies inside the failed call. This does not reconstruct an exact frame window.

T is6,182,111,708ns in the trace; the latest possible failed start is6,082,111,708ns. Each of the following non-overlapping Preempted intervals ends later than that bound, with more than1ms margin, and its start timestamp exactly matches a context-switch stack containing both the promotion test and the poll function:

| Start (trace ns) | End (trace ns) | Recorded Preempted (ms) |
| ---: | ---: | ---: |
| 6,059,279,041 | 6,093,512,041 | 34.233000 |
| 6,093,518,625 | 6,104,052,291 | 10.533666 |
| 6,104,057,041 | 6,104,790,833 | 0.733792 |
| 6,104,798,125 | 6,170,912,000 | 66.113875 |
| 6,170,923,416 | 6,172,201,166 | 1.277750 |

Their recorded total is **112.892083ms**, sufficient by itself to exceed100ms. Shrinking each interval inward by1ms at each end, flooring short intervals at zero, still leaves104.880541ms. This is a sensitivity calculation, not a certified bound on every kernel timestamp or loss error. The exact case-owned panic-hook stack also appears at6,172,221,875ns before thread termination; using that tighter upper bound independently selects the same5 intervals.

This is evidence that recorded Preempted time sufficiently contributed to **this new stress-condition failure**. It does not show that the entire failed frame was off-CPU, establish pure involuntary runqueue starvation, identify which competing work caused it, or retroactively diagnose the old file-filter/Walker failures. The clock bridge, native state/stack timestamp correctness, unique case/thread identification and source's immediate-failure control flow are premises. There is no independent certification of zero trace-event loss. All original failures remain FAIL; the evidence does not waive the100ms gate or establish a production fix.

The91.551834ms clocked maximum independently contains91.487375ms Preempted,0.054167ms Running and0.004291ms Interrupted in its conservative inner window. Its switch-out stack is in `Instant::elapsed → PendingBackgroundIndexFinalize::advance`, rather than a CPU-heavy100ms operation. A different clocked frame records4.232542ms Runnable and25.410292ms Blocked, kept separate from Preempted.

## Complete outcomes

All6 predeclared captures ran once and finalized successfully (tracer exit0). Each full suite contains1,679 tests including20 ignored tests. Four suites passed; two failed. No failed test was replaced by a successful retry.

| Capture | Suite PASS / FAIL | Clocked frames | Max wall (ms) | Running (ms) | Preempted (ms) | Interrupted (ms) | Short uncertain windows |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| default-1 | 1,659 / 0 | 1,554 | 4.134458 | 0.779541 | 3.344167 | 0.004541 | 1 |
| default-2 | 1,659 / 0 | 1,524 | 32.858459 | 0.845208 | 32.006541 | 0.000375 | 0 |
| default-3 | 1,658 / 1 | 1,570 | 21.299875 | 0.677419 | 20.610374 | 0.005207 | 0 |
| threads48-1 | 1,659 / 0 | 1,816 | 32.901250 | 0.430582 | 32.461583 | 0.003875 | 5 |
| threads48-2 | 1,659 / 0 | 1,989 | 55.788834 | 0.585997 | 55.191085 | 0.003792 | 13 |
| threads48-3 | 1,657 / 2 | 1,975 | 91.551834 | 0.054167 | 91.487375 | 0.004291 | 17 |

State columns describe the conservative inner window of the **same maximum-wall frame**, not independently chosen maxima; uncertain edges are not distributed among states. All10,428 clocked records are retained. Their native clock offsets are admitted,36 short inner windows remain explicitly unresolvable and the other10,392 have no UNKNOWN coverage gaps. All clocked wall values are below100ms. The separate promotion assertion failure above is still a natural100ms FAIL and is not omitted merely because that path lacked a clocked record. These two observation axes must not be pooled into a claim that TC-207 always passed.

The other failed tests are retained separately:

- `default-3`: `tc_184_restored_preview_kind_resolution_survives_index_refresh`, seed `0x1840002e`, step337, `CompleteNewestIndex(Failed)`; the semantic guard observed unrelated tab15 changing root0→1 and lifecycle Failed→Dormant. Its cause is unresolved; no guard or expectation was changed.
- `threads48-3`: `runtime_config::tests::editable_settings_save_preserves_unknown_json_and_defers_effective_config`; its unchanged-config comparison found different runtime settings, including search threshold7 versus25,000 and auto-check false versus true. Private absolute paths remain in the local raw logs. This failure is not a TC-207 wall failure and its cause is unresolved.

The conditions intentionally differ in parallelism, while ambient host activity and tracer overhead are not controlled. Three runs per condition are exploratory evidence, not a failure-frequency or production-capacity estimate. The original historical failures, this new stress-condition promotion failure and unrelated functional failures retain separate identities.

## Retained evidence and validation

[evidence.tar.gz](evidence.tar.gz), SHA-256 `bd87825ea5d74e69560896eb8c2442a5252eb8cd3f7054d5aa6c797773d79eaf` (8,559,002 bytes), contains95 regular files with an internal SHA-256 inventory. It retains every capture receipt, all10,428 clocked frames and their target-only state/stack inputs, summaries, separate promotion-thread inputs/proof, all suite totals and failed names, the pre-run plan, source/compiler/binary hashes, frozen Rust/helper sources, parser failure versions and RED/GREEN logs. Original raw traces, XML, TOC and full stdout/stderr remain in private Git-ignored local storage. Exact log/export hashes and post-export raw-bundle file-tree digests preserve their identities; bundle entry-name inventories remain private. The digest covers file bytes and symlink targets, excluding filesystem metadata/xattrs.

Parallel stdout interrupted some individual libtest result lines. Those per-test entries remain UNKNOWN/AMBIGUOUS in `test-outcomes.json`; they are not reconstructed as PASS. The authoritative harness totals and all unique failed names remain available. This result-line uncertainty is separate from the36 short frame-attribution windows. The initial inventory's strict per-line parser rejection is also retained. All missing lines, failed tests, analysis-cap errors and corrected analyses preserve their original axes.

Extract into a new private directory, verify every `sha256.json` member, then use the archived `method/natural-reproduction/project.py` and `promotion.py` with their adjacent `method/system-trace/analyze.py`. Recompute `summarize(inputs)` for each capture and `prove(promotion_inputs)` for the failed case; compare to the retained summaries/proof without overwriting originals. The canonical promotion extractor was independently run against the same raw XML and matched all8 numeric/admission fields of the retained exploratory probe; its proof replay also matched.

VM-001 is selected because only historical diagnostic helpers/evidence and navigation docs changed. No Rust/runtime code changed. Python16 focused tests, the existing21 trace helper tests, repository-contract/quick validation, archive/hash/source/replay checks and local-link/diff checks pass. Failing regressions preceded the new parser/proof behavior; no application TDD or repeat Cargo build is needed for this evidence-only diff. The6 full lib-suite executions above remain experimental PASS/FAIL evidence, not a claim that a fresh product validation was universally green. Independent read-only review checked the inference, source guards and failure retention; no threshold or failing expectation was altered.

Native GUI operations, Windows performance/clipboard, real theme notifications, share disconnect/recovery and WSL guest/Windows host-vCPU tracing were NOT RUN in this campaign. Native Mac trace execution is complete. The old historical file-filter/Walker causes, the precise wall/CPU totals of the new failed promotion frame, the distinction between voluntary yield and involuntary preemption, and the causes of the two unrelated failures remain unresolved. There is no product fix or release publication in this evidence-only change.
