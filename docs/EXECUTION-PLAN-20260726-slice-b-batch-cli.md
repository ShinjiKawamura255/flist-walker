# EXECUTION PLAN: Slice B - batch CLI

## Control
- Date: 2026-07-26
- Owner: main agent; Terra implementation delegate
- Target Project: FlistWalker
- Plan Role: slice plan
- Execution Profile: strict
- Risk Tier: medium
- Required Safety Controls: explicit action fan-out opt-in; root authorization; explicit FileList overwrite/ancestor propagation; temporary-root tests.
- Planning Depth: roadmap+slice
- Review Pattern: staged-subagents
- Review Requiredness: required-before-implementation-and-final
- Review Placement: before-and-final
- Execution Mode: none
- Execution Mode Policy: inherit roadmap
- Plan Readiness: ready-for-implementation
- Parent Plan: `docs/EXECUTION-PLAN-20260726-roadmap-cli-tui-parity.md`
- Child Plan(s): none
- Work Item Manifest: `docs/EXECUTION-WORK-ITEMS-20260726-cli-tui-parity.json`
- Scope Label: batch CLI parity

### Temporary Ownership
- AGENTS.md Initial State: existing
- Temporary AGENTS.md Ownership: section-only
- Docs Directory Initial State: existing
- Temporary Docs Directory Ownership: existing-directory

## Goal
- Outcome: Batch users can sort, explicitly open/reveal, opt into stored roots, list roots, and create FileList safely.
- Success Conditions: New options are documented/tested; defaults and stdout compatibility remain unchanged.
- Closure Slice: not applicable

## Scope
### In
- Exact public flags and combinations defined by Slice A's Batch CLI matrix: `--sort`, `--action`, `--action-all`, `--use-default-root`, `--saved-root`, `--list-saved-roots`, `--create-filelist`, `--overwrite-filelist`, and `--propagate-ancestors`.

### Out
- Arbitrary command templates, shell evaluation, implicit mass open, clipboard mutation.

## Constraints And Assumptions
- Sorting occurs before `limit`; `score` preserves existing order.
- `print` remains default and the only mode using existing stdout result framing.
- Non-print action diagnostics use stderr and return nonzero on blocked/partial failure.
- FileList creation never prompts in batch mode; missing consent flags cause a clear refusal.
- FileList target planning, rollback reporting, and persistence locking follow Slice A contracts; adapter code must not duplicate them.
- Batch FileList exit priority is fixed: success 0; clean cancellation with no commit or complete rollback 130; any I/O or rollback failure 1, including cancellation-triggered rollback failure.

## Execution
1. Implement clap validation and typed batch options.
2. Reuse the shared all-match search/sort API and deterministic metadata missing-value ordering.
3. Implement authorized action dispatch and partial-completion reporting.
4. Implement stored-root reads and FileList creation flow.
5. Add README examples and integration tests.

## Validation
- Automated: `cargo test --test cli_contract`, focused action/indexer/session/sort tests, cancellation tests, full suite.
- Manual: dry fixture invocation; no real external application launch.
- Regression Focus: output bytes, NUL/non-UTF8 handling, exit codes, legacy invocation, commit-then-cancel rollback success/failure classification.
- Performance/Security: search perf if full-match materialization changes; authorization tests for escape/link cases.

## Trace
- SDD Impact: required
- Related FR/NFR/CON: FR-006/010/026, NFR-001/006
- Related SP/DES/TC: SP-001/004/006/010/013/015/016, DES-005/006/016/017, TC-006A/050/052/057B/110/111/115
- Docs To Update Before Closure: README and CLI SDD sections.

## Review
- Review Viewpoints: compatibility, security, validation, rollback, operability
- Checkpoint(s): workflow-level before-and-final

## Temporary `AGENTS.md` Rule
- Plan Paths: roadmap then this slice.
- Overrides: real external action validation requires explicit user approval.

## Progress Log
- 2026-07-26 Planned.
- 2026-07-26 Main-agent preflight completed; implementation awaits workflow-level focused re-review.
- 2026-07-26 `slice-b.options-sort` completed by Terra and integrated by Sol; all nine shared sort modes, sort-before-limit, default/saved root selectors, exclusive root listing, and legacy framing are covered by TC-163. Focused CLI/bin tests, full library regression, check, clippy, fmt, and diff checks passed; one unrelated ordering-sensitive index-worker test failed once in aggregate and passed targeted plus full-library rerun.

## Closure Capsule
- Verification Result: pending
- Review Result: pending
- Required Final Review Complete: no
- Findings Handled: no
- Required Revalidation Complete: no
- Durable Docs Updated: no
- Rollback Ready: yes, batch feature commit
- Remaining Risks: action fan-out and full-match metadata cost
- Temporary Artifacts Removable Without Losing Open Work: no
- Close Decision: do not close

### Strict Extension
- Impacted Contracts/Compatibility: new opt-in flags only; existing defaults frozen.
- Layer Or Responsibility Boundaries: main.rs adapter delegates shared operations.
- Review Checkpoint / Findings Disposition Notes: workflow-level checkpoints.
- Additional Verification: byte-level stdout assertions.
