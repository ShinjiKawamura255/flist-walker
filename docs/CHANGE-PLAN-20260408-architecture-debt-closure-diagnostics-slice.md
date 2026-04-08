# CHANGE PLAN: Diagnostics and Supportability Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md`
- Child Plan(s): none
- Scope Label: architecture-debt-diagnostics
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: No blockers. The slice stays narrow, the two phases are enough, and the scope remains on tracing/supportability rather than a logging rewrite.

## 1. Background
- The project already emits some warnings and window trace data, but the failure modes that matter for support are still spread across worker transitions and notice strings.
- Diagnostics should help answer “what happened” without requiring ad hoc reproduction steps or code reading.
- This slice exists after the updater and perf boundaries are stable enough that logging and support hooks can attach to a reasonably steady contract.

## 2. Goal
- Make runtime tracing and support investigation usable for the project’s real failure modes.
- Keep the diagnostics changes lightweight and closeable, not a new logging framework.
- Make the following outcomes observable:
  - update and worker request transitions have explicit trace points,
  - request IDs or similar correlation points are visible where failures matter,
  - support notes explain how to inspect the resulting logs/traces.

## 3. Scope
### In Scope
- tracing and debug hooks
- request identifier visibility in failure paths
- latency/counter notes that help support
- support docs and validation notes

### Out of Scope
- updater contract changes
- perf budget changes
- new user-facing features

## 4. Constraints and Assumptions
- Diagnostics must stay behind existing trace/log hooks and not create new runtime dependencies.
- Any new trace output should be useful in both local reproduction and CI logs.
- Any docs change must keep the current SDD/TDD traceability intact.

## 5. Current Risks
- Risk:
  - the new traces could be noisy or redundant.
  - Impact:
    - supportability gets worse instead of better.
  - Mitigation:
    - log only transitions and failure points that help correlate request flow.
- Risk:
  - the diagnostics work could drift into perf or updater contract changes.
  - Impact:
    - the slice loses focus.
  - Mitigation:
    - keep the work to tracing, support hooks, and docs only.

## 6. Execution Strategy
1. Phase 1: Add explicit trace points for request flow and support failures
   - Files/modules/components:
     - `rust/src/app/workers.rs`
     - `rust/src/app/update.rs`
     - `rust/src/app/mod.rs` if needed for trace helper access
   - Expected result:
     - update / worker failure paths emit traceable events with request IDs or equivalent correlation data.
   - Verification:
     - `cargo test`
2. Phase 2: Sync support docs and validation notes
   - Files/modules/components:
     - `docs/ARCHITECTURE.md`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/TASKS.md`
   - Expected result:
     - support and troubleshooting notes describe how to read the new traces and what they mean.
   - Verification:
     - `cargo test`
     - targeted docs review

## 7. Detailed Task Breakdown
- [ ] Add explicit trace points for update and worker failures
- [ ] Keep request IDs visible where support benefits from correlation
- [ ] Document the resulting support/troubleshooting flow
- [ ] Keep diagnostics changes out of perf/update contract scope

## 8. Validation Plan
- Automated tests:
  - `cargo test`
- Manual checks:
  - trace output from a representative failure path
  - docs/support note alignment
- Performance or security checks:
  - trace volume should remain lightweight
- Regression focus:
  - request correlation clarity
  - failure-path discoverability

## 9. Rollback Plan
- Diagnostics traces can be removed without touching the underlying worker or updater logic.
- If the traces are too noisy, reduce them before broadening the scope.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-debt-closure`, read `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md]` and `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-diagnostics-slice.md]` before starting implementation.
- Execute the diagnostics work in the documented phase order unless the roadmap or slice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Drafted as the active diagnostics slice after perf gate stabilization.
- 2026-04-08 00:00 Phase 1 completed: update request and response traces now carry request_id-correlated support details.
- 2026-04-08 00:00 Phase 2 completed: supportability notes are synchronized in architecture/design/testplan/task tracking docs.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Work executed according to the plan or the plan updated first
- [x] Verification completed
- [x] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- Keep diagnostics grounded in actual support questions.
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.
