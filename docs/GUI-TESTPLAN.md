# GUI TESTPLAN

## Scope
- Target version: current working build unless a release candidate is specified.
- Screens/flows: startup, indexing, search, preview, selection, actions, sorting, FileList dialogs, tabs, theme, responsiveness.
- Priority: release-critical manual smoke gate for VM-002 / VM-006 and TC-010 / TC-011 / TC-099.

## Ownership
- Owner: release operator or the engineer changing GUI/app orchestration.
- Frequency:
  - before publishing a release candidate
  - after changes covered by VM-002 that affect render, dialog, focus, tab, search result, preview, or FileList GUI flows
  - after structural refactoring that touches GUI-adjacent app orchestration
- Evidence location: `rust/target/gui-smoke/evidence/`.
- Fixture command: `scripts/gui-smoke-fixture.sh`.
- Launch command after fixture creation: `cd rust && cargo run --bin flistwalker -- --root target/gui-smoke/root --limit 1000`.
- Headful automation smoke:
  - Linux/macOS/WSLg: `scripts/gui-headful-smoke.sh --duration 10`
  - Windows: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-headful-smoke.ps1 -DurationSeconds 10`

## Environment Matrix
| Environment | Required When | Notes |
| --- | --- | --- |
| Linux desktop or WSLg | routine development smoke | Validates default developer path and fixture script. |
| Windows 11 | release candidate or Windows-specific UI/input changes | Required for IME, window movement, Explorer/open behavior, and self-update dialog checks. |
| macOS | release candidate or macOS-specific UI/input changes | Required for command-key behavior and app bundle/manual update expectations. |

## Test Data
1. Run `scripts/gui-smoke-fixture.sh`.
2. Use the printed fixture root as the GUI root.
3. For `cargo run`, the script writes `rust/target/debug/flistwalker.ignore.txt` so the `Use Ignore List` checkbox can be validated without modifying the repository root.
4. Store local notes, screenshots, and logs under `rust/target/gui-smoke/evidence/`.

## Pass / Fail Policy
- PASS: every required `GSM-*` case for the environment is PASS or explicitly SKIPPED with an accepted reason.
- FAIL: any product behavior mismatch, UI freeze, stale dialog, wrong action target, broken selection, or missing evidence for a required case.
- SKIPPED: allowed only for environment-specific cases that cannot apply to the current OS, and the reason must be recorded.
- Flake policy: manual GUI smoke may be retried once for clear environment/display instability. A repeated failure is product or test-plan debt and must be tracked before release.

## Test Cases
| ID | Flow | Steps | Expected |
| --- | --- | --- | --- |
| GSM-001 | Startup and indexing | Launch with the fixture root. Wait for indexing to settle. | Result list appears, status is understandable, first row is selected when candidates exist, and query input accepts typing immediately. |
| GSM-002 | Search and highlight | Search `alpha`, `'alpha`, `!old`, `^README`, `end$`, and `alpha|beta`. | Non-matches hide, operators behave consistently with CLI/unit contract, and highlights are visible on matched text. |
| GSM-003 | Preview and selection | Move current row with arrows, page keys, mouse selection, and preview visibility toggle if available. | Preview follows current row without blocking list movement; binary/unreadable placeholder is not shown for text fixture files. |
| GSM-004 | Open/copy action routing | Select `actions/open-target.txt` and `actions/space name.txt`; exercise open/copy/open-folder shortcuts appropriate to the platform. | Action targets are the selected paths, paths with spaces remain intact, and notices/errors do not freeze the GUI. |
| GSM-005 | Sort modes | Switch `Score`, `Name`, `Modified`, and `Created`; type a query while date sorting is active. | Sorting changes order without losing input responsiveness; returning to `Score` produces a coherent ranked list. |
| GSM-006 | FileList and dialogs | Confirm the fixture is loaded from `FileList.txt`; run Create File List and exercise confirm/cancel paths. | FileList source is visible, dialogs describe the action, cancel leaves state clean, and completion notice is understandable. |
| GSM-007 | Tabs | Create a new tab, switch roots or queries per tab, close a tab, and reorder tabs when supported by the environment. | Each tab keeps its root/query/results; closing/reordering does not swap active tab identity. |
| GSM-008 | Dialog and error handling | Open any available confirmation/failure dialog path such as FileList overwrite/cancel or update-check failure when configured for manual testing. | Dialog focus, default action, cancel action, and returned notice are clear and do not leak stale state. |
| GSM-009 | Theme visual pass | Check light and dark theme, especially selected row fill, tab accent, highlight color, and disabled controls. | Contrast and selected/focused states remain readable; no obvious layout clipping in the main panels. |
| GSM-010 | Responsiveness | While indexing or switching roots, type, backspace, move selection, scroll results, and cancel pending dialogs. | UI remains interactive; long work is reflected by status/progress instead of freezing the event loop. |

## Automation Boundary
- Automated unit/headless coverage remains in Rust tests for render snapshots, `run_ui_frame`, shortcuts, tabs, dialogs, update commands, and index pipeline state.
- The headless GUI surface snapshot MUST cover the visible app contract that can be asserted without opening a native window: active root, query text, filter toggles, ignore-list toggle, result sort mode, result count/current row target, pinned selection count, tab count/active tab, preview visibility/width, top actions, status line, and FileList/update dialog labels/buttons.
- When adding GUI controls whose state is visible without native platform interaction, add or update a headless snapshot assertion before relying on manual `GSM-*` smoke coverage.
- Headful automation is a release/nightly smoke gate only. It launches the native GUI against the standard fixture, treats early process exit as FAIL, writes `GUI-HEADFUL-SMOKE.local.md`, and then stops the process after the configured duration.
- The headful smoke does not replace `GSM-*` manual checks because it does not assert typed search, visual highlight quality, platform open behavior, IME, or window movement.
- Pull-request CI does not require native GUI launch unless a deterministic platform harness is explicitly added later.
- CI continues to own `cargo test`, clippy, coverage, audit, and performance gates.

## Risks
- Manual evidence can be skipped under time pressure.
  - Mitigation: release candidates require `docs/GUI-TESTREPORT.md` or a generated local report to be filled with evidence paths.
- Environment-specific behavior may be under-tested on non-release changes.
  - Mitigation: Windows/macOS are required for release candidates and platform-specific UI/input changes.
