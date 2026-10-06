# Accepted active v7 indexing calibration

Fixed collector source `212cd502c19b6bc36e02c7b04d2000beaeb7e31d`, native-validated reference `9640bef9884525ea641087d09122541877f40f29`, protocol `same-job-RCR-f1-matched-21-observer-v7`. All original roots and normal source CLI executions enable timing enforcement. Candidate medians must not exceed 1.50 times the slower same-job reference median; symmetric reference median drift must not exceed 1.25. MAX remains diagnostic.

| Session | Role | Whole workflow |
| --- | --- | --- |
| 1 | Exploratory | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37435020193) |
| 2 | Exploratory | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37440464651) |
| 3 | Exploratory | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37446055847) |
| 4 | Held-out | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37451067830) |
| 5 | Held-out | [PASS](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37455665120) |

All five attempt-1 runs completed all three groups. The reservation was fixed at 08:01:46 UTC on 2026-10-06, before any new outcome. The first three whole passes were frozen at 10:37:57, before held-out session 4 was created at 10:38:01. No source, threshold, cadence, case, sample or outcome was refit. All 17 cells retain 8,190 raw leg rows, 680 observations (340 mandatory medians and 340 MAX diagnostics), 136 complete five-session families and 45 distinct component UUIDs. Cadence remains 21/21/7 for f1/matched/stable.

The largest candidate median ratio was 1.0431323373132713; the largest reference median drift was 1.0781768872505233. Candidate MAX ratio 1.2726168638382833 is diagnostic. Ratios describe individual comparisons; absolute timings from different runners are not pooled. No isolated-delay, p95, false-positive or future-stability guarantee is inferred.

Source-before/after, execution context, work, request/body, oracle, input chronology, process reaping/group absence and fixture cleanup passed. Across 15 jobs, maximum shared build was 720.59 seconds, shared measurement 2,061.05 seconds and job duration 2,796 seconds, below fixed 1,800/2,700/5,400 second limits. The unchanged controller enforces a 21,600-second monotonic campaign deadline. Reservation-to-completion was about 14,700 seconds; that wall-clock diagnostic is separate from the deadline guard.

The archive preserves 504 original files in 22,493,952 compressed bytes. Every member was hash-checked after round-trip; all five whole workflow ZIPs and retained source-review ZIPs passed CRC. All 15 original group comparisons were replayed with the exported actual-source normal CLI, with identical reports and every sample retained. [Manifest](evidence-manifest.json), [original-source replay](original-source-replay.json), [descriptive families](descriptive-summary.json), [review subjects](acceptance-subjects.json) and [independent acceptance](calibration-acceptance-review.md) bind these results. The archive includes before/source/controller reviews, the actual exported three Python files and verification code.

Collector source 212 was admitted after [ordinary CI](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37431395078) and [Guardian](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37431392737) attempt 1 succeeded. Reference 9640 passed [ordinary CI](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37427336513) and [Guardian](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37427334186) attempt 1. Both required checks, native Linux/macOS/Windows, coverage, builds, Clippy and GNU updater E2E passed. Library counts were 1,591/1,582/1,614/1,591 with zero failures. Cargo audit correctly skipped unchanged Cargo inputs. All 259 Rust/workflow/PowerShell/Cargo/build inputs match between collector and reference.

Independent review accepted this source-bound five-session calibration and corrected published active execution with no findings. The same five actual enforced executions provide both axes; no extra unchanged pre-merge performance run was used. Overall FINAL, protected merge and actual rebase-merged master proof are separate gates tracked in [PR 173](https://github.com/ShinjiKawamura255/flist-walker/pull/173). This record does not relabel the collector as a later HEAD or master execution.

The prior F60 inactive calibration, C650 active chronology failure and 41dc native failure keep their original scopes. The old settings Load-versus-positive-MAX-Save phase and earlier native/E2E causes remain unknown. Current diagnostics, native success and calibration do not establish original-worker causal repair or original-AFDD full test-program/binary equivalence. Production behavior, build inputs and numeric thresholds are unchanged.
