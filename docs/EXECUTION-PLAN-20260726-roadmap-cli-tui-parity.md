# EXECUTION PLAN: CLI / TUI usability parity roadmap

## Control
- Date: 2026-07-26
- Owner: main agent (Sol planning/integration owner)
- Target Project: FlistWalker
- Plan Role: roadmap
- Execution Profile: strict
- Risk Tier: medium
- Required Safety Controls: External action, cross-process session merge, and FileList write behavior are applicable; status is planned; main agent owns approval boundaries and evidence; automated tests MUST use recording backends and temporary roots; real external application launch and writes outside test roots are not validation steps; completion requires contract matrices, authorization/write-plan evidence, merge/concurrency tests, and recorded manual terminal evidence.
- Planning Depth: roadmap+slice
- Review Pattern: staged-subagents
- Review Requiredness: required-before-implementation-and-final
- Review Placement: before-and-final
- Execution Mode: autonomous
- Execution Mode Policy: After the user reviews the ready roadmap, the main agent may advance through ready slices, delegate bounded implementation items to Terra, integrate and commit each verified rollback unit, and stop only for blocking findings or material scope/compatibility/safety changes. Planning, manifest, integration, commits, and closure remain main-agent owned.
- Plan Readiness: ready-for-implementation
- Parent Plan: none
- Child Plan(s):
  - `docs/EXECUTION-PLAN-20260726-slice-a-cli-contracts-shared-core.md`
  - `docs/EXECUTION-PLAN-20260726-slice-b-batch-cli.md`
  - `docs/EXECUTION-PLAN-20260726-slice-c-tui-selection.md`
  - `docs/EXECUTION-PLAN-20260726-slice-d-tui-operations.md`
  - `docs/EXECUTION-PLAN-20260726-slice-e-closure.md`
- Work Item Manifest: `docs/EXECUTION-WORK-ITEMS-20260726-cli-tui-parity.json`
- Scope Label: CLI/TUI usability parity
- Related Docs/Tickets: FR-006, FR-010, FR-011, FR-025, FR-026; SP-001, SP-004, SP-006, SP-010, SP-013, SP-015, SP-016; DES-005, DES-006, DES-016, DES-017; TC-006, TC-006A, TC-050, TC-052, TC-054, TC-057B, TC-110, TC-111, TC-115, TC-162

### Temporary Ownership
- AGENTS.md Initial State: existing
- Temporary AGENTS.md Ownership: section-only
- Docs Directory Initial State: existing
- Temporary Docs Directory Ownership: existing-directory

## Goal
- Outcome: Batch CLI and interactive TUI gain the accepted GUI-derived usability features without breaking script-safe stdout, terminal restoration, action authorization, or FileList safety.
- Success Conditions: All accepted features are implemented, SDD trace is synchronized, required automated and terminal validations pass, logical commits exist, and final independent review has no unresolved blocking/major finding.
- Closure Slice: final slice validates goal completion and close-or-continue decision.

## Scope
### In
- Batch CLI: explicit action mode, result sorting, opt-in default/saved-root access, saved-root listing, and explicit FileList creation.
- TUI: preview, open/reveal actions, query history, query/pin clear actions, help, sorting, runtime filter toggles, saved-root switching, refresh, and FileList creation.
- Shared non-GUI domain extraction for sort, history/root persistence access, action authorization/execution, and FileList operation primitives where needed.
- TDD, SDD updates, README help, terminal recovery checks, and logical commits.

### Out
- CLI/TUI multi-tab and tab session restoration.
- Silent use of GUI default root by batch CLI.
- Changing default batch stdout framing or TUI `Enter` selection-output behavior.
- Arbitrary shell command execution, shell expansion, or plugin-style action hooks.

## Constraints And Assumptions
- Constraint: Existing `--cli [QUERY] --root ... --limit ...` and path-only stdout contracts remain compatible.
- Constraint: `Enter` in TUI continues to restore the terminal and emit selected paths; opening uses a distinct key or explicit action mode.
- Constraint: Non-score sorting is applied to the full match set before `limit`; score sorting retains current ranking behavior.
- Constraint: Batch external actions are explicit, reject accidental multi-target action unless a separately explicit all-target opt-in is present, and reuse root-bound authorization.
- Constraint: TUI external actions always target the current row only; pins affect selection output only and never fan out side effects.
- Constraint: Default/saved roots are opt-in in batch mode; current-directory default remains unchanged.
- Constraint: Preview, sorting metadata, refresh, FileList creation, and external actions do not block the TUI input/render loop.
- Constraint: FileList overwrite and ancestor propagation require explicit flags in non-interactive batch mode; cancellation is honored before replacement/propagation boundaries.
- Constraint: FileList writes use a precomputed target plan, report every committed/failed/rolled-back target, never swallow ancestor I/O errors, and attempt rollback after partial failure/cancellation.
- Constraint: TUI cannot return, switch root, or emit selection while a FileList transaction is unsettled; cancellation keeps the event loop responsive until rollback/report completion, and FileList worker handles are never timeout-detached.
- Constraint: TUI/GUI history uses an asynchronous coalescing persistence worker and locked latest-read/delta-merge/atomic-write operation that preserves GUI session/default-root/tabs and unknown JSON fields without blocking GUI/TUI frame dispatch.
- Assumption: Existing public preview, action, indexer, query, and path APIs can be reused or extracted without changing GUI behavior.

## Execution
1. Slice A: Freeze SDD/API contracts and extract shared core boundaries.
   - Expected Result: failing tests describe all new behavior; GUI semantics remain characterized.
   - Verification: focused unit/contract tests compile and fail for missing behavior before implementation, then pass for shared core.
   - Rollback Boundary: shared core and contract commit.
2. Slice B: Implement batch CLI additions.
   - Expected Result: new flags are explicit and script-safe.
   - Verification: CLI integration tests plus focused action/FileList tests.
   - Rollback Boundary: batch CLI feature commit.
3. Slice C: Implement TUI selection experience.
   - Expected Result: preview/actions/history/clear/help work without changing stdout ownership.
   - Verification: TUI unit tests, terminal guard tests, pseudo-terminal/manual evidence.
   - Rollback Boundary: TUI selection feature commit.
4. Slice D: Implement long-lived TUI operations.
   - Expected Result: sort/toggles/root/refresh/FileList operations are asynchronous and cancellable.
   - Verification: worker freshness/cancellation tests, CLI contract tests, full Rust suite.
   - Rollback Boundary: TUI operations feature commit.
5. Slice E: Closure.
   - Expected Result: docs, complete validation, independent final review, cleanup, and close-or-continue decision.
   - Verification: validation matrix, diff checks, final review record.
   - Rollback Boundary: docs/closure commit; temporary plan cleanup only after closure gate.

### Work Item Manifest
- Manifest Path: `docs/EXECUTION-WORK-ITEMS-20260726-cli-tui-parity.json`
- Ready Item Rule: dependencies complete and the owning slice is ready.
- Completion Evidence Rule: changed files, focused tests, and result summary are recorded.
- Repair Rule: repair the smallest failed item; update this roadmap before changing scope or compatibility.

## Validation
- Automated: `cargo test --test cli_contract`; `cargo test cli_tui::tests`; focused action/indexer/session/sort tests; `cargo test`; `cargo check`; `cargo clippy --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `git diff --check`.
- Manual: Windows terminal TUI evidence for stdout redirection, preview/help layout, action keys using safe fixture targets only when explicitly approved, resize/paste, cancellation, and terminal restoration; WSL/non-UTF8 path evidence when available.
- Regression Focus: path-only stdout, NUL framing, exit codes, request freshness, selection preservation, ordered hidden pins, root authorization, terminal cleanup-before-output, GUI sort/action/history behavior.
- Performance/Security: ignored indexing perf tests when index paths change; release search perf test when ranking/materialization changes; no real external action during automated checks.

## Trace
- SDD Impact: required
- Related FR/NFR/CON: FR-006, FR-010, FR-011, FR-025, FR-026, NFR-001, NFR-006, NFR-011
- Related SP/DES/TC: SP-001, SP-004, SP-006, SP-010, SP-013, SP-015, SP-016; DES-005/006/016/017; TC-006/006A/050/052/054/057B/110/111/115/162 plus new focused cases if existing IDs cannot express the additions.
- Docs To Update Before Closure: README.md, README-ja.md, REQUIREMENTS/SPEC/DESIGN/TESTPLAN topic files and traceability maps.

## Review
- Review Viewpoints: purpose-scope, responsibility-boundary, ordering, validation, rollback, architecture, testing, security, operability
- Checkpoint(s): before-and-final

### Before-Implementation Checkpoint
- Status: レビュー済み
- Reviewer Independence / Prior Involvement: fresh-cycle reviewer independent from material author and implementation owners; prior involvement limited to the same cycle's initial findings
- Capability Class: standard
- Provenance: 2026-07-26 initial and fresh-cycle independent `gpt-5.6-sol` reviews; fresh-cycle focused re-review by `/root/cli_tui_replan_review` returned GO with blocking 0 / major 0 / minor 0
- Findings And Disposition:
  - Public contract granularity and FileList write safety (blocking): resolved by normative matrices, precomputed write plan, rollback/report contract, and focused fixtures.
  - Side-effect lifecycle, persistence privacy/merge, manifest boundedness, and sort closure (major): resolved by current-only TUI actions, async delta-merge persistence, 15 sequential items, shared search ownership, and performance gates.
  - FileList terminal settlement and history UI-blocking gaps found by the first focused re-review (major): resolved by no-detach settlement and async coalescing persistence redesign.
  - Fresh-cycle intent precedence, coalescing algebra, and batch rollback exit priority (major): resolved; fresh-cycle focused re-review confirmed all three and returned zero findings.

### Final Checkpoint
- Status: 未レビュー
- Reviewer Independence / Prior Involvement: independent from implementation owners; prior involvement recorded at invocation
- Capability Class: standard
- Provenance: pending focused final review
- Findings And Disposition: pending

## Temporary `AGENTS.md` Rule
- Use the concise standard draft from `plan-driven-execution/references/temporary-rule-and-close.md` after readiness and before-implementation review.
- Plan Paths: this roadmap followed by slices A through E.
- Overrides: Terra owns bounded implementation items; Sol owns planning, integration, commit, safety, and closure decisions.

## Progress Log
- 2026-07-26 Planned from accepted GUI-to-CLI/TUI parity scope.
- 2026-07-26 Main-agent preflight completed for scope, dependency order, validation, rollback, stop conditions, and contract matrices; plans marked ready-for-implementation, but implementation remains blocked until focused re-review passes.
- 2026-07-26 Focused re-review resolved the original six findings but found two new major gaps. Execution remains NO-GO; replanning completed for FileList settlement and asynchronous history delta merge. No further independent review will be invoked without explicit user authorization.
- 2026-07-26 User authorized a fresh independent review cycle. Its initial review found three contract ambiguities; remediation was added and the cycle's single focused re-review is pending.
- 2026-07-26 Fresh-cycle focused re-review completed: GO for implementation, blocking 0, major 0, minor 0.

## Closure Capsule
- Verification Result: pending
- Review Result: pending
- Required Final Review Complete: no
- Findings Handled: no
- Required Revalidation Complete: no
- Durable Docs Updated: no
- Rollback Ready: logical commits planned per slice
- Remaining Risks: external action fan-out, FileList overwrite/propagation, TUI worker lifecycle, metadata-sort cost
- Temporary Artifacts Removable Without Losing Open Work: no
- Close Decision: do not close

### Strict Extension
- Impacted Contracts/Compatibility: CLI stdout/exit codes, TUI Enter/Esc behavior, GUI shared sort/action/history semantics.
- Layer Or Responsibility Boundaries: shared domain APIs remain GUI-independent; adapters own flags/keys/rendering; workers own I/O.
- Review Checkpoint / Findings Disposition Notes: before-and-final independent checkpoints are mandatory.
- Additional Verification: terminal lifecycle and recording-backend authorization evidence.
