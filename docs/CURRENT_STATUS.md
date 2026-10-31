# Current Status

This document is the durable current-state snapshot for maintainers. It does not claim validation for the current HEAD, own an active task queue, or retain point-in-time test logs.

## Product Direction

- The Rust GUI, batch CLI, and interactive TUI under `rust/` are the only maintained product implementation.
- The retired Python prototype is available only through Git history and must not be restored to the current tree.
- Windows remains the primary distribution target while Windows, macOS, and Linux behavior stays supported.
- GUI responsiveness is the primary implementation constraint: indexing, search, preview, kind resolution, action execution, update work, and FileList creation stay off the UI thread.

## Quality Posture

- Asynchronous responses are correlated by request identity; stale work must not roll UI or tab state backward.
- Bounded worker queues, explicit cancellation/terminal settlement, and off-thread heavy-payload reclamation are maintained as regression-sensitive contracts.
- Search targets remain below 100 ms for 100,000 candidates; the exact performance commands and larger observational profiles are selected through the [Validation Matrix](testplan/validation-matrix.md).
- Weekly indexing observations cover search overlap, FileList/Walker ingestion, filters and Active-search/Warm-index contention. The default comparator enforces68 median decisions across17 cells, with68 MAX diagnostics and mandatory source/work/request/overlap/cleanup admission. Full tracked harness inventory and pre-build work preflight reject contract drift while allowing recorded product-source differences. The finite measured completion trace follows retained-tab activation and preserves all final oracles and deadlines. The [native-admitted preserved reference](history/indexing-v10-accepted-2026-10-27/native-reference.md) and [accepted prospective source-bound five](history/indexing-v10-accepted-2026-10-27/measurement-diagnosis.md) support the v10 contract; prior [v9 evidence](history/warm-reclaim-v9-2026-10-18/measurement-diagnosis.md) and rejected runs retain their original identities. New application-source native checks, actual product comparison, protected merge and master execution remain separate evidence. This measures the in-process algorithm/coordination path, with process startup/IPC and whole-product throughput outside its scope. [CI Operations](CI_OPERATIONS.md) owns collection and failure handling. This adoption changes no current product Rust, Cargo input, workflow or numeric threshold.
- Required pull-request checks are `CI Gate` and `CI Policy Guardian`. Trusted CI policy changes use the controlled rollout in [CI Operations](CI_OPERATIONS.md), not an ordinary PR shortcut.
- The enforced Rust line-coverage gate is 75%; 80% remains an improvement target rather than a release prerequisite.
- Deterministic GUI, native interaction, and liveness are independent evidence axes. A PASS on one axis never promotes another axis from `NOT RUN`.

## Known Evidence Gaps

- UI regression prevention from [PR #234](https://github.com/ShinjiKawamura255/flist-walker/pull/234) is delivered. Settings-persistence CI timeouts remain a separately tracked unresolved issue with cause `UNKNOWN`; focused local non-reproduction is not fix confirmation. Apply the [local/CI evidence rules](AI_DEVELOPMENT.md#local-validation-and-ci-disposition) without changing the original CI result or required checks.

- TC-207 keeps its strict 100ms wall gate. Additional tracing of the [native stress failure with 48 test threads](history/tc207-timing-2026-10-24/natural-reproduction/README.md) is on hold as a documented stress-condition limitation; the historical file-filter/Walker causes remain unknown. A fresh normal-condition failure or matching native GUI stall reopens investigation. Retained failures and release gates remain unchanged.

- Paged preview has an application-owned control mode via Primary+Shift+L, with normal-render-path regression tests. Native OS key delivery and IME remain separate manual evidence axes.
- Native Japanese IME composition, alternate-DPI display movement, real UNC access, and explicitly authorized external Open/Reveal behavior require suitable platform sessions and remain conditional manual evidence.
- Releases reconcile change-time checks under [Release Validation Selection](testplan/release-validation.md); unaffected functional/native evidence can be reused. Full native evaluation is not required for every release. Changed GUI flows follow [GUI-TESTPLAN.md](GUI-TESTPLAN.md); known native failures still require fix confirmation or explicit acceptance.
- macOS release artifacts may remain unnotarized under the temporary release posture; the public release note must disclose that condition.

## Maintenance Priorities

1. Preserve responsive UI and request-exact asynchronous ownership.
2. Keep FileList, walker, query, tab-resource, and stateful-endurance regression gates deterministic.
3. Improve low-covered native GUI owner seams without weakening the coverage threshold.
4. Keep updater, action authorization, release assets, and trusted CI policy fail closed.
5. Record durable validation evidence when a release, incident, or explicit maintenance baseline requires it.

## Live State Sources

Do not copy volatile branch, commit, PR, release, or test-count values into this file. Resolve them from their owning source:

| Need | Source of truth |
| --- | --- |
| Current checkout and branch | `git status --short --branch`, `git rev-parse HEAD` |
| Current package version | `rust/Cargo.toml` and the root package entry in `rust/Cargo.lock` |
| Published versions | Git tags and GitHub Releases |
| Active work scope | The invoking issue, PR, user request, or agent task; see [TASKS.md](TASKS.md) |
| Required checks for a change | `python3 scripts/validate_change.py --base <base> --plan`, then the selected VM detail |
| Current CI result | GitHub Actions run for the exact head SHA |
| Release evidence | [releases/INDEX.md](releases/INDEX.md) and versioned evidence stored there |
| Completed maintenance context | [history/INDEX.md](history/INDEX.md) |

## Evidence Durability

- `rust/target/`, `dist/`, temporary directories, local `.local.*` reports, and terminal scrollback are transient evidence. They may support the active task but are not durable documentation targets.
- Durable evidence must use a committed sanitized record, an immutable tag/release record, or an exact external run/PR URL with enough identity to retrieve it.
- Rebase workflows rewrite commit SHAs. Prefer PR number, tag, exact GitHub run, or post-merge commit; verify any recorded SHA still resolves before treating it as current evidence.
- Point-in-time validation belongs in a PR work packet, `docs/releases/evidence/`, or a deliberately retained history record. Keep this file limited to current direction, posture, gaps, and routing.

## Continue From Here

| Need | Document |
| --- | --- |
| AI worktree, evidence, authority, and handoff policy | [AI_DEVELOPMENT.md](AI_DEVELOPMENT.md) |
| Choose documents or checks for a change | [INDEX.md](INDEX.md) |
| Locate source directories and entrypoints | [STRUCTURE.md](STRUCTURE.md) |
| Understand runtime ownership and invariants | [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md), then [ARCHITECTURE.md](ARCHITECTURE.md) |
| Select validation commands | [TESTPLAN.md](TESTPLAN.md) and the [Validation Matrix](testplan/validation-matrix.md) |
| Prepare or inspect a release | [RELEASE.md](RELEASE.md) and [releases/INDEX.md](releases/INDEX.md) |
