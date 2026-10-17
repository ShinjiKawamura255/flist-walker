# WarmReclaim repair and source-bound v9 measurements

This record retains completed native/measurement evidence for [PR188](https://github.com/ShinjiKawamura255/flist-walker/pull/188). Measurement acceptance is complete; independent FINAL, latest-head protected checks and merge are separate gates. No release or actual master-source performance execution is claimed here.

## Failure and repair

The original [macOS run39042890365, job117425811574](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39042890365/job/117425811574) observed zero new allocations where the WarmReclaim harness immediately required one for inactive tab2 at initial FileList refresh. Its child was ready, reaped and cleaned; the180-second watchdog did not expire. The original early-return branch was not logged.

A controlled real bounded-reclaimer Full state with retained empty build capacity reproduces that zero-versus-one assertion with allocation history enabled. Production already supports deferred refresh. The repaired shared driver requires either exactly one immediate target-tab request or the actual inactive target-tab deferred flag. Repeated deferred clicks coalesce; subsequent real frames must allocate exactly one owned request and satisfy all existing terminal, oracle, actual producer-overlap and physical cleanup requirements. Missing deferral or unplanned allocation still fails. No pre-drain, sequentialization, extra deadline, worker/queue change or production scheduling repair was added. Controlled reproduction proves the invalid immediate-allocation assumption; it does not identify the unobserved branch or OS latency of the original execution.

Parallel controls independently exposed timestamp-only fixture-root collisions and strict-teardown ENOENT. The old concurrent same-name control produced408 distinct paths from1,024 requests; process ID plus fixed-width atomic sequence produces1,024 unique paths while retaining strict cleanup. This is a separate observed test-isolation defect.

The [original Windows Flush failure38941365054](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/38941365054) did not expose enough phase/reply information to attribute scheduler, lock or I/O latency. The history readiness repair waits for the final accepted generation to be successfully persisted before the unchanged1-second Flush wait, preserving the original2-second readiness bound. Test-only observations distinguish timeout and disconnection. RED/GREEN proves the repaired readiness contract, not the original Windows latency's cause.

[V8 session1](../warm-reclaim-v8-2026-10-18/rejected-v8-session1.md), sourcee8/reference9dd, then failed strict reference-before admission in all three groups because the Python validator omitted existing `theme="system"` and `restore_queries_enabled=true` serialized defaults. No candidate or reference-after leg and no numeric comparison were admitted. All original v8 raw/invalid receipts/controller/reservation/logs remain rejected in their own archive; no successful subset, unchanged rerun or old result entered v9.

The repair admits exactly those existing values and still rejects missing/unknown keys, wrong types and nondefault values. Actual Rust Default serialization is checked against the same test-only JSON witness consumed by Python synthetic controls; only CPU-capped thread count and trace-path prefix are normalized. Production configuration/defaults/persisted format are unchanged.

## Source and native admission

- Base: `5c4c9eee9af50a7d1a9ced5d1e285481d5904cc5`.
- Actual healthy reference: `6edfb599d69fc31657032a1419bdd255a165fe80`; [native admission](native-reference.md), [CI39049915613](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39049915613), [Guardian39049914105](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39049914105), attempt1 whole PASS.
- Actual executing collector/candidate: `63fc68aa72b873d1c8181f3d3f6cf37cfc94d9c5`; [CI39050926174](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39050926174), [Guardian39050924217](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39050924217), attempt1 whole PASS before reservation/dispatch.
- All applicable Linux/macOS/Windows, coverage, platform Clippy, builds, Windows GNU updater E2E and both required checks passed. At each source, library counts are Linux1626/macOS1617/Windows1647, zero failures; pre-existing ignores19/19/20 are unchanged. Cargo inputs are unchanged and hosted audit correctly skipped; the retained fresh local audit passed.
- Actual collector CI checkout `f295e43cb5ba6e033114c081850eee2e69372c2e` has tree `45527cb1866820715039ed83bde4cce6e1d79e5e`, equal to the collector tree. Reference CI tree equality is recorded separately. Synthetic PR merge checkouts are never relabelled as source commits.
- All275 native/build inputs,240 complete Rust inputs and7 measured WORK_FILES match reference/candidate. Only comparator reference/protocol assignments differ; all numeric/default-enforcement controls remain. The new Rust schema witness affects full test-program identity, so old v7/v8 data cannot substitute for this new reference/source campaign. Every original v9 comparison reports full-Rust null equality TRUE.

## Prospective protocol and results

Protocol `same-job-RCR-f1-matched-21-observer-v9` retained default active policy `rcr-median-v3`: each of68 candidate medians must be at most1.50 times the slower reference median, and symmetric reference-median drift must be at most1.25. All68 MAX observations remain diagnostic. Source/work/request/overlap/oracle/physical/process/fixture admission remains mandatory.

All five slots, their three-exploratory/two-held-out roles, source/reference/policy,31 support hashes and controller hash were frozen at `2026-10-17T17:04:43.511853+00:00`, before any v9 performance outcome. Each whole session has all17 cells at100,000 inputs,21 f1/matched pairs and7 stable pairs, three RCR legs,1,638 rows,68 mandatory medians,68 MAX diagnostics and nine distinct component identities. Whole workflows ran serially; any failure/invalid/indeterminate result would stop the controller without retry, refit or subset adoption.

The first three whole successes were frozen before fourth dispatch by the reserved controller. A separate retained observation at2026-10-17T19:38:04.334438+00:00 recorded the exact frozen hash/first-three run IDs and unchanged policy while fourth run39063291829 was queued with no indexing outcomes. Both held-out slots were already reserved, and nothing was retuned.

| Role | Actual whole workflow, attempt1 | Whole/normal source CLI | Worst mandatory median ratio | Worst reference-median drift |
| --- | --- | --- | --- | --- |
| exploratory | [run39052188724](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39052188724) | PASS | 1.015492 | 1.042269 |
| exploratory | [run39055878997](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39055878997) | PASS | 1.021667 | 1.071214 |
| exploratory | [run39059596332](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39059596332) | PASS | 1.027726 | 1.029203 |
| held-out | [run39063291829](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39063291829) | PASS | 1.028806 | 1.041438 |
| held-out | [run39066816025](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39066816025) | PASS | 1.027934 | 1.080157 |

All15 groups passed the original exported source CLI in normal Python with active enforcement, then all15 original groups were replayed again by the prospectively retained verifier. Independent recount gives8,190 raw rows,680 observations partitioned into340 mandatory medians and340 MAX diagnostics, and45 unique component IDs. Worst mandatory candidate ratio is1.028805618; worst reference-median drift is1.080156828. Worst candidate MAX ratio is1.429095975, diagnostic only. No raw sample was removed or rewritten, and no absolute timing cohort was pooled across runs.

Maximum shared build/measurement/job times are814.193/2273.017/3028.000 seconds, within unchanged1800/2700/5400-second bounds. Campaign completion was `2026-10-17T21:18:25.688159+00:00`, 15222.176 seconds after reservation, within21600 seconds. Original process/cleanup receipts and actual job timestamps are retained.

## Durable evidence and limits

- [Original artifacts archive](accepted-v9-original-artifacts.tar.gz): 23,113,550 bytes, 453 files; SHA256 `ccb9d47ca1387f82b1a64781910757938abbffe2444fbc21e0a0dcfe2538ac07`. Every member's size/SHA256 and round-trip byte equality were checked in the [manifest](accepted-v9-evidence-manifest.json).
- [Recount/source-CLI report](accepted-v9-recount.json) retains all five run/group identities and actual15 normal CLI replays. The archive contains original raw/build/discovery/receipts/reports, actual source scripts, frozen reservation/controller/support, before-review record, native/source CI originals, all five whole-workflow log ZIPs, original macOS failure and controlled RED/GREEN/fixture-isolation logs. ZIP CRC checks passed.
- Local full Rust1617/0/19ignored and integrations/menu/docs, all9 normal TC229 leaves at42cells/4096 inputs, format/Clippy, Python123, collector26 normal/-O/-OO/PYTHONOPTIMIZE1/2, contract/trusted-policy checks and controller39 offline controls passed at their recorded source checkpoints. Synthetic controls are not performance calibration.
- Explicitly authorized isolated macOS GUI interaction at original Light/history source37d passed Light/Dark search matches, FILE/DIR/LINK labels, selected/current rows and pointer/Down behavior; staged binary SHA256 `eda25aafaa0c02c84c9bcc6c06c6385b7d609573c7ec9f8d1f1d80d6e066949e`, with both task apps closed and host process absence verified. Later changes are test/collector/evidence only. Windows/Linux native GUI interaction is NOT RUN; hosted correctness tests do not promote that axis.
- This healthy-code null campaign validates the current sustained gate and admission program. It does not prove the original failure branch, original Windows latency, old binary equivalence, isolated-delay/p95 behavior, a false-positive guarantee or actual rebase-merged master execution. Protected merge and any master-source run retain separate identities in PR188. No release/tag/settings/trusted-workflow mutation occurred.

Independent overall review, latest-head protected checks and merge retain separate evidence in PR188. This measurement record supplies no overall merge approval.

## Validation surfaces for final review

| Surface | Actual source/evidence | Result and limits |
| --- | --- | --- |
| Automated native | Reference6ed and collector63fc exact whole CI/Guardian, original logs in archive | PASS, all applicable jobs and required checks, native Windows/macOS/Linux, coverage/build/GNU E2E |
| Automated local Rust | Schema repair6ed full `cargo test --locked`, format and all-target Clippy | PASS, library1617/0/19ignored plus applicable integrations/menu/docs; source Rust inputs match collector63fc |
| Automated normal harness | Nine real physical TC229 leaves,42cells/4096 inputs | PASS, original180s watchdogs/request/oracle/overlap/cleanup guards retained |
| Controlled/simulated | Real bounded-reclaimer Full, immediate/missing-flag/coalescing controls; concurrent root guard; shared schema witness/negative controls | PASS with retained RED/GREEN; synthetic controls never calibrate performance |
| Automated collector | Python123, collector26 optimization/environment modes, offline controller39 | PASS; exact numeric/default enforcement remains active |
| Automated real measurements | Five whole sessions, all15 original group normal source-CLI replays and independent recount | PASS,8190rows/340mandatorymedians/340MAXdiagnostics/45IDs; all full-Rust null flags TRUE |
| Manual changed GUI subset | Original37d explicitly authorized isolated macOS Light/Dark apps, selected/current rows, matches/kind labels, pointer/Down | PASS at that exact source; apps closed and host absence checked; later production GUI source unchanged |
| Manual Windows/Linux GUI | No native interaction session for this task | NOT RUN; hosted tests do not certify this axis |
| Documentation/contracts | `validate_change.py --base origin/master --plan` and `--quick`, permanent evidence/current routing on collector63fc worktree | PASS, VM001/002/008/009 selected, repository contract and Python123/0 in48.690s; no production indexing coordination change triggers VM003 conditionals |

The final independent reviewer must assess the exact base/head plus permanent worktree evidence before the durable-evidence checkpoint. Temporary task controls remain unstaged through latest-head CI and authoritative protected merge; after all user goals are met they are removed with task hash/clean-state proof, without an empty commit. Rollback uses task commits through a PR. No new release, tag, repository-setting or trusted-workflow authority is implied.
