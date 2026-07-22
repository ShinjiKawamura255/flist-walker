<a id="top"></a>

# Testing, Trade-offs, and Traceability

## 11. Test Strategy

Testing follows [TESTPLAN.md](../TESTPLAN.md). For this design, the important architectural boundaries are:

- Unit tests for FileList detection/parsing/writing, walker classification, query/search ranking, actions, update candidate resolution, and UI model helpers.
- App tests grouped by owner seam: update commands, session restore, session tabs, index pipeline, shortcuts, window/IME, render tests.
- Integration tests under [rust/tests](../../rust/tests), especially CLI contract checks.
- Performance tests for FileList line-only fast path and walker classification.
- Docs-only changes use VM-001: affected doc diff review and `rg` reference consistency checks; Rust tests are not required unless Rust files change.

Recommended validation for changes that use this document:

| Matrix | Change area | Minimum validation | Escalation / follow-up |
| --- | --- | --- | --- |
| VM-001 | Docs only | affected doc diff review and `rg` reference checks | Rust tests are unnecessary unless Rust files change. |
| VM-002 | App/UI orchestration | `cd rust && cargo test` | GUI smoke for dialog/focus/tab/render/input changes. |
| VM-003 | Index/FileList/walker | `cd rust && cargo test` plus the three ignored perf tests from AGENTS.md / TESTPLAN when indexing paths change | Large-root GUI smoke and trace smoke if observable worker trace changes. |
| VM-004 | Search/query/highlight/sort contract | `cd rust && cargo test` | Manual query checks for `'`, `!`, `^`, `$`, and `|` when user-visible behavior changes. |
| VM-005 | CLI/build/release/updater | `cd rust && cargo test` | Release docs review, platform asset review, manual update tests as needed. |
| VM-006 | CI coverage gate / GUI validation docs | `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75` | Re-measure and update baseline when raising threshold. |
| VM-007 | Supportability docs/templates | affected doc/template diff review and redaction/telemetry wording check | No Rust tests unless support code changes. |

Manual-heavy checks remain documented in [TESTPLAN.md](../TESTPLAN.md). Structural GUI checks use the isolated staged launch and three-axis evidence contract in [GUI-TESTPLAN.md](../GUI-TESTPLAN.md); diagnostics use deterministic owner tests unless a dedicated staged trace launch exists; VM-005 self-update checks use only a private sandbox and local inert feed. Release/security changes should also consider `cargo audit`, release sidecar completeness, and notarization notes.

[[↑ Back to Top]](#top)

## 12. Trade-offs and Extension Points

### Trade-offs

- FileList indexing favors throughput over early perfect kind classification. This keeps startup responsive but requires deferred kind resolution and unknown-kind UI handling.
- FileList line-only parsing favors Windows/WSL practical compatibility for `\` separated lists over strict POSIX literal-backslash disambiguation during the initial stream. Later kind resolution can refine metadata without slowing the fast path.
- Root containment is action-time rather than index-time. This preserves FileList compatibility and speed while placing security enforcement at the execution boundary.
- GUI state is split into many owner modules. This increases file count but reduces accidental cross-feature mutation in a large egui app.
- Background tabs compact display-oriented caches to reduce memory pressure, trading some activation work for better long-session behavior.
- Search uses a hybrid fuzzy/literal/regex model. This preserves fzf-like behavior for plain tokens while still allowing regex syntax when requested.
- Self-update is platform-specific and conservative. Windows/Linux can auto-apply after verification; macOS is manual-only to avoid unsupported replacement/notarization assumptions.
- CI coverage/audit gates add runtime cost to CI, but they protect release-target OS behavior, dependency hygiene, and architectural test coverage from silent drift.

### Extension Points

- New search operators should start in [query.rs](../../rust/src/query.rs), then update search ranking and UI highlight together.
- New candidate metadata should be added as a deferred worker/cache path unless it is proven safe for the index fast path.
- New GUI features should define state ownership first: runtime active state, tab snapshot state, feature bundle state, or cache state.
- New worker flows should add request/response types in [app/worker_protocol.rs](../../rust/src/app/worker_protocol.rs), channel wiring in [app/worker_bus.rs](../../rust/src/app/worker_bus.rs), and ownership tests under [app/tests](../../rust/src/app/tests).
- New public environment variables require README/support/release documentation review; dev/test overrides should stay out of public docs.

[[↑ Back to Top]](#top)

## 13. Open Questions

No unresolved in-scope open questions remain for this detailed design document.

Out-of-scope follow-up candidates:

- Whether to add generated diagrams to release documentation.
- Whether macOS notarization should become a release publish gate after signing infrastructure is ready.

[[↑ Back to Top]](#top)

## 14. Traceability Summary

This document is descriptive, not a new normative specification. It maps to the existing SDD chain as follows:

| Requirement area | Existing SDD IDs | Detailed design sections | Test plan |
| --- | --- | --- | --- |
| FileList priority and creation | SP-001, DES-001, DES-007 | Sections 6.3, 6.4, 6.11, 7.4, 8.2, 8.3, 9 | TC-001, TC-019, TC-030, TC-047, TC-052, TC-084, TC-088, TC-102 |
| Walker indexing | SP-002, DES-002, DES-006 | Sections 6.3, 6.4, 8.2 | TC-002, TC-083 |
| Search and highlight | SP-003, DES-003 | Sections 6.5, 8.1 | TC-003, TC-071, TC-072, TC-092, TC-093 |
| Action execution | SP-004, SP-005, DES-004 | Sections 6.12, 8.4, 10 | TC-004, TC-004A, TC-050, TC-118 |
| CLI contract | SP-006, DES-005 | Sections 6.1, 8.1 | TC-006, TC-006A |
| GUI operation and responsiveness | SP-010, SP-013, DES-009, DES-013 | Sections 6.6 through 6.10, 7.4, 8.1, 9 | TC-010, TC-057 through TC-064, TC-068 through TC-070, TC-104 |
| GUI regression plan and Window/IME stability | SP-011, DES-010, DES-011 | Sections 6.8, 6.9, 10, 11 | TC-011, TC-020, TC-099 |
| CI / Release security hygiene | SP-012, DES-012 | Sections 10, 11 | TC-056, TC-090, TC-108 |
| PowerShell Windows GNU build | SP-018, DES-019 | Sections 10, 11 | TC-145, TC-146, TC-147, TC-148 |
| Self-update | SP-014, DES-014 | Sections 6.13, 8.5, 10 | TC-074 through TC-081, TC-086, TC-096 through TC-098, TC-100, TC-117, TC-119 |
| Diagnostics and supportability | SP-010, SP-014, DES-015 | Sections 6.9, 10, 11 | TC-109, TC-120 |
| Non-functional performance/reliability/testability | SP-007, SP-008, SP-009, DES-006, DES-007, DES-008 | Sections 9, 10, 11, 12 | VM-002 through VM-006 and related perf/security cases |

[[↑ Back to Top]](#top)
