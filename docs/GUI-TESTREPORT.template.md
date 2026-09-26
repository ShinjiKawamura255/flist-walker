# GUI TESTREPORT

## Summary
- Date:
- Tester:
- Build/version:
- Commit:
- Scope:
- Execution profile: change-focused candidate / platform certification / residual addendum
- OS/display:
- Fixture command: `scripts/gui-smoke-fixture.sh`
- Fixture root: `rust/target/gui-smoke/root`
- Evidence dir: `rust/target/gui-smoke/evidence/`
- Deterministic command:
- Headful command:
- Staged executable:
- Settings isolation:
- Fixture hash/count validation:
- Pre/post staged allowlist and updater-artifact check:
- Axis overall: NOT RUN (required axis NOT RUN)
- Release outcome: BLOCKED / PASS / ACCEPTED WITH DEVIATION

## Session Prerequisites

Complete this table before the first native launch. Add exact prerequisites required by the selected scope.

| Prerequisite | Available | Evidence / reason unavailable | Affected axes |
| --- | --- | --- | --- |
| Native OS session | No | | |
| Required display and DPI topology | No | | |
| Required IME | No | | |
| Authorized masked UNC fixture | No | | |
| Disposable owned external handler | No | | |
| Clipboard zero-format safety gate | No | | |
| Loopback listener/feed and signing material | No | | |
| Isolated restart profile | No | | |

## Results
Each axis cell uses `STATUS — reason — evidence — reproduction`.

| ID | Deterministic | Native interaction | Liveness | Notes |
| --- | --- | --- | --- | --- |
| GSM-001 | NOT RUN — run deterministic wrapper | NOT RUN — startup/focus/typing | NOT RUN — run isolated headful smoke | Startup/indexing |
| GSM-002 | NOT RUN — run deterministic wrapper and VM-004 | NOT RUN — search/highlight/operators | NOT RUN — supporting only | Search/highlight/operators |
| GSM-003 | NOT RUN — run deterministic wrapper | NOT RUN — preview and selection movement | NOT RUN — supporting only | Preview/selection |
| GSM-004 | NOT RUN — TC-050/051 recording seams | NOT RUN — satisfy the separate external-action and clipboard safety gates | NOT RUN — supporting only | Open/reveal and Copy Path are distinct native axes; deterministic PASS may coexist with either native NOT RUN |
| GSM-005 | NOT RUN — run deterministic wrapper | NOT RUN — sort modes/typing | NOT RUN — supporting only | Sort modes |
| GSM-006 | NOT RUN — run deterministic wrapper | NOT RUN — fixture source/dialog interaction | NOT RUN — supporting only | FileList/dialogs |
| GSM-007 | NOT RUN — run deterministic wrapper | NOT RUN — separately record cross-process SavedTabState restore and in-process closed-tab/Recent-Inactive behavior | NOT RUN — supporting only | Do not expect sort/PIN/selection/results/preview/lifecycle/heavy snapshots from cross-process SavedTabState |
| GSM-008 | NOT RUN — run deterministic wrapper | NOT RUN — help, configured Ctrl+W focus/IME behavior, preset picker/editor, Named Root manager, and local/forced dialogs only | NOT RUN — supporting only | Help/keyboard configuration/picker/editor/manager/dialog cancel/failure |
| GSM-009 | NOT RUN — run deterministic wrapper | NOT RUN — light/dark visual pass | NOT RUN — supporting only | Theme/contrast |
| GSM-010 | NOT RUN — run deterministic wrapper/perf gates | NOT RUN — responsiveness during native input | NOT RUN — run isolated headful smoke | Responsiveness |
| GSM-011 | NOT RUN — run deterministic wrapper and TC-180 | NOT RUN — limited-to-Unlimited Apply, popup/editor checkbox alignment, preset reset, and tab-local interaction | NOT RUN — supporting only | Maximum depth and presets |
| GSM-012 | NOT RUN — run paged-preview group | NOT RUN — selection/copy/focus/scroll, Japanese/long line, CSV/TSV column colors in both themes, pointer Color toggle, and documented keyboard Color route | NOT RUN — supporting only | Record pointer and keyboard Color activation separately; absence of a reachable keyboard route is FAIL |
| GSM-013 | NOT RUN — run settings-dialog group | NOT RUN — isolated save/restart and owned JSON editor | NOT RUN — supporting only | Settings dialog and JSON route |

## Native Residuals
| Case | Status | Reason | Evidence | Reproduction |
| --- | --- | --- | --- | --- |
| Real UNC authorization | NOT RUN | Authorized reachable share unavailable or not approved | | Follow TC-051 with masked server/share names. |
| Japanese literal input | NOT RUN | Committed Unicode input not exercised | | Type a benign Japanese literal into the staged query and read back rendering/result response. |
| Japanese IME composition | NOT RUN | Staged-window-only switch/restore/read-back or composition events unavailable | | Use Windows Japanese IME and GSM-002/010 without changing an unrecoverable host input state. |
| DPI scale change | NOT RUN | Alternate DPI not exercised | | Move the staged window between configured scale factors. |
| Multi-display movement | NOT RUN | Multiple displays not exercised | | Move the staged window across displays and restore. |
| External open/reveal | NOT RUN | Disposable owned handler/session unavailable | | Use only a fixture target and an isolated handler/session that cannot affect default-app MRU or reuse an existing window. |
| Copy Path/clipboard | NOT RUN | Clipboard safety gate not satisfied | | Follow the zero-format/sequence/known-value/post-clear gate in `GUI-TESTPLAN.md`; never read pre-existing content. |
| Updater loopback failure path | NOT RUN | Isolated literal-loopback probe not exercised | | Use a staged target and record listener/feed/redirect, request, hash, artifact, process, and server evidence. |
| Updater signed apply/restart | NOT RUN | Signing or staged-apply prerequisite unavailable | | Use `scripts/manual-self-update-test.ps1` only with its signing prerequisite and private sandbox. |
| Isolated user-settings persistence | NOT RUN | Staged profile restart read-back not exercised | | Change harmless state below isolated profile, restart the same staged binary/profile, and read it back. |

## Release Deviation

Complete only when `Release outcome` is `ACCEPTED WITH DEVIATION`.

- Exact version:
- Exact candidate/run and executable SHA-256:
- Formal `FAIL` / `NOT RUN` axes retained:
- User decision and date:
- Working path or substitute evidence:
- Public disclosure:
- Follow-up owner and next affected release gate:
- Non-inheritance statement:

## Defects
- None recorded.

## Follow-ups
- Record SKIPPED only with a concrete environment reason.
- Keep Deterministic, Native interaction, and Liveness independent; never promote liveness into native PASS.
- Batch the complete set of unavailable prerequisites and run warnings before requesting a decision; do not create one approval request per duplicate emission or per already-known unavailable axis.
- Store screenshots/logs only under the Git-ignored `rust/target/gui-smoke/evidence/`; do not stage, commit, or copy them into `docs/`. Durable ordinary-change evidence is the sanitized PR summary or an exact Actions run URL.
