# REQUIREMENTS

This file is the entry point for FlistWalker requirements. Detailed FR/NFR/CON and AC content is split by topic under `docs/requirements/`.

## Document Map
| Topic | Content |
| --- | --- |
| [Product Scope and Acceptance Criteria](requirements/product-scope.md) | Background / KPI, scope, use cases, acceptance criteria |
| [Functional Requirements](requirements/functional.md) | FR-001 through FR-034 |
| [Quality, Constraints, and Risks](requirements/quality-constraints.md) | NFR, CON, risks |
| [Requirements Traceability](requirements/traceability.md) | FR/NFR to SP/DES/TC traceability excerpt |

## Scope Summary
- Rust CLI/GUI implementation.
- Candidate collection through FileList and walker sources.
- fzf-compatible query handling, result actions, multi-select, and batch actions.
- Startup update check through GitHub Releases and Windows/Linux self-update.
- Prototype feature expansion, network-drive optimization, installer creation, and macOS `.app` bundle auto-update are out of scope.

## ID Ownership
- FR entries live in [Functional Requirements](requirements/functional.md).
- NFR/CON entries and risks live in [Quality, Constraints, and Risks](requirements/quality-constraints.md).
- AC entries live in [Product Scope and Acceptance Criteria](requirements/product-scope.md).

## Traceability (excerpt)
- Full excerpt: [Requirements Traceability](requirements/traceability.md)
- FR-001 -> SP-001 -> DES-001 -> TC-001
- FR-003 -> SP-003 -> DES-003 -> TC-003, TC-155
- FR-007 -> SP-010 -> DES-009 -> TC-010
- FR-007, NFR-008 -> SP-010 -> DES-006, DES-007, DES-009 -> TC-150, TC-151, TC-152, TC-153
- FR-007, NFR-009 -> SP-010 -> DES-009 -> TC-154
- FR-019 -> SP-014 -> DES-014, DES-009 -> TC-074, TC-140
- FR-032 -> SP-018 -> DES-019 -> TC-145, TC-146, TC-147, TC-148
- FR-033 -> SP-014 -> DES-014 -> TC-158, TC-159, TC-160
- NFR-010 -> SP-014 -> DES-014 -> TC-157, TC-159, TC-160
- FR-034 -> SP-001 -> DES-001 -> TC-161
- NFR-001 -> SP-007 -> DES-006 -> TC-007, TC-156, TC-161
