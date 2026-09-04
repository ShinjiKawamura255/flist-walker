# AGENTS.md for FlistWalker

This file is the concise project policy entrypoint. The current user request has highest priority; this file overrides broader workspace guidance when they conflict.

## 1. Purpose And Scope

- Build a Rust GUI/CLI/TUI that provides fast fuzzy file/folder search and safe open/execute actions comparable to `fzf --walker`.
- Maintain `FileList.txt` / `filelist.txt` root-only priority loading, separate File/Folder walkers, fzf-compatible query operators, highlighting, filtering, multi-select, and batch actions.
- Support Windows, macOS, and Linux. Keep the WSL/Linux and PowerShell Windows GNU build paths.
- Maintain SDD/TDD under `docs/`. Do not restore the retired Python prototype, optimize for network drives, or add an installer without an explicit scope change.

## 2. Critical Runtime Invariants

- Never block the UI thread with indexing, search, preview, kind resolution, action execution, updater work, FileList creation, heavy payload destruction, or other unbounded I/O/computation.
- Long operations use bounded workers and visible status. Input, cursor movement, and scrolling remain responsive while they run.
- Correlate asynchronous work by request identity. Ignore stale responses and never let them roll UI, tab, or resource state backward.
- Preserve query contracts for `'`, `!`, `^`, `$`, regex/plain matching, ranking, highlighting, and GUI/CLI/TUI consistency.
- Invoke external commands with argument arrays rather than shell expansion. Revalidate action targets immediately before OS interaction and fail closed on unresolved paths.

## 3. Documentation And Validation

- Start at `docs/INDEX.md`. Use `docs/CURRENT_STATUS.md` for durable posture and `docs/AI_DEVELOPMENT.md` for worktree, evidence, authority, PR handoff, and independent-review policy.
- REQUIREMENTS owns FR/NFR/CON and acceptance criteria; SPEC owns SP; DESIGN owns DES; TESTPLAN owns TC and validation selection. Preserve `FR/NFR/CON → SP → DES → TC` traceability.
- Use TDD for behavior changes: focused failing test, implementation, green, refactor, then required regressions. Document why TDD is inapplicable for docs-only or mechanical changes.
- Before completion, run `python3 scripts/validate_change.py --base <base> --plan` and follow every selected intent checklist and VM detail. Rust implementation changes require at least `cargo test` unless the VM requires a stronger command.
- Point-in-time evidence belongs in the PR, exact CI run, versioned release evidence, or retained history. Do not treat ignored `rust/target/`, `dist/`, temporary, or `.local.*` files as durable current truth.

## 4. Change Guardrails

- Update indexer tests first when changing FileList case, priority, or root-only detection.
- Run both ignored VM-003 indexing performance guards when changing `rust/src/indexer/`, `rust/src/app/index_worker.rs`, or related indexing coordination; read the selected VM-003 detail for additional conditional gates.
- Dependency, Cargo, workflow, updater, or packaging changes must keep `THIRD_PARTY_NOTICES.txt`, license sidecars, `docs/OSS_COMPLIANCE.md`, and related release docs consistent.
- Never place development/manual-test update override names in README, public release docs/templates, release text, or user-facing CLI/GUI help. The forbidden-name list is owned by the release preflight.
- Do not weaken tests, skip validation, hide warnings, broaden refactors, or change specs only to make a failing check pass.

## 5. Release And CI

- `docs/RELEASE.md` owns release assets, checksums, notarization posture, build scripts, and publication procedure. Use `skills/flistwalker-release-preflight/` before any tag/release/publish task and `skills/flistwalker-release-notes/` for CHANGELOG or release text.
- `docs/CI_OPERATIONS.md` owns CI pins, trusted-policy rollout, branch protection, canary/audit response, rollback, and proof-PR requirements.
- Required PR checks are `CI Gate` and `CI Policy Guardian`. Do not use `pull_request_target` outside the read-only trusted guardian, execute PR code there, weaken permissions, or bypass required checks.
- Do not create/push tags or publish/edit/delete a release unless the user explicitly requested that release operation and all release gates are complete.

## 6. Git, Worktrees, And External State

- Never push directly to `master`. Use a machine PR with rebase auto-merge; merge and squash commits are disabled.
- Before editing or branch mutation, use `skills/flistwalker-pr-lifecycle/` and its read-only worktree preflight. Never switch, reset, clean, commit in, or delete a branch owned by another worktree.
- A stacked change records its old-parent SHA and, after the parent merges, replays only task-owned commits onto updated `origin/master`; rerun validation and independent review after replay.
- Do not include pre-existing or unrelated changes in a commit. Do not use `git reset --hard`, manual remote branch deletion, `master` force push, or admin bypass. The prescribed PR command and repository auto-delete setting may perform server-side post-merge cleanup.
- Repository-setting changes, immutable CI rollout, external application launch, sensitive-data transmission, tag/release publication, and destructive operations require the exact authority defined in `docs/AI_DEVELOPMENT.md`.

## 7. Project-Local Skills

- `skills/flistwalker-pr-lifecycle/`: worktree mode, PR creation, rebase auto-merge, merge confirmation, and safe owner-worktree cleanup.
- `skills/flistwalker-change-review/`: independent read-only review for high-risk changes and plan-driven checkpoints.
- `skills/flistwalker-release-preflight/`: release readiness gate.
- `skills/flistwalker-release-notes/`: release-range evidence and public release text.
