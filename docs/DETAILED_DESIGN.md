# FlistWalker Detailed Design

This file is the entry point for the repository-level detailed design. Detailed sections are split by topic under `docs/detailed-design/`.

## Document Map
| Topic | Content |
| --- | --- |
| [Overview and Scope](detailed-design/overview-scope.md) | Overview, implementation map, terms, audience guide, scope |
| [Architecture Overview](detailed-design/architecture-overview.md) | Application architecture and deployment view |
| [Module Detailed Design](detailed-design/module-design.md) | Module ownership and implementation boundaries |
| [Data Design](detailed-design/data-design.md) | Core entities, state lifecycle, integrity constraints, state transitions |
| [Control Flow and Sequence](detailed-design/control-flow.md) | Startup, indexing, search, FileList, action, and self-update sequences |
| [Resilience, Security, and Operations](detailed-design/resilience-operations.md) | Error handling, resilience, security, operations, diagnostics |
| [Testing, Trade-offs, and Traceability](detailed-design/testing-traceability.md) | Test strategy, trade-offs, extension points, open questions, traceability |

## Reading Guide
- Start with [Overview and Scope](detailed-design/overview-scope.md) for purpose and terminology.
- For code ownership decisions, read [Module Detailed Design](detailed-design/module-design.md) and [Architecture Overview](detailed-design/architecture-overview.md).
- For behavior-preserving refactors, also read [Data Design](detailed-design/data-design.md) and [Control Flow and Sequence](detailed-design/control-flow.md).
- Use [TESTPLAN.md](./TESTPLAN.md) for the validation matrix after selecting changed files.

## Related Docs
- [INDEX.md](./INDEX.md)
- [REQUIREMENTS.md](./REQUIREMENTS.md)
- [SPEC.md](./SPEC.md)
- [DESIGN.md](./DESIGN.md)
- [ARCHITECTURE.md](./ARCHITECTURE.md)
- [TESTPLAN.md](./TESTPLAN.md)
