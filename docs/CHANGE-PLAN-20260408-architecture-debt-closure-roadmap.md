# CHANGE PLAN: FlistWalker Architecture Debt Closure Roadmap

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Parent Plan: none
- Child Plan(s): `docs/CHANGE-PLAN-20260408-architecture-debt-closure-perf-gate-slice.md`
- Scope Label: architecture-debt-closure
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Initial review found no blockers. The slice ordering is acceptable, and the feature freeze language now allows debt-closure docs/tests to continue while new feature work stays paused.

## 1. Background
- The coordinator, worker/domain, and open/execute seam cleanup have removed the highest-friction structural issues, but the project still has visible architecture debt in updater hardening, perf gates, diagnostics, and docs/process separation.
- The remaining debt is not feature work. It is the work required to make the current system easier to reason about, verify, and operate before new features resume.
- This roadmap defines a freeze on new feature work until the listed debt classes are resolved or deliberately re-scoped. Debt-closure implementation, plan updates, and the docs/tests needed to support those slices remain in scope.

## 2. Goal
- Resolve the currently visible architecture debt in priority order and leave the project in a state where feature work can resume without immediately reintroducing structural risk.
- Keep each debt class isolated so that updater, perf, diagnostics, and docs concerns do not collapse back into one large refactor.
- Make the following outcomes observable:
  - updater behavior is split into clear decision/apply boundaries and contract coverage,
  - at least one lightweight perf gate is available in PR CI,
  - diagnostics support real investigation instead of only happy-path operation,
  - docs distinguish normative, operational, and historical/closure content.

## 3. Scope
### In Scope
- Updater hardening and self-update contract boundaries
- Perf gate strengthening and budget enforcement
- Diagnostics and supportability improvements
- Docs restructuring needed to close the debt program cleanly

### Out of Scope
- New feature work
- UI redesign
- Release packaging policy changes unrelated to the listed debt classes

## 4. Constraints and Assumptions
- The existing SDD/TDD document set remains the source of truth for stable requirements, specs, design, and tests.
- Feature work is paused until this debt program is closed or explicitly re-scoped. Work needed to keep the roadmap, slices, and supporting docs consistent is still allowed.
- Each slice may be updated if discovery reveals a narrower or safer boundary, but the roadmap should continue to reflect the currently visible debt classes.
- The roadmap is intentionally broader than a single slice; the active slice and its subslice will carry the implementation detail.

## 5. Current Risks
- Risk:
  - The debt classes may interact and create scope creep.
  - Impact:
    - updater hardening could spill into perf, docs, or release policy work.
  - Mitigation:
    - Keep slices orthogonal and update the roadmap first when boundaries move.
- Risk:
  - Perf gates can become noisy if added before boundaries are stable.
  - Impact:
    - PR checks become expensive without catching meaningful regressions.
  - Mitigation:
    - Introduce only the lightest useful gate first and keep the heavy suite separate.
- Risk:
  - Docs work can absorb implementation details and lose its closure role.
  - Impact:
    - the roadmap turns back into a design notebook.
  - Mitigation:
    - Keep docs restructuring as a closure slice, not a dumping ground for code detail.

## 6. Execution Strategy
1. Slice A: Updater Hardening
   - Purpose:
     - Separate updater decision logic, staged apply flow, and app command boundaries so self-update behavior is easier to verify and less risky to change.
   - Boundary:
     - `rust/src/updater.rs`
     - `rust/src/app/update.rs`
     - `rust/src/app/workers.rs`
     - `rust/src/app/state.rs`
     - update-related docs and tests
   - Dependency / Ordering:
     - First slice. Self-update is the most release-sensitive remaining debt.
   - Entry condition:
     - This roadmap and its active subslice are reviewed and the temporary AGENTS rule points to them.
   - Exit condition:
     - updater boundaries are explicit, and the active contract paths are backed by tests.
2. Slice B: Perf Gate Strengthening
   - Purpose:
     - Add a lightweight perf gate to PR CI so the most likely regressions are caught without moving the full perf suite into the hot path.
   - Boundary:
     - perf regression workflow, validation matrix, and related docs
   - Dependency / Ordering:
     - After updater boundaries are stable enough that perf changes can be measured meaningfully.
   - Entry condition:
     - updater and search/index boundaries are stable enough to define a cheap, useful gate.
   - Exit condition:
     - PR CI has a lightweight perf guard and the heavy suite remains clearly separated.
3. Slice C: Diagnostics and Supportability
   - Purpose:
     - Make runtime tracing and support investigation usable for the project’s real failure modes.
   - Boundary:
     - tracing, debug hooks, request identifiers, latency/counter notes, and support docs
   - Dependency / Ordering:
     - After the major request/response owners are stable enough that diagnostics can be attached to the right boundaries.
   - Entry condition:
     - request flow ownership is stable enough to define diagnostic events.
   - Exit condition:
     - common failure cases can be diagnosed from the available logs and debug hooks.
4. Slice D: Docs and Closure Restructuring
   - Purpose:
     - Separate normative docs, operational docs, and historical/closure material, then close out the debt program cleanly.
   - Boundary:
     - `docs/ARCHITECTURE.md`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/TASKS.md`
     - any final closure notes needed to keep the repo readable
   - Dependency / Ordering:
     - Last, after the code-facing debt slices settle.
   - Entry condition:
     - updater, perf, and diagnostics slices have produced durable decisions.
   - Exit condition:
     - docs no longer mix live architecture guidance with temporary debt-program scaffolding.

## 6.1 Future Workstreams Not Yet Drafted
- Any new debt class discovered during the above slices should be added here first, then cut into a new slice only after the roadmap is updated.

## 7. Detailed Task Breakdown
- [ ] Freeze new feature work until the debt program is closed or re-scoped
- [ ] Draft the updater hardening slice and active subslice
- [ ] Define the first lightweight perf gate candidate and budget
- [ ] Define the diagnostics/supportability boundary
- [ ] Define the docs/closure cleanup boundary

## 8. Validation Plan
- Automated tests:
  - roadmap itself is docs-only
  - each slice will carry its own `cargo test` and follow-up checks
- Manual checks:
  - roadmap / slice / subslice parent-child relation
  - roadmap and `TASKS.md` snapshot alignment
  - roadmap stays at debt-class level and does not spill implementation detail into itself
- Performance or security checks:
  - perf/security specifics are delegated to the relevant slices
- Regression focus:
  - the roadmap must remain a debt-closure plan, not a hidden feature backlog

## 9. Rollback Plan
- This roadmap can be replaced or rewritten without code rollback.
- If the ordering proves wrong, update this roadmap before changing any slice order.
- If a debt class disappears or expands, adjust the slice list rather than forcing unrelated work into the current plan.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-debt-closure`, read `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md]`, `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-slice.md]`, and `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-updater-subslice.md]` before starting implementation.
- Execute the updater hardening work in the documented order unless the roadmap, slice, or subslice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Planned architecture debt closure roadmap created after previous plan close.
- 2026-04-08: Slice A active batch split the updater decision path into asset-selection and support-classification helpers, with contract tests added.
- 2026-04-08 00:00 Slice A Phase 1 completed: updater candidate resolution is now isolated from staged apply work; Phase 2 docs/contract sync remains pending.
- 2026-04-08 00:00 Slice A Phase 2 completed: updater command-surface comments and docs/testplan references now match the current boundary.
- 2026-04-08 00:00 Slice A closed and Slice B drafted as the next active slice for perf gate strengthening.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- Keep the roadmap focused on debt closure, not new feature work.
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.
