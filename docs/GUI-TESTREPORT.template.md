# GUI TESTREPORT

## Summary
- Date:
- Tester:
- Build/version:
- Commit:
- Scope:
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
- Overall: NOT RUN (required axis NOT RUN)

## Results
Each axis cell uses `STATUS — reason — evidence — reproduction`.

| ID | Deterministic | Native interaction | Liveness | Notes |
| --- | --- | --- | --- | --- |
| GSM-001 | NOT RUN — run deterministic wrapper | NOT RUN — startup/focus/typing | NOT RUN — run isolated headful smoke | Startup/indexing |
| GSM-002 | NOT RUN — run deterministic wrapper and VM-004 | NOT RUN — search/highlight/operators | NOT RUN — supporting only | Search/highlight/operators |
| GSM-003 | NOT RUN — run deterministic wrapper | NOT RUN — preview and selection movement | NOT RUN — supporting only | Preview/selection |
| GSM-004 | NOT RUN — TC-050/051 recording seams | NOT RUN — explicit authorization required | NOT RUN — supporting only | Open/copy routing; deterministic PASS may coexist with native NOT RUN |
| GSM-005 | NOT RUN — run deterministic wrapper | NOT RUN — sort modes/typing | NOT RUN — supporting only | Sort modes |
| GSM-006 | NOT RUN — run deterministic wrapper | NOT RUN — fixture source/dialog interaction | NOT RUN — supporting only | FileList/dialogs |
| GSM-007 | NOT RUN — run deterministic wrapper | NOT RUN — tabs/reorder | NOT RUN — supporting only | Tabs/per-tab state |
| GSM-008 | NOT RUN — run deterministic wrapper | NOT RUN — local/forced dialogs only | NOT RUN — supporting only | Dialog cancel/failure |
| GSM-009 | NOT RUN — run deterministic wrapper | NOT RUN — light/dark visual pass | NOT RUN — supporting only | Theme/contrast |
| GSM-010 | NOT RUN — run deterministic wrapper/perf gates | NOT RUN — responsiveness during native input | NOT RUN — run isolated headful smoke | Responsiveness |

## Native Residuals
| Case | Status | Reason | Evidence | Reproduction |
| --- | --- | --- | --- | --- |
| Real UNC authorization | NOT RUN | Authorized reachable share unavailable or not approved | | Follow TC-051 with masked server/share names. |
| Japanese IME composition | NOT RUN | IME/input environment unavailable | | Use Windows Japanese IME and GSM-002/010. |
| DPI scale change | NOT RUN | Alternate DPI not exercised | | Move the staged window between configured scale factors. |
| Multi-display movement | NOT RUN | Multiple displays not exercised | | Move the staged window across displays and restore. |
| External open/copy | NOT RUN | Explicit authorization required | | Use fixture targets only after authorization. |

## Defects
- None recorded.

## Follow-ups
- Record SKIPPED only with a concrete environment reason.
- Keep Deterministic, Native interaction, and Liveness independent; never promote liveness into native PASS.
- Store screenshots/logs under `rust/target/gui-smoke/evidence/` when a release-candidate run is performed.
