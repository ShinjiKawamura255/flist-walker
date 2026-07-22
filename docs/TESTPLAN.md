# TESTPLAN

This file is the entry point for FlistWalker test planning. Detailed test strategy, TC tables, runner commands, manual procedures, and traceability are split by topic under `docs/testplan/`.

## Document Map
| Topic | Content |
| --- | --- |
| [Test Strategy and Levels](testplan/strategy-levels.md) | Scope, priority, unit/integration/manual/perf/sec levels |
| [Test Cases](testplan/test-cases.md) | TC ID table and related SP mapping |
| [Validation Matrix and Runner Commands](testplan/validation-matrix.md) | Regression guards, change-type checklist, Validation Matrix, commands |
| [Manual Regression and Traceability](testplan/manual-regression-traceability.md) | Environment, manual self-update, diagnostics trace smoke, structural GUI smoke, later regression guards, traceability excerpt |

## Change-Type Checklist
Use the checklist in [Validation Matrix and Runner Commands](testplan/validation-matrix.md#change-type-checklist) before choosing commands. It maps common change intents such as docs-only edits, search contract changes, GUI orchestration, indexing, runtime config, release/update work, and supportability docs to the required docs, tests, and follow-up checks.

## ID Ordering
ID-bearing tables and normative sections SHOULD be kept in ascending ID order to make later insertions predictable. Suffix IDs such as `TC-003A` SHOULD be placed immediately after their parent ID. History, release notes, regression guards, and narrative examples MAY use chronological, risk, or topic order when that is clearer than numeric order.

## Validation Matrix
- Full matrix: [Validation Matrix and Runner Commands](testplan/validation-matrix.md)
- VM-001 Docs only: affected doc diff review and `rg` ID/reference checks; Rust implementation untouched means `cargo test` is not required.
- VM-002 App/UI orchestration: `cd rust && cargo test` plus focused render/GUI checks when relevant.
- VM-003 Indexing path: `cd rust && cargo test` plus ignored perf tests for FileList / Walker indexing paths.
- VM-004 Search/query contract: `cd rust && cargo test` and focused query GUI checks when relevant.
- VM-005 CLI / build / release / updater: `cd rust && cargo test` plus release/update-specific checks.
- VM-006 CI coverage gate / GUI validation docs: coverage command or script/parser checks as applicable.
- VM-007 Supportability docs/templates: affected doc/template diff review and support wording checks.
- VM-008 Runtime config bootstrap: `cd rust && cargo test` plus first-run/config precedence checks when relevant.

## Docs-only Validation
For documentation-only restructuring, apply VM-001:
- review affected doc diff;
- scan ID/reference consistency with `rg`;
- confirm local Markdown links resolve;
- confirm the top-level SDD/TDD files link to the detail topic files.

## Action Authorization Verification
- TC-050 is the automated contract for the UI `Reject` / `Defer` precheck, worker all-target preauthorization, immediate per-target recheck, fail-closed resolution handling, recording-executor call count, display/execution path separation, and partial completion.
- TC-051 adds Unix symlink and Windows link/junction/path-form coverage plus the environment-dependent real-UNC evidence in [Manual Regression and Traceability](testplan/manual-regression-traceability.md#action-authorization-platform-evidence-tc-050--tc-051).
- An unavailable Windows junction or real-UNC environment is recorded as `not run`; it is not equivalent to passing evidence.

## Bounded Worker Scheduling Verification
- TC-150 through TC-153 fix the action/kind/index capacity arithmetic, accepted-only non-blocking dispatch, `Full`/`Disconnected` settlement, stale-before-I/O cancellation, RAII load accounting, lock boundaries, named worker ownership, 250ms shutdown budget, and structured load/correlation trace.
- Apply VM-002 to action/kind/runtime scheduling and VM-003 to index scheduling; TC-149 remains the independent background-index snapshot regression guard.

## Tab Ownership Transfer Verification
- TC-154 fixes allocation-preserving active/inactive ownership transfer, lifecycle/routing compatibility, persisted-field projection, and native large-tab responsiveness.
- Apply VM-002 with focused tab owner tests; use non-compacting/non-sparse fixtures for pointer/capacity identity and verify result compaction separately.

## Traceability (excerpt)
- Full excerpt: [Manual Regression and Traceability](testplan/manual-regression-traceability.md)
- TC-001 -> SP-001 -> DES-001 -> FR-001
- TC-003, TC-155 -> SP-003 -> DES-003 -> FR-003
- TC-010 -> SP-010 -> DES-009 -> FR-007
- TC-150, TC-151, TC-152, TC-153 -> SP-010 -> DES-006, DES-007, DES-009 -> FR-007, NFR-008
- TC-154 -> SP-010 -> DES-009 -> FR-007, NFR-009
- TC-156 -> SP-007 -> DES-006 -> NFR-001
- TC-157, TC-158, TC-159, TC-160 -> SP-014 -> DES-014 -> FR-033, NFR-010
- TC-161 -> SP-001, SP-007 -> DES-001, DES-006 -> FR-034, NFR-001
- TC-050, TC-051 -> SP-004 -> DES-004, DES-007 -> FR-009
- TC-056 -> SP-012 -> DES-012 -> NFR-005
- TC-074 -> SP-014 -> DES-014 -> FR-019
- TC-111 -> SP-016 -> DES-017 -> FR-026
- TC-145 -> SP-018 -> DES-019 -> FR-032
