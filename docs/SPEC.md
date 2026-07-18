# SPEC

This file is the entry point for FlistWalker specifications. Detailed SP content is split by topic under `docs/spec/`. Each topic file keeps its MUST/SHOULD clauses, preconditions, postconditions, edge/error cases, and regression guards.

## Document Map
| Topic | SP IDs |
| --- | --- |
| [Indexing and Performance Specification](spec/indexing-performance.md) | SP-001, SP-002, SP-007 |
| [Search, Actions, CLI, Errors, and Testability Specification](spec/search-actions-cli.md) | SP-003, SP-004, SP-005, SP-006, SP-008, SP-009 |
| [GUI Behavior Specification](spec/gui-behavior.md) | SP-010, SP-011, SP-013 |
| [Operations, Release, and Runtime Configuration Specification](spec/operations-release-config.md) | SP-012, SP-014, SP-015, SP-016, SP-017, SP-018 |

## Update Rule
- Add new SP IDs to the relevant topic file, then update this map and the related requirement, design, and test traceability in the same change.
- When changing the meaning of an existing SP, check the corresponding FR/DES/TC in the same change.

## Traceability (excerpt)
- FR-001 -> SP-001 -> DES-001 -> TC-001
- FR-002 -> SP-002 -> DES-002 -> TC-002
- FR-003 -> SP-003 -> DES-003 -> TC-003
- FR-007 -> SP-010 -> DES-009 -> TC-010
- FR-009 -> SP-004 -> DES-004, DES-007 -> TC-050, TC-051
- FR-012 -> SP-013 -> DES-013 -> TC-057
- FR-019 -> SP-014 -> DES-014 -> TC-074
- FR-025 -> SP-015 -> DES-016 -> TC-110
- FR-026 -> SP-016 -> DES-017 -> TC-111
- FR-027 -> SP-017 -> DES-018 -> TC-113
- FR-032 -> SP-018 -> DES-019 -> TC-145
- NFR-005 -> SP-012 -> DES-012 -> TC-056
