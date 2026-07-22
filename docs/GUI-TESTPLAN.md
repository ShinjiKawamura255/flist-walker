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
- Evidence rule: release-candidate and VM-002 GUI-adjacent checks must record a dated report with environment and separate Deterministic, Native interaction, and Liveness statuses for every required `GSM-*` case. Use `docs/GUI-TESTREPORT.template.md`; a PASS on one axis never implies PASS on another. Chat-only confirmation is acceptable only for exploratory development smoke and must not be used as release-candidate evidence.
- Fixture command: `scripts/gui-smoke-fixture.sh`.
- Deterministic scenario commands:
  - Linux/macOS/WSL: `scripts/gui-deterministic-scenarios.sh`
  - Windows: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-deterministic-scenarios.ps1`
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
1. Run `scripts/gui-smoke-fixture.sh`. It copies the checked-in UTF-8 fixture, validates its hash manifest and expected FileList entries, and preserves an existing local report.
2. Use the printed fixture root as the GUI root.
3. Use a headful smoke script for native launch. It stages a disposable executable plus ignore/sample files under `rust/target/gui-smoke/runs/`, isolates settings, rejects adjacent updater artifacts, and launches only that staged copy.
4. Store local notes, screenshots, and logs under `rust/target/gui-smoke/evidence/`. Do not store user configuration content or unmasked UNC names.

## Pass / Fail Policy
- Each `GSM-*` row has three independent axes: Deterministic, Native interaction, and Liveness. Each axis records `PASS`, `FAIL`, `SKIPPED`, or `NOT RUN`, plus reason, evidence, and reproduction procedure.
- PASS: every required axis for every required `GSM-*` case is PASS or explicitly SKIPPED with an accepted reason. Overall cannot be PASS when a required native axis is NOT RUN.
- FAIL: any product behavior mismatch, UI freeze, stale dialog, wrong action target, broken selection, or missing evidence for a required case.
- SKIPPED: allowed only for environment-specific cases that cannot apply to the current OS, and the reason must be recorded.
- NOT RUN: allowed only outside release-candidate gates, or for explicitly out-of-scope flows. The report must state why the case was not run and what automated coverage, if any, partially covers it.
- Flake policy: manual GUI smoke may be retried once for clear environment/display instability. A repeated failure is product or test-plan debt and must be tracked before release.

## Test Cases
| ID | Flow | Steps | Expected |
| --- | --- | --- | --- |
| GSM-001 | Startup and indexing | Launch with the fixture root. Wait for indexing to settle. | Result list appears, status is understandable, first row is selected when candidates exist, and query input accepts typing immediately. |
| GSM-002 | Search and highlight | Search `alpha`, `'alpha`, `!old`, `^README`, `end$`, and `alpha|beta`. | Non-matches hide, operators behave consistently with CLI/unit contract, and highlights are visible on matched text. |
| GSM-003 | Preview and selection | Move current row with arrows, page keys, mouse selection, and preview visibility toggle if available. | Preview follows current row without blocking list movement; binary/unreadable placeholder is not shown for text fixture files. |
| GSM-004 | Open/copy action routing | Use TC-050/051 recording/authorization seams by default. Exercise native open/copy/open-folder only with explicit authorization and only against fixture targets. | Deterministic evidence records resolved/display paths and backend call count without an OS action. If authorized natively, paths with spaces remain intact and notices/errors do not freeze the GUI. |
| GSM-005 | Sort modes | Switch `Score`, `Name`, `Modified`, and `Created`; type a query while date sorting is active. | Sorting changes order without losing input responsiveness; returning to `Score` produces a coherent ranked list. |
| GSM-006 | FileList and dialogs | Confirm the fixture is loaded from `FileList.txt`; run Create File List and exercise confirm/cancel paths. | FileList source is visible, dialogs describe the action, cancel leaves state clean, and completion notice is understandable. |
| GSM-007 | Tabs | Create a new tab, switch roots or queries per tab, close a tab, and reorder tabs when supported by the environment. | Each tab keeps its root/query/results; closing/reordering does not swap active tab identity. |
| GSM-008 | Dialog and error handling | Open a fixture-local confirmation/cancel path. Use only an injected/forced update-check failure; do not perform a network update check. | Dialog focus, default action, cancel action, and returned notice are clear and do not leak stale state. |
| GSM-009 | Theme visual pass | Check light and dark theme, especially selected row fill, tab accent, highlight color, and disabled controls. | Contrast and selected/focused states remain readable; no obvious layout clipping in the main panels. |
| GSM-010 | Responsiveness | While indexing or switching roots, type, backspace, move selection, scroll results, and cancel pending dialogs. | UI remains interactive; long work is reflected by status/progress instead of freezing the event loop. |

## Automation Boundary
- `scripts/gui-deterministic-scenarios.tsv` is the canonical group inventory consumed by both wrappers. They use `cargo test --locked --lib`, reject zero/under-count discovery, explicitly skip the ignored `measure_cjk_font_load_headless` measurement without `--ignored`, and require zero ignored executions.
- Automated unit/headless coverage remains in Rust tests for render snapshots, `run_ui_frame`, shortcuts, tabs, dialogs, update commands, action authorization, worker bounds, stale routing, IME events, window geometry, and index pipeline state.
- The headless GUI surface snapshot MUST cover the visible app contract that can be asserted without opening a native window: active root, query text, filter toggles, ignore-list toggle, result sort mode, result count/current row target, pinned selection count, tab count/active tab, preview visibility/width, top actions, status line, and FileList/update dialog labels/buttons.
- When adding GUI controls whose state is visible without native platform interaction, add or update a headless snapshot assertion before relying on manual `GSM-*` smoke coverage.
- When adding GUI controls that require native platform interaction, update the relevant `GSM-*` case and the report template before accepting manual-only coverage.
- Headful automation is a release/nightly smoke gate only. It launches a fresh BaseDir-owned staged copy against the standard fixture, treats early process exit as FAIL, records the staged path/settings isolation/pre- and post-launch allowlist and `.flistwalker-update*` absence in `GUI-HEADFUL-SMOKE.local.md`, and then stops the process after the configured duration.
- The headful smoke does not replace `GSM-*` manual checks because it does not assert typed search, visual highlight quality, platform open behavior, IME, or window movement.
- Pull-request CI does not require native GUI launch unless a deterministic platform harness is explicitly added later.
- CI continues to own `cargo test`, clippy, coverage, audit, and performance gates.

## Deterministic Scenario Map
| GSM | Canonical group(s) | Deterministic claim | Native residual |
| --- | --- | --- | --- |
| GSM-001 | `surface-dialog-theme`, `bounded-index`, `stale-routing` | Startup surface, status, render frame, and latest index response state. | Native focus/typing and visible indexing still need direct observation. |
| GSM-002 | `surface-dialog-theme`, `stale-routing` plus VM-004 | Query/result/highlight surface and stale search rejection. | Visual highlight quality and actual typing remain native. |
| GSM-003 | `surface-dialog-theme`, `stale-routing` | Selection/preview state and render behavior. | Mouse/key feel and preview latency remain native. |
| GSM-004 | `action-guard`, `stale-routing` | Root confinement, recording-executor calls, display/execution paths, stale completion rejection. | External action and clipboard are NOT RUN without explicit authorization; real UNC is separate. |
| GSM-005 | `surface-dialog-theme`, `stale-routing` | Sort controls/result state and stale response behavior. | Perceived responsiveness while typing remains native. |
| GSM-006 | `surface-dialog-theme`, `bounded-index`, `stale-routing` | FileList/dialog command and latest-response state. | Visible source label and fixture-local dialog interaction remain native. |
| GSM-007 | `tab-ownership`, `background-routing`, `stale-routing` | Large payload ownership, tab identity, background response isolation. | Visible drag/reorder and keyboard interaction remain native. |
| GSM-008 | `surface-dialog-theme`, `stale-routing` | Dialog commands, cancel/failure state, stale response rejection. | Native focus/default-button behavior remains native. |
| GSM-009 | `surface-dialog-theme` | Stable light/dark color and surface contracts. | Contrast, clipping, and display rendering remain native. |
| GSM-010 | `bounded-action`, `bounded-kind`, `bounded-index`, `terminal-settlement`, `tab-ownership`, `background-routing`, `stale-routing` | Queue bounds, settlement, stale-before-I/O, ownership, and latest-response invariants. | Perceived interactivity during actual native input remains native. |

The `ime-window-geometry` group is cross-cutting evidence for GSM-001/002/007/010. It proves composition-event ownership and geometry normalization deterministically; Japanese IME, DPI scaling, and multi-display movement remain separate native evidence.

## Risks
- Manual evidence can be skipped under time pressure.
  - Mitigation: release candidates require a generated local report or a report based on `docs/GUI-TESTREPORT.template.md` to be filled with environment, `GSM-*` status, and evidence paths before publish.
- Environment-specific behavior may be under-tested on non-release changes.
  - Mitigation: Windows/macOS are required for release candidates and platform-specific UI/input changes.
