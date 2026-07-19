# DESIGN

This file is the entry point for FlistWalker design. Detailed DES content is split by topic under `docs/design/`.

## Document Map
| Topic | Content |
| --- | --- |
| [Architecture Overview](design/architecture-overview.md) | DES-001 through DES-005, DES-009, DES-010, and DES-012 through DES-019 responsibilities and implementation locations |
| [Flows, Data Model, and API Contract](design/flows-data-api.md) | Main flows, data model, Rust API contract |
| [Non-functional Runtime Design](design/nonfunctional-runtime.md) | DES-006, DES-007, DES-008, DES-011, and runtime policies |
| [Operations, Trade-offs, and Traceability](design/operations-traceability.md) | Error handling, migration/rollback, trade-offs, traceability excerpt |

## Architecture Summary
- Indexing separates FileList resolution, walker traversal, hierarchy reading, and FileList writing.
- Search separates query interpretation, match evaluation, ranking, cache/config, and execution while sharing interpretation with GUI/CLI highlighting.
- Actions separate the non-blocking UI precheck, authoritative worker-side resolved-path authorization, and the OS execution leaf; only the final authorized execution path crosses the OS boundary.
- The GUI keeps `egui/eframe` as the adapter and splits ownership across state, render, tabs, pipeline, workers, update, and filelist modules.
- Action, kind, and index execution use fixed worker counts, bounded queues, non-blocking dispatch, stale-before-I/O settlement, directly owned worker handles, and bounded shutdown.
- Runtime config, self update, ignore list, release sample, and supportability trace are tracked as separate DES concerns.

## Traceability (excerpt)
- Full excerpt: [Operations, Trade-offs, and Traceability](design/operations-traceability.md)
- DES-001 -> TC-001 (SP-001)
- DES-003 -> TC-003 (SP-003)
- DES-004, DES-007 -> TC-050, TC-051 (SP-004)
- DES-009 -> TC-010 (SP-010)
- DES-006, DES-007, DES-009 -> TC-150, TC-151, TC-152, TC-153 (SP-010)
- DES-012 -> TC-056 (SP-012)
- DES-014 -> TC-074, TC-075, TC-076, TC-077, TC-078, TC-081, TC-140 (SP-014)
- DES-017 -> TC-111, TC-127 (SP-016)
- DES-019 -> TC-145, TC-146, TC-147, TC-148 (SP-018)
