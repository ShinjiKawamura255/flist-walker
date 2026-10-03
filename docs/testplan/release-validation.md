# Release Validation Selection

## Policy

Validate behavior when it changes. A routine release reconciles those results and checks the new distribution; it does not repeat the complete product evaluation. This document owns release selection and evidence reuse. The [Validation Matrix](validation-matrix.md) owns change-time VM requirements, and [GUI TESTPLAN](../GUI-TESTPLAN.md) owns native procedures and safe sessions. Test thresholds, product contracts, required CI checks and publication authority are unchanged.

Before candidate work, the release operator records one selection table for the complete release delta from the previous public tag, then reconciles every later change through the candidate SHA. A last-minute patch adds only its affected checks; comparing only the last patch is insufficient. Use `python3 scripts/validate_change.py --base <previous-public-tag> --plan` and inspect the diff and dependency boundaries. VM routing is a candidate list, not an instruction to execute all commands or the entire GSM matrix.

## Every Release

| Gate | Required evidence |
| --- | --- |
| Release delta and readiness | Prior public tag, exact candidate SHA/run, selected affected contracts, unresolved defects, and source evidence for each required check |
| Protected source and CI | Required PR checks and the candidate workflow's existing Linux/macOS/Windows test, clippy, audit and build jobs for the exact source; use completed run results instead of repeating the same local commands |
| Version and distribution | Version/changelog/tag consistency, exact asset inventory, current archive/sidecar license content, binary identity, checksums, signing key consistency and detached signature verification |
| Previous-version compatibility | Latest public version read-back and the real candidate manifest accepted by the N-1 checker; non-increasing versions or incompatibility remain blockers |
| Run warnings and publication | Completed logs, existing warning disposition rules, no overwrite of public tags/assets, draft review and publication read-back |

These gates concern a newly produced artifact or current external state and cannot be satisfied by an unrelated old artifact. Unchanged checker/packaging self-tests may reuse valid change-time evidence; the actual candidate manifest, inventory and signature must still be checked. A coverage result already supplied by required CI is sufficient; no duplicate local coverage run is required. Audit remains fresh because advisories can change without source changes. Existing workflow enforcement is not weakened by this document.

## Change-Triggered Evaluation

Run these checks at implementation/PR time, including the necessary native OS observation. At release time reuse eligible completed evidence. A version bump, a release date, a new whole-executable hash or an unrelated change alone is not a retest trigger.

| Changed responsibility or invalidation trigger | Selected additional evaluation |
| --- | --- |
| GUI rendering, dialogs, layout, preview or theme | Changed normal/edge states and adjacent controls in affected GSM flows; shared rendering changes cover Windows/macOS, a platform-only adapter covers that platform |
| Key mapping, focus, text editing, IME or window geometry | Changed input surfaces and modal/background isolation; IME only for composition/input changes, alternate DPI/multiple displays only for scale/monitor/geometry/backend changes |
| Tabs, restore, persistence or runtime defaults | Changed in-process and cross-process contracts, isolated save/restart, compatibility/error paths and relevant GSM-007/008/013 subflows |
| Index/search/cache/worker coordination | Selected VM-002/003/004/010 regressions and their conditional performance/pressure tests; native responsiveness only where the changed path affects it |
| CLI startup/one-shot engine, shared indexing/search, Windows entrypoint/subsystem/linking, release profile/compiler or relevant dependency | TC-193 Windows performance on the affected release build; unchanged 5 warmups/25 samples and ratio <=0.70. Import/subsystem/resource inspections still apply to new Windows assets |
| Action target resolution, command dispatch or platform handler adapter | TC-050/051/164 and affected external Open/Reveal/Copy/UNC axes; real UNC only when network-path authorization is affected |
| Updater trust/download/activation or update GUI | Affected TC-157..160/171/186..188/191/194/200/215 and native dialog/loopback/transaction axes; real installed binaries are never a test fixture |
| Dependencies/toolchain/backend/build/packaging/assets | Component impact review; platform startup/launch smoke for changed bootstrap/subsystem/bundle behavior, affected GUI/input/performance checks, and selected VM-005/006/009 gates |
| Documentation, test-plan text or release-note wording only | Reference/contract/diff review and selection consistency; no Rust, coverage, native GUI, performance, fixture regeneration or wrapper rerun solely because a document changed |
| New defect, crash, regression report, changed supported environment, or uncertain boundary | Reopen the implicated checks; expand to the dependency boundary that cannot be shown unaffected |

For broad architecture/backend migrations or a new platform, select the full applicable matrix for the affected scope. Full platform certification requires a concrete broad-impact reason or an explicit user request; neither each release nor each minor version implies full certification. Scheduled/nightly checks keep their own cadence.

Select subflows/axes rather than entire large GSM rows. When a shared reducer/widget has exhaustive deterministic coverage, native observation may use one representative surface per distinct input/OS adapter and relevant boundary/error state; it need not repeat every key x field x theme combination. Record which surfaces share that implementation and which distinct adapters still require observation. A changed unshared route, platform delivery, focus or modal ownership cannot be inferred from another surface. For example a Settings footer layout fix requires short-window/footer/focus checks, not Open JSON, all seven saved fields, IME, every preset field and signed updater restart unless their shared implementation changed. A Preview scroll fix selects body/outer scrolling, controls and keyboard reachability; unchanged CSV parsing/copy and pending-read routing may reuse their own evidence. Shared widget/backend changes can invalidate more than one surface and must not be labeled local just because the diff is small.

## Evidence Reuse

Evidence carries its original identity and result. It never becomes a claim that the new executable was tested. The selection table separates execution status from release disposition:

- `RUN`: a selected check needs fresh evidence, or an incomplete/failed affected check remains unresolved.
- `REUSE`: the selected check is satisfied by an eligible earlier PASS; link that result and the no-impact comparison.
- `NOT REQUIRED`: no trigger applies. Keep any historical NOT RUN/FAIL visible; this is scope selection, not a new test result or a waiver for a known product defect.
- `DEVIATION`: a required check is unresolved and the user has explicitly accepted the version/scope-specific exception. Keep formal FAIL/NOT RUN and the residual risk.

Reuse requires all of the following:

1. A retrievable source result with check/subflow, outcome, tested SHA/binary identity, OS/environment/fixture, command or native procedure, and a durable PR/run/versioned record (retained dated evidence may support the active task and must be summarized durably before closure).
2. All changes from that tested source to the candidate are reconciled, including intermediate commits, relevant callers, shared adapters, dependency features/lockfile, compiler/build options, defaults and persistent formats. Record the exact comparison and why the tested contract is unaffected; an unchanged file alone is insufficient.
3. Equivalent supported environment and fixtures for the claimed contract, no relevant advisory/incident/new defect, and no later FAIL that supersedes the PASS. Missing or expired source artifacts must be recovered or the check rerun.
4. For change-time native evidence on a local working build, reconcile its source patch to the merged code and candidate build/metadata. OS-specific behavior cannot be transferred to another OS; changed relevant features/bundle metadata/signing/launch path need their affected checks.

Binary/source identity changes invalidate artifact-specific evidence (digest/signature/inventory/startup for a changed launch path) and affected behavior, not every functional result. Pure text documentation changes do not invalidate product evidence. A failed native regression requires fresh native confirmation of the fix unless an explicit deviation covers that defect; deterministic PASS alone cannot close it. No failed threshold, expected result or test is changed to obtain PASS.

A previous NOT RUN on an unaffected, unselected subflow is tracked as validation debt and does not block a routine release by itself. A new/changed feature with missing required change-time evidence is selected RUN/DEVIATION; absence of any prior baseline cannot justify REUSE. Known unresolved product defects remain release blockers until fixed and confirmed or explicitly accepted, even if a later release contains no further change in that area. Tool/session failures are NOT RUN with the actual tool error, not product FAIL or proof that the device is locked.

## Selection Record

Use this table in the existing PR/release packet; do not create another parallel plan. The release operator owns selection and final reconciliation. Independent release/change review checks the trigger and reuse reasoning. Normal selection and valid reuse need no additional user approval; unavailable selected requirements or accepting a defect do.

| Check / GSM subflow and axis | Trigger / affected contract and OS | Disposition | Original result and source identity | Tested-source to candidate comparison / no-impact reason | Fresh result or remaining action |
| --- | --- | --- | --- | --- | --- |

A prerequisite inventory includes only selected native axes. Do not request displays, IME, clipboard clearance, handlers or signing material for unselected flows. Append revised selections/results after a source change; preserve old observations rather than rewriting history. Existing explicit version-scoped deviations retain their authorized scope after a candidate change when its impact and applicability are recorded; they do not extend to another version, new defects outside that scope or exact-run warning approvals.

## Selection Examples

| Delta / evidence | Decision |
| --- | --- |
| README wording only; product and build inputs unchanged | Every-release artifact gates; no functional/GUI/performance rerun |
| Narrow Settings/Results layout; prior CLI/index PASS | Changed/adjacent GUI and theme checks; reuse CLI/index/TC-193 if dependency comparison establishes no impact; new asset identity/signature checks remain required |
| Search evaluator/one-shot cache changed | Focused matching/parity plus selected search performance and TC-193; unrelated Settings persistence is not repeated |
| Rust/egui/backend or linker version changed | Review actual dependency boundary; startup and affected rendering/input/platform/performance checks are reopened; old hash difference is not the reason by itself |
| GUI tool cannot operate an unlocked desktop | Required affected native checks remain NOT RUN; unrelated unselected residuals are not promoted to blockers |
| Original narrow-window native FAIL followed by rendering fix | Fresh native confirmation of that defect remains RUN; headless green is supporting evidence |
