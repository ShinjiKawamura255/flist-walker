<a id="top"></a>

# Resilience, Security, and Operations

## 9. Error Handling and Resilience

### Error Classes

| Class | Handling |
| --- | --- |
| Invalid root | `resolve_root` returns an error before CLI/GUI flow continues. |
| FileList read/parse failure | Synchronous CLI returns error; GUI worker returns `Failed`. |
| Superseded index/search | Newer request ID cancels or causes stale response discard. |
| Invalid regex | Search worker returns error text; GUI keeps normal operation. |
| Preview decode/unreadable file | Preview text reports unavailable/unreadable state rather than blocking UI. |
| Action failure | Worker returns a notice/error string; GUI displays it. |
| Update check/download/verification failure | Update manager records failure dialog/notice; search UI remains usable. |
| Worker shutdown delay | Runtime uses shutdown flag and bounded join timeout. |
| Root change during pending FileList flow | Pending confirmations, deferred-after-index state, preview, pinned paths, and current row are cleared for the old root. |
| Stale sort/update response | Request ID and mode/state checks prevent old metadata or update results from overwriting current UI state. |
| Ancestor FileList propagation failure | Ancestor update is silently stopped; root FileList creation remains successful when the root write completed. |

### Resilience Patterns

- Latest request wins: search and preview workers drain queued requests and process only the newest.
- Stale response rejection: active request IDs and tab routing maps gate response application.
- Incremental indexing: FileList and walker produce batches rather than one large final vector.
- Deferred metadata: unknown file kind and date metadata are resolved outside initial index/render hot paths.
- Developer-only adaptive walker evaluation: experimental backend and metrics settings are read from the manual `developer` runtime config section, are not written into auto-generated seed config, and are not public user help.
- Bounded caches: preview/highlight/sort metadata caches avoid unbounded long-session growth.
- Graceful degradation: self-update becomes manual-only when platform/key support is insufficient.
- Root cleanup: root switches explicitly discard old-root selection and confirmation state before new indexing begins.
- Trace separation: worker-side structured tracing and GUI window trace remain separate so diagnostics cannot alter request routing behavior.

[[↑ Back to Top]](#top)

## 10. Security and Operations

### Security Design

- External commands are invoked through argument arrays or platform APIs, not shell string expansion.
- Windows `.ps1` search results are opened rather than directly executed.
- FileList may display root-external entries, but GUI action execution validates current-root containment before launching.
- Self-update verifies `SHA256SUMS.sig` before trusting `SHA256SUMS`, then verifies staged file checksums.
- Update override environment variables are development/manual-test only and must not be documented in public user help.
- Query history is stored locally in plain text and can be disabled with runtime config `history_persist_disabled=true`.

### Operations

| Operation | Design |
| --- | --- |
| Build | `cargo` from [rust](../../rust), with Windows GNU helper scripts under [scripts](../../scripts). |
| GUI run | `cargo run --bin flistwalker -- --root .. --limit 1000`. |
| CLI run | `cargo run -- --cli "query" --root .. --limit 1000`. |
| Release | Release assets and sidecar notices are managed by scripts and GitHub Actions described in [RELEASE.md](../RELEASE.md). |
| Diagnostics | Worker tracing uses `RUST_LOG`; GUI/window trace uses `FLISTWALKER_WINDOW_TRACE=1` and optional path override. |
| Support | [SUPPORT.md](../SUPPORT.md) defines redaction and issue reporting expectations. |
| CI security | Cross-platform CI keeps release OS targets under test and runs dependency vulnerability checks such as `cargo audit`. |
| Coverage | The CI coverage gate uses `cargo llvm-cov` with the line threshold documented in [TESTPLAN.md](../TESTPLAN.md). |
| Windows GNU release | WSL/Linux build scripts keep `x86_64-pc-windows-gnu` builds and Windows resource/icon embedding reproducible. |
| Release sidecars | Standalone assets and archives must carry README, LICENSE, and THIRD_PARTY_NOTICES sidecars. |
| macOS notarization | Notarization is currently a documented manual/non-blocking release consideration until signing infrastructure is ready. |

### Diagnostics and Supportability

Worker-side tracing is opt-in through `RUST_LOG` and uses canonical fields for request flows:

- `flow`: logical worker family such as `search`, `preview`, `filelist`, `action`, `sort`, `update`, or `index`.
- `event`: event family such as `started`, `finished`, `failed`, `canceled`, `receiver_closed`, `completed`, or `superseded`.
- `request_id`: present for request-scoped flows.
- `source_kind`: used by index flow to distinguish `filelist`, `walker`, and `none`.

GUI/session/input/update diagnostics use `FLISTWALKER_WINDOW_TRACE=1` and optional `FLISTWALKER_WINDOW_TRACE_PATH`. This channel is for window geometry, IME composition, query text changes, startup/update dialog state, and similar UI diagnostics. It is intentionally separate from worker `tracing` so support instrumentation does not change hot-path request acceptance.

Support documents and issue templates should ask for version, OS, launch mode, reproduction steps, and redacted diagnostics, while avoiding claims of default telemetry or automatic crash upload.

### Failure Triage

1. For search/index issues, inspect source kind, request ID, root, include flags, and whether the response was stale.
2. For GUI state mix-ups, inspect active tab ID, background routing maps, and `AppTabState` snapshot boundaries.
3. For FileList creation, inspect request ID, requested root, pending confirmation state, and cancel flag.
4. For update issues, inspect candidate asset names, support classification, signature/checksum result, and platform path.
5. For Window/IME issues, inspect saved/restored geometry, monitor clamp data, DPI setup, `CompositionEnd` fallback, and window trace events.
6. For release issues, inspect asset names, sidecar completeness, checksum/signature files, release template notes, and notarization status.

[[↑ Back to Top]](#top)
