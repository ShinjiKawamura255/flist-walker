# GUI TESTREPORT

## Summary
- Date: 2026-04-26
- Tester: shinji
- Build/version: development build after `eframe 0.34.1` update
- Commit: `e723690`
- Scope: WSLg/Linux manual visual smoke for `GSM-001` through `GSM-010`
- OS/display: WSL2 Linux (`LOSGATOS`, `6.6.87.2-microsoft-standard-WSL2`) with WSLg/Mesa display path
- Fixture command: `scripts/gui-smoke-fixture.sh`
- Fixture root: `rust/target/gui-smoke/root`
- Evidence dir: `rust/target/gui-smoke/evidence/`
- Launch command: `cd rust && cargo run --bin flistwalker -- --root target/gui-smoke/root --limit 1000`
- Overall: PASS

## Results
| ID | Status | Notes | Evidence |
| --- | --- | --- | --- |
| GSM-001 | PASS | Startup/indexing manually confirmed; no blocking visual/runtime issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-002 | PASS | Search/highlight/operators smoke manually confirmed; no issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-003 | PASS | Preview and selection movement smoke manually confirmed; no issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-004 | PASS | Open/copy action routing smoke manually confirmed; no issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-005 | PASS | Sort modes smoke manually confirmed; no issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-006 | PASS | FileList source/dialog smoke manually confirmed; no issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-007 | PASS | Tabs/per-tab state smoke manually confirmed; no issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-008 | PASS | Dialog/cancel/failure handling smoke manually confirmed; no issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-009 | PASS | Light/dark theme visual pass manually confirmed; no issue observed. | User manual confirmation in chat on 2026-04-26 |
| GSM-010 | PASS | Responsiveness during indexing/search manually confirmed; no freeze observed. | User manual confirmation in chat on 2026-04-26 |

## Defects
- None recorded.

## Follow-ups
- Repeat this report for release candidates, especially on Windows 11 and macOS.
- Store screenshots/logs under `rust/target/gui-smoke/evidence/` when a release-candidate run is performed.
- Record SKIPPED only with a concrete environment reason.
