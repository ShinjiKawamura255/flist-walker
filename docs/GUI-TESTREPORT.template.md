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
- Launch command: `cd rust && cargo run --bin flistwalker -- --root target/gui-smoke/root --limit 1000`
- Overall: NOT RUN

## Results
| ID | Status | Notes | Evidence |
| --- | --- | --- | --- |
| GSM-001 | NOT RUN | Startup/indexing | |
| GSM-002 | NOT RUN | Search/highlight/operators | |
| GSM-003 | NOT RUN | Preview and selection movement | |
| GSM-004 | NOT RUN | Open/copy action routing | |
| GSM-005 | NOT RUN | Sort modes | |
| GSM-006 | NOT RUN | FileList source and Create File List dialog | |
| GSM-007 | NOT RUN | Tabs and per-tab state | |
| GSM-008 | NOT RUN | Dialog cancel/failure handling | |
| GSM-009 | NOT RUN | Light/dark theme visual pass | |
| GSM-010 | NOT RUN | Responsiveness during indexing/search | |

## Defects
- None recorded.

## Follow-ups
- Record SKIPPED only with a concrete environment reason.
- Store screenshots/logs under `rust/target/gui-smoke/evidence/` when a release-candidate run is performed.
