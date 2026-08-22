# SPEC

This file is the entry point for FlistWalker specifications. Detailed SP content is split by topic under `docs/spec/`. Each topic file keeps its MUST/SHOULD clauses, preconditions, postconditions, edge/error cases, and regression guards.

## Document Map
| Topic | SP IDs |
| --- | --- |
| [Indexing and Performance Specification](spec/indexing-performance.md) | SP-001, SP-002, SP-007, SP-021 |
| [Search, Actions, CLI, Errors, and Testability Specification](spec/search-actions-cli.md) | SP-003, SP-004, SP-005, SP-006, SP-008, SP-009, SP-023 |
| [GUI Behavior Specification](spec/gui-behavior.md) | SP-010, SP-011, SP-013, SP-022 |
| [Operations, Release, and Runtime Configuration Specification](spec/operations-release-config.md) | SP-012, SP-014, SP-015, SP-016, SP-017, SP-018 |

## Update Rule
- Add new SP IDs to the relevant topic file, then update this map and the related requirement, design, and test traceability in the same change.
- When changing the meaning of an existing SP, check the corresponding FR/DES/TC in the same change.

## Traceability (excerpt)
- FR-001 -> SP-001 -> DES-001 -> TC-001
- FR-002 -> SP-002 -> DES-002 -> TC-002
- FR-003 -> SP-003 -> DES-003 -> TC-003, TC-155
- FR-007 -> SP-010 -> DES-009 -> TC-010
- FR-007, NFR-008 -> SP-010 -> DES-006, DES-007, DES-009 -> TC-150, TC-151, TC-152, TC-153
- FR-007, NFR-009 -> SP-010 -> DES-009 -> TC-154
- NFR-001 -> SP-007 -> DES-006 -> TC-007, TC-156, TC-161, TC-185
- FR-009 -> SP-004 -> DES-004, DES-007 -> TC-050, TC-051
- FR-012 -> SP-013 -> DES-013 -> TC-057
- FR-019 -> SP-014 -> DES-014 -> TC-074
- FR-033 -> SP-014 -> DES-014 -> TC-158, TC-159, TC-160, TC-171, TC-186, TC-187
- NFR-010 -> SP-014 -> DES-014 -> TC-157, TC-159, TC-160, TC-171, TC-186, TC-187
- FR-034 -> SP-001 -> DES-001 -> TC-161
- FR-035 -> SP-006, SP-014 -> DES-005, DES-014 -> TC-169
- FR-036 -> SP-010 -> DES-009 -> TC-173
- FR-007, NFR-008 -> SP-010 -> DES-009 -> TC-192
- NFR-002, NFR-007, FR-033, NFR-010 -> SP-014 -> DES-014 -> TC-188, TC-189
- FR-020, FR-023, CON-004 -> SP-014 -> DES-014 -> TC-190
- NFR-005, FR-020, NFR-010 -> SP-014 -> DES-014 -> TC-191
- FR-025 -> SP-015 -> DES-016 -> TC-110, TC-112, TC-117, TC-176
- FR-026 -> SP-016 -> DES-017 -> TC-111, TC-167, TC-168
- FR-006 -> SP-004, SP-006, SP-013 -> DES-004, DES-005, DES-006 -> TC-163, TC-164, TC-172
- FR-010, NFR-012 -> SP-001, SP-006 -> DES-005, DES-007 -> TC-165, TC-166
- FR-011, NFR-012 -> SP-016 -> DES-017 -> TC-167, TC-168
- FR-027 -> SP-017 -> DES-018 -> TC-113
- FR-032 -> SP-018 -> DES-019 -> TC-145
- NFR-005 -> SP-012 -> DES-012 -> TC-056, TC-178
- FR-037 -> SP-019 -> DES-020 -> TC-174
- FR-038 -> SP-020 -> DES-021 -> TC-175
- FR-039 -> SP-021 -> DES-022 -> TC-180
- NFR-013 -> SP-022 -> DES-023 -> TC-181, TC-182, TC-183, TC-184
- FR-040, NFR-014 -> SP-023 -> DES-024 -> TC-193
