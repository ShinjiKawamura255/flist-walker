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
| GSM-004 | NOT RUN — TC-050/051 recording seams | NOT RUN — satisfy the separate external-action and clipboard safety gates | NOT RUN — supporting only | Open/reveal and Copy Path are distinct native axes; deterministic PASS may coexist with either native NOT RUN |
| GSM-005 | NOT RUN — run deterministic wrapper | NOT RUN — sort modes/typing | NOT RUN — supporting only | Sort modes |
| GSM-006 | NOT RUN — run deterministic wrapper | NOT RUN — fixture source/dialog interaction | NOT RUN — supporting only | FileList/dialogs |
| GSM-007 | NOT RUN — run deterministic wrapper | NOT RUN — tabs/reorder | NOT RUN — supporting only | Tabs/per-tab state |
| GSM-008 | NOT RUN — run deterministic wrapper | NOT RUN — help, preset picker/editor, Named Root manager, and local/forced dialogs only | NOT RUN — supporting only | Help/picker/editor/manager/dialog cancel/failure |
| GSM-009 | NOT RUN — run deterministic wrapper | NOT RUN — light/dark visual pass | NOT RUN — supporting only | Theme/contrast |
| GSM-010 | NOT RUN — run deterministic wrapper/perf gates | NOT RUN — responsiveness during native input | NOT RUN — run isolated headful smoke | Responsiveness |
| GSM-011 | NOT RUN — run deterministic wrapper and TC-180 | NOT RUN — limited-to-Unlimited Apply, popup/editor checkbox alignment, preset reset, and tab-local interaction | NOT RUN — supporting only | Maximum depth and presets |

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

## Defects
- None recorded.

## Follow-ups
- Record SKIPPED only with a concrete environment reason.
- Keep Deterministic, Native interaction, and Liveness independent; never promote liveness into native PASS.
- Store screenshots/logs under `rust/target/gui-smoke/evidence/` when a release-candidate run is performed.
