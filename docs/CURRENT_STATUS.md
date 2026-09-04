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
- Required pull-request checks are `CI Gate` and `CI Policy Guardian`. Trusted CI policy changes use the controlled rollout in [CI Operations](CI_OPERATIONS.md), not an ordinary PR shortcut.
- The enforced Rust line-coverage gate is 75%; 80% remains an improvement target rather than a release prerequisite.
- Deterministic GUI, native interaction, and liveness are independent evidence axes. A PASS on one axis never promotes another axis from `NOT RUN`.

## Known Evidence Gaps

- Native Japanese IME composition, alternate-DPI display movement, real UNC access, and explicitly authorized external Open/Reveal behavior require suitable platform sessions and remain conditional manual evidence.
- Native headful GUI launch is not a normal pull-request gate. GUI-adjacent changes and release candidates follow [GUI-TESTPLAN.md](GUI-TESTPLAN.md).
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
