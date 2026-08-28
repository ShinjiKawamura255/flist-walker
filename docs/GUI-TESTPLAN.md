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
- Evidence history is append-only. A later residual run adds a dated addendum or a new result record; it does not rewrite the status or reason observed by an earlier run.
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
| GSM-001 | Startup and indexing | Launch with the fixture root, wait for indexing to settle, then clear a non-empty query and repeat an empty→non-empty result transition. | Result list appears, status is understandable, empty Results has no cursor, every non-empty Results state visibly selects a valid row by the next frame, and query input accepts typing immediately. |
| GSM-002 | Search and highlight | Search `alpha`, `'alpha`, `!old`, `^README`, `end$`, and `alpha|beta`. | Non-matches hide, operators behave consistently with CLI/unit contract, and highlights are visible on matched text. |
| GSM-003 | Preview and selection | Move current row with arrows, page keys, mouse selection, query reset, preset apply, history accept/cancel, tab switch/restore, and preview visibility toggle if available. | Preview follows current row without blocking list movement; cursor remains visible for non-empty Results; programmatic query replacement leaves the text caret at the new query end; binary/unreadable placeholder is not shown for text fixture files. |
| GSM-004 | Open/copy action routing | Use TC-050/051 recording/authorization seams by default. Treat external open/reveal and Copy Path as separate native axes. Exercise external open/reveal only against fixture targets in a disposable owned handler/session that cannot update a real default application's MRU or reuse an existing window. Exercise Copy Path only through the clipboard safety gate below. | Deterministic evidence records resolved/display paths and backend call count without an OS action. Native PASS requires the axis-specific safety gate, intact spaces, exact target attribution, and a responsive GUI; one native axis never promotes the other. |
| GSM-005 | Sort modes | Switch `Score`, `Name`, `Modified`, and `Created`; type a query while date sorting is active. | Sorting changes order without losing input responsiveness; returning to `Score` produces a coherent ranked list. |
| GSM-006 | FileList and dialogs | Confirm the fixture is loaded from `FileList.txt`; run Create File List and exercise confirm/cancel paths. | FileList source is visible, dialogs describe the action, cancel leaves state clean, and completion notice is understandable. |
| GSM-007 | Tabs | Set `Depth: ≤ 2` from the second-row control between `Folders` and `Preview`, create a new tab, switch roots/queries/depth per tab, close a tab, and reorder tabs when supported by the environment. With `restore_tabs_enabled`, prepare restored tabs covering Query empty/non-empty and FileList/Walker, then activate each once. Repeat rapid A→B→C→A switching while prior indexing is active. | Each tab keeps its root/query/results/depth; the limited tab continues to show `Depth: ≤ 2`, other existing tabs are unchanged, a new tab starts at `Depth: All`, and closing/reordering does not swap active tab identity. Every lazy-refresh activation shows the selected tab and `Indexing...` by the next rendered frame, keeps input/scroll responsive, and does not wait for an older tab's FileList/index completion before establishing the new request. |
| GSM-008 | Help, keyboard configuration, preset picker/editor, Named Root manager, dialog, and error handling | Open shortcut help from both `Help` and `F1`; verify the OS primary modifier, current Emacs setting, and preset/Named Root guidance, then close it with `F1`, `Esc`, enabled `Ctrl+G`, and `Close` while checking that background search/action state is unchanged. On Windows/Linux, first keep `ctrl_w_deletes_word_in_query=false` and confirm `Ctrl+W` closes a tab; then enable it together with Emacs keybindings and confirm focused normal/history search deletes exactly one previous word without closing the tab, unfocused input still closes the tab, and IME composition does neither. On macOS, confirm opted-in `Ctrl+W` edits focused search text while `Cmd+W` remains tab close. Open the GUI picker with `Ctrl+Shift+P` or `Cmd+Shift+P`, use `Add` to verify the current root/query/type/source/regex/case/ignore/sort initialize a new draft, save it, fuzzy-filter by name, move selection with both `Up` / `Down` and enabled `Ctrl+P` / `Ctrl+N`, apply with `Enter`, `Ctrl+J`, and `Ctrl+M`, and cancel with `Esc` and `Ctrl+G`. In the normal query, history filter, preset filter, every preset editor text field, Named Root name/path, and saved-root add/edit fields, verify `Ctrl+A/E/B/F/H/D/K/Y/U` use the same cursor/delete/kill/yank behavior, including Unicode text and single-delete `Ctrl+H`. Disable Emacs keybindings once and confirm those Ctrl chords do not trigger application-owned navigation, apply, cancel, or text editing. Open the selected preset with `F2` and `Edit`, edit every pure-search field, verify root remains directly editable and can also be replaced through `Browse...`, cancel the folder picker once to confirm the typed value remains, then rename and save with the primary modifier plus Enter and `Save`. Delete the selected preset with the `Delete` button, cancel once with Esc, then confirm and verify the current tab is unchanged. From both the picker heading's `Manage named roots...` and the editor's `Manage...`, add an absolute root, verify path can be entered directly, selected through `Browse...`, or copied from `Use current root`, cancel the folder picker once without losing the typed value, rename/repath it, verify linked preset selection follows the rename, exercise `Ctrl+P` / `Ctrl+N` selection and `Ctrl+G` cancel, then exercise delete confirmation and verify the preset falls back to its snapshot. Verify nested Esc behavior, folder-picker failure, collision/relative-path/write errors retain the draft or confirmation, catalog mutations do not apply to the current tab, and the main panel has no preset control. Also open a fixture-local confirmation/cancel path. Use only an injected/forced update-check failure; do not perform a network update check. | Help, keyboard configuration, picker/editor, and manager content match the current platform/configuration; shared Emacs navigation/accept/cancel and text editing follow the active surface and disabled settings, contextual `Ctrl+W` owns at most one action, and modal focus and close paths do not leak input to the background. Loading/error/empty/no-match/saving/deleting states are understandable, pure-search apply uses the selected preset, atomic preset add/replace/delete preserve current-tab state, catalog mutation preserves unknown fields and reference/fallback contracts, and confirmation/error dialog focus, default action, cancel action, and returned notice are clear without stale state. |
| GSM-009 | Theme visual pass | Check light and dark theme, especially selected row fill, tab accent, highlight color, and disabled controls. | Contrast and selected/focused states remain readable; no obvious layout clipping in the main panels. |
| GSM-010 | Responsiveness | While indexing or switching roots, type, backspace, move selection, scroll results, and cancel pending dialogs. Repeat tab switching with a large FileList tab, a Walker tab, empty/non-empty queries, and at least one stale in-flight request; record the first-frame status and any visible pause separately from total indexing time. | UI remains interactive; long work is reflected by status/progress instead of freezing the event loop. Tab selection/status changes appear by the next rendered frame, no multi-second unexplained gap occurs before `Indexing...`, and total index completion time is not misreported as transition latency. |
| GSM-011 | Maximum depth and presets | From `Depth: All`, apply depth `2`, reopen the popup, select `Unlimited`, and Apply. Confirm Cancel preserves the prior value and that the `Unlimited` checkbox and label are vertically centered. Then save/edit/apply presets with unlimited and limited depth, including applying an unlimited preset after a limited one. Switch tabs before and after applying each preset. | Walker and FileList show only candidates through the selected depth; `Unlimited` remains selected until Apply and restores `Depth: All`; the button always displays the active tab value; preset summaries/editor show depth; popup and editor checkbox rows are vertically centered; applying affects only the active tab and remains until explicitly changed; legacy presets without depth behave as `All`. |

## Automation Boundary
- `scripts/gui-deterministic-scenarios.tsv` is the canonical group inventory consumed by both wrappers. They use `cargo test --locked --lib`, reject zero/under-count discovery, explicitly skip the ignored `measure_cjk_font_load_headless` and `perf_tc_154_tab_transition_coordinator_p95_stays_below_hard_ceiling` measurements without `--ignored`, and require zero unexpected ignored executions.
- Automated unit/headless coverage remains in Rust tests for render snapshots, `run_ui_frame`, shortcuts, tabs, dialogs, update commands, action authorization, worker bounds, stale routing, IME events, window geometry, and index pipeline state.
- The headless GUI surface snapshot MUST cover the visible app contract that can be asserted without opening a native window: active root, query text, filter toggles, maximum-depth label/state, ignore-list toggle, result sort mode, result count/current row target, pinned selection count, tab count/active tab, preview visibility/width, top actions, status line, and FileList/update dialog labels/buttons.
- When adding GUI controls whose state is visible without native platform interaction, add or update a headless snapshot assertion before relying on manual `GSM-*` smoke coverage.
- When adding GUI controls that require native platform interaction, update the relevant `GSM-*` case and the report template before accepting manual-only coverage.
- Headful automation is a release/nightly smoke gate only. It launches a fresh BaseDir-owned staged copy against the standard fixture, treats early process exit as FAIL, records the staged path/settings isolation/pre- and post-launch allowlist and `.flistwalker-update*` absence in `GUI-HEADFUL-SMOKE.local.md`, and then stops the process after the configured duration.
- The headful smoke does not replace `GSM-*` manual checks because it does not assert typed search, visual highlight quality, platform open behavior, IME, or window movement.
- Pull-request CI does not require native GUI launch unless a deterministic platform harness is explicitly added later.
- CI continues to own `cargo test`, clippy, coverage, audit, and performance gates.

## Native Residual Safety Gates
- Axis statuses remain `PASS`, `FAIL`, `SKIPPED`, or `NOT RUN`. Environment or authorization prerequisites are recorded as `NOT RUN — <exact prerequisite>`; `blocked` is workflow state, not a GUI axis status.
- Japanese literal text and Japanese IME composition are separate evidence. Literal injection proves committed Unicode rendering only. Composition PASS additionally requires composition events plus identifier-only record/restore/read-back of a staged-window-scoped input profile and IME mode; otherwise composition remains NOT RUN.
- Multi-display PASS requires before/cross-display/return window coordinates and a responsive interaction after crossing. Alternate-DPI PASS additionally requires at least two observed display scale factors; two displays at one scale leave alternate DPI NOT RUN.
- Copy Path must not read pre-existing clipboard content. First check format count and sequence number only. If nonempty, leave it untouched and record NOT RUN. If empty, compare only the test-generated known fixture value in memory, emit no content, stop without clearing on an unexpected sequence change, otherwise clear and read back zero formats.
- External open/reveal requires a disposable owned handler/session. Without one, do not invoke the real default application, change associations, close a reused window, or promote deterministic routing to native PASS.
- Real UNC requires an existing, authorized, reachable fixture share. Do not create a share, enumerate unrelated network paths, or disclose an unmasked server/share name merely to remove NOT RUN.
- Updater/network native evidence uses only an isolated staged target and literal `127.0.0.1` or `::1` listener/feed/asset/redirect URLs. Record production-env absence, canonical staged path, pre/post target hash, updater-artifact count, timeout, process/server cleanup, and rollback. If signing material is absent, a 404/no-redirect failure-path PASS may coexist with signed apply/restart NOT RUN.
- User-settings persistence is exercised only under isolated `LOCALAPPDATA`/`APPDATA`/`USERPROFILE`. Record the isolated path class, a harmless state change, staged restart read-back, and zero use of the real profile.

## Deterministic Scenario Map
| GSM | Canonical group(s) | Deterministic claim | Native residual |
| --- | --- | --- | --- |
| GSM-001 | `surface-dialog-theme`, `bounded-index`, `stale-routing` | Startup surface, status, render frame, and latest index response state. | Native focus/typing and visible indexing still need direct observation. |
| GSM-002 | `surface-dialog-theme`, `stale-routing` plus VM-004 | Query/result/highlight surface and stale search rejection. | Visual highlight quality and actual typing remain native. |
| GSM-003 | `surface-dialog-theme`, `stale-routing` | Selection/preview state and render behavior. | Mouse/key feel and preview latency remain native. |
| GSM-004 | `action-guard`, `stale-routing` | Root confinement, recording-executor calls, display/execution paths, stale completion rejection. | External open/reveal remains NOT RUN without a disposable owned handler/session; Copy Path remains NOT RUN unless its clipboard safety gate passes. Real UNC is separate. |
| GSM-005 | `surface-dialog-theme`, `stale-routing` | Sort controls/result state and stale response behavior. | Perceived responsiveness while typing remains native. |
| GSM-006 | `surface-dialog-theme`, `bounded-index`, `stale-routing` | FileList/dialog command and latest-response state. | Visible source label and fixture-local dialog interaction remain native. |
| GSM-007 | `tab-ownership`, `background-routing`, `stale-routing` | Large payload ownership, tab identity, background response isolation. | Visible drag/reorder and keyboard interaction remain native. |
| GSM-008 | `surface-dialog-theme`, `shortcut-help`, `preset-picker`, `stale-routing` | Help/picker/editor/Named Root manager snapshots and content, nested modal command/input isolation, pure-search-only apply, atomic add/replace/delete requests, rename reference-follow/delete fallback/collision/unknown-field handling, cancel/failure state, stale response rejection. | Native picker/editor/manager typing and focus, list/combo layout, delete warning, error readability, and default-button behavior remain native. |
| GSM-009 | `surface-dialog-theme` | Stable light/dark color and surface contracts. | Contrast, clipping, and display rendering remain native. |
| GSM-010 | `bounded-action`, `bounded-kind`, `bounded-index`, `terminal-settlement`, `tab-ownership`, `background-routing`, `stale-routing` | Queue bounds, settlement, stale-before-I/O, ownership, and latest-response invariants. | Perceived interactivity during actual native input remains native. |
| GSM-011 | `surface-dialog-theme`, `preset-picker`, `tab-ownership`, `bounded-index` | Visible depth state, limited-to-unlimited state transition, preset compatibility, tab-local ownership, and depth-aware worker requests. | Popup/editor checkbox alignment, Apply/Cancel interaction feel, and clipping at narrow native widths remain native. |

The `ime-window-geometry` group is cross-cutting evidence for GSM-001/002/007/010. It proves composition-event ownership and geometry normalization deterministically; Japanese IME, DPI scaling, and multi-display movement remain separate native evidence.

## Risks
- Manual evidence can be skipped under time pressure.
  - Mitigation: release candidates require a generated local report or a report based on `docs/GUI-TESTREPORT.template.md` to be filled with environment, `GSM-*` status, and evidence paths before publish.
- Environment-specific behavior may be under-tested on non-release changes.
  - Mitigation: Windows/macOS are required for release candidates and platform-specific UI/input changes.
