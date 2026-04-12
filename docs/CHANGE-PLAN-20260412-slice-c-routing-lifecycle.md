# CHANGE PLAN: Routing and Lifecycle Cleanup

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260412-roadmap-architecture-idealization.md](./CHANGE-PLAN-20260412-roadmap-architecture-idealization.md)
- Child Plan(s): none
- Scope Label: routing-lifecycle
- Related Tickets/Issues: none
- Review Status: レビュー中
- Review Notes:
  - Slice B の closure 判断で、request routing と tab lifecycle がまだ split していると判定された。
  - この slice は closure の前提を満たすための残差であり、理想形を削るための縮小版ではない。

## 1. Background
- `tabs.rs` には今も action request polling、root browse UI/state 操作、tab lifecycle transition が混在している。
- `request_id` routing は request 種別ごとに別 owner へ散っており、response apply と cleanup の責務が一箇所に定まっていない。
- ここを詰めないまま closure に進むと、`tabs.rs` が thin shell になったとは言えない。

## 2. Goal
- request routing の所有権を明示し、request 種別ごとの bind/take/clear を同一の routing owner に寄せる。
- polling entrypoint を揃え、`worker_runtime.rs` の dispatch は owner 境界ごとに統一された呼び口を使う。
- `tabs.rs` は tab activation / close / reorder / restore の lifecycle に限定し、root browse や request routing を持ち込まない。
- tab close 時の request cleanup は one-shot の owner path に集約し、preview/action/sort の cleanup をバラさない。

## 3. Scope
### In Scope
- `rust/src/app/search_coordinator.rs`
- `rust/src/app/index_coordinator.rs`
- `rust/src/app/worker_bus.rs`
- `rust/src/app/preview_flow.rs`
- `rust/src/app/result_flow.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/input.rs`
- `rust/src/app/worker_runtime.rs`
- `rust/src/app/state.rs`
- 必要に応じて追加する request routing 用の新規 owner module

### Out of Scope
- search/index core algorithm changes
- UI redesign beyond lifecycle separation
- application feature changes

## 4. Constraints and Assumptions
- request_id を使った stale response rejection は維持する。
- tab switch / restore / close / reorder の現行挙動は壊さない。
- background responses は current tab と background tab を混同しない。

## 5. Current Risks
- Risk: routing owner を新設しても API が分散したままだと、split が見えにくくなる。
  - Impact: cleanup が散らかり、保守性が改善しない。
  - Mitigation: bind/take/clear/poll の呼び口を routing owner に統一する。
- Risk: lifecycle cleanup の共通化で close 時の副作用が変わる。
  - Impact: tab close 後の残留 state や stale response の誤反映。
  - Mitigation: close path に unit tests を追加し、request routing の清掃順序を固定する。

## 6. Execution Strategy
1. Phase C1: routing owner normalization
   - Files/modules/components: `worker_bus.rs`, `search_coordinator.rs`, `index_coordinator.rs`, `preview_flow.rs`, `result_flow.rs`, `worker_runtime.rs`
   - Expected result: request routing helpers and polling entrypoints share one ownership model instead of being split by request type.
   - Verification: background response routing tests and queue/inflight tests.
   - Entry condition: slice B review has identified the remaining routing split and the owner boundary to normalize is frozen.
   - Exit condition: bind/take/clear/poll operations follow one routing boundary and no longer depend on per-request-file ownership drift.
2. Phase C2: lifecycle cleanup consolidation
   - Files/modules/components: `tabs.rs`, `input.rs`, `state.rs`
   - Expected result: `tabs.rs` keeps lifecycle transitions, and close-time cleanup uses one path for preview/action/sort/request routing.
   - Verification: tab close / switch / reorder tests and stale response cleanup tests.
   - Entry condition: routing owner normalization has stabilized the remaining request maps.
   - Exit condition: tab lifecycle ownership is isolated and non-lifecycle UI/state operations are moved out of `tabs.rs`.

## 7. Detailed Task Breakdown
- [ ] request routing owner を一本化する。
- [ ] polling entrypoint を揃える。
- [ ] tab close cleanup を one-shot path にまとめる。
- [ ] `tabs.rs` を lifecycle 専用に寄せる。

## 8. Validation Plan
- Automated tests:
  - `cargo test`
  - `rust/src/app/tests/session_tabs.rs`
  - `rust/src/app/tests/session_restore.rs`
  - `rust/src/app/tests/index_pipeline/*`
- Manual checks:
  - tab close / reopen
  - background response landing after tab close
  - root browse and tab lifecycle interactions
- Regression focus:
  - stale response rejection
  - cleanup ordering
  - lifecycle-only ownership

## 9. Rollback Plan
- If request routing consolidation destabilizes response handling, revert the new routing boundary first and keep the closure block in place.
- Keep lifecycle cleanup changes separate from routing normalization so one can be reverted without losing the other.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-idealization`, read the roadmap and earlier slices before starting implementation.
- Read them in this order:
  - `docs/CHANGE-PLAN-20260412-roadmap-architecture-idealization.md`
  - `docs/CHANGE-PLAN-20260412-slice-a-core-boundary.md`
  - `docs/CHANGE-PLAN-20260412-slice-b-shell-decomposition.md`
  - `docs/CHANGE-PLAN-20260412-slice-c-routing-lifecycle.md`
  - `docs/CHANGE-PLAN-20260412-slice-d-closure.md`
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Phase execution is delegated to subagents by default; the main agent acts as orchestrator and reviewer.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Keep the roadmap open until the closure slice has been completed and the goal-validation result has been recorded.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12  Planned after closure review found routing/lifecycle ownership still split.

## 12. Communication Plan
- Return to user when the slice is reviewed and ready for implementation, or when a blocking issue requires plan update.

## 13. Completion Checklist
- [ ] Plan created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Routing owner normalization completed
- [ ] Lifecycle cleanup completed
- [ ] Verification completed
- [ ] Temporary rule removed after completion

## 14. Final Notes
- This slice exists because the closure review found a real architectural gap, not because the target was too ambitious.
