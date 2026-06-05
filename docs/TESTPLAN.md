# TESTPLAN

This file is the entry point for FlistWalker test planning. Detailed test strategy, TC tables, runner commands, manual procedures, and traceability are split by topic under `docs/testplan/`.

## Document Map
| Topic | Content |
| --- | --- |
| [Test Strategy and Levels](testplan/strategy-levels.md) | Scope, priority, unit/integration/manual/perf/sec levels |
| [Test Cases](testplan/test-cases.md) | TC ID table and related SP mapping |
| [Validation Matrix and Runner Commands](testplan/validation-matrix.md) | Regression guards before the runner section, Validation Matrix, commands |
| [Manual Regression and Traceability](testplan/manual-regression-traceability.md) | Environment, manual self-update, diagnostics trace smoke, structural GUI smoke, later regression guards, traceability excerpt |

## Validation Matrix
- Full matrix: [Validation Matrix and Runner Commands](testplan/validation-matrix.md)
- VM-001 Docs only: affected doc diff review and `rg` ID/reference checks; Rust implementation untouched means `cargo test` is not required.
- VM-002 App/UI orchestration: `cd rust && cargo test` plus focused render/GUI checks when relevant.
- VM-003 Indexing path: `cd rust && cargo test` plus ignored perf tests for FileList / Walker indexing paths.
- VM-004 Search/query contract: `cd rust && cargo test` and focused query GUI checks when relevant.
- VM-005 CLI / build / release / updater: `cd rust && cargo test` plus release/update-specific checks.

## Docs-only Validation
For documentation-only restructuring, apply VM-001:
- review affected doc diff;
- scan ID/reference consistency with `rg`;
- confirm local Markdown links resolve;
- confirm the top-level SDD/TDD files link to the detail topic files.

## Traceability (excerpt)
- Full excerpt: [Manual Regression and Traceability](testplan/manual-regression-traceability.md)
- TC-001 -> SP-001 -> DES-001 -> FR-001
- TC-003 -> SP-003 -> DES-003 -> FR-003
- TC-010 -> SP-010 -> DES-009 -> FR-007
- TC-056 -> SP-012 -> DES-012 -> NFR-005
- TC-074 -> SP-014 -> DES-014 -> FR-019
- TC-111 -> SP-016 -> DES-017 -> FR-026
