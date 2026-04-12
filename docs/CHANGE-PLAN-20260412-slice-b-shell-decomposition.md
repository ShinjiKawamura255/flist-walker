# CHANGE PLAN: Shell Decomposition and State Ownership

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
- Scope Label: shell-decomposition
- Related Tickets/Issues: none
- Review Status: レビュー中
- Review Notes:
  - 初回レビューでは phase gating を明確にすべきという指摘があった。
  - この slice は thin shell を実現する中核であり、state projection と orchestration の境界を固定する。

## 1. Background
- `app/mod.rs`、`state.rs`、`tabs.rs`、`pipeline.rs` が大きく、state の所有権と同期が広い範囲で手書きになっている。
- 理想形では、shell は boot / routing / lifecycle / reducer / render / persistence に分解され、各 owner が明確に責務を持つ。

## 2. Goal
- `FlistWalkerApp` は entrypoint と orchestration の薄い層に縮小し、feature 単位の state machine は dedicated owner に移す。
- tab snapshot は canonical projection に一本化し、活 state と persisted state の双方向コピーを減らす。
- search/index/preview/action/sort/update/filelist の response apply は reducer boundary に集約する。

## 3. Scope
### In Scope
- `rust/src/app/mod.rs`
- `rust/src/app/bootstrap.rs`
- `rust/src/app/session.rs`
- `rust/src/app/render.rs`
- `rust/src/app/input.rs`
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/pipeline.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/result_flow.rs`
- `rust/src/app/preview_flow.rs`
- `rust/src/app/search_coordinator.rs`
- `rust/src/app/index_coordinator.rs`
- `rust/src/app/worker_bus.rs`
- `rust/src/app/ui_state.rs`
- `rust/src/app/query_state.rs`
- `rust/src/app/filelist.rs`
- `rust/src/app/update.rs`

### Out of Scope
- search/index core algorithm changes
- UI redesign beyond shell separation
- OS integration behavior changes

## 4. Constraints and Assumptions
- current behavior for tab switch, restore, close, reorder, and background responses must remain intact.
- request_id routing must continue to protect against stale responses.
- UI responsiveness remains the highest policy.

## 5. Current Risks
- Risk: projection logic splits into many small mirrors and becomes harder to reason about.
  - Impact: bugs during restore or tab switch.
  - Mitigation: define one canonical projection path and test it from snapshot creation to restore.
- Risk: reducer extraction accidentally duplicates state transition logic.
  - Impact: background and active response handling diverge.
  - Mitigation: keep one reducer per semantic transition and share helpers where the transition is identical.

## 6. Execution Strategy
1. Phase B1: shell responsibility map and owner boundaries
   - Files/modules/components: `app/mod.rs`, `app/bootstrap.rs`, `app/session.rs`, `app/render.rs`, `app/input.rs`
   - Expected result: coordinator entrypoints become thin dispatchers with explicit owner calls.
   - Verification: app startup/shutdown/session tests.
   - Entry condition: slice A is complete, baseline `cargo test` is green, and the owner map for shell entrypoints is frozen.
   - Exit condition: boot/dispatch/persistence の入口と owner 境界が一覧化され、mod.rs に残す責務が明確になる。
2. Phase B2: canonical tab/state projection
   - Files/modules/components: `state.rs`, `tab_state.rs`, `tabs.rs`, `ui_state.rs`, `query_state.rs`
   - Expected result: there is one authoritative projection path between live shell state and persisted tab state.
   - Verification: snapshot/restore/tab switch/reorder tests.
   - Entry condition: B1 で残留責務が整理され、投影対象 state が固定されている。
   - Exit condition: live state と persisted snapshot の projection が単一経路になっている。
3. Phase B3: reducer and command boundary consolidation
   - Files/modules/components: `pipeline.rs`, `result_reducer.rs`, `result_flow.rs`, `preview_flow.rs`, `pipeline_owner.rs`, `filelist.rs`, `update.rs`
   - Expected result: response handling is centralized, predictable, and less coupled to `FlistWalkerApp`.
   - Verification: response lifecycle tests and targeted command boundary tests.
   - Entry condition: B2 の projection が固定され、response apply の対象 owner がぶれない。
   - Exit condition: active/background response handling が reducer boundary に集約されている。
4. Phase B4: routing and lifecycle cleanup
   - Files/modules/components: `search_coordinator.rs`, `index_coordinator.rs`, `worker_bus.rs`, `tabs.rs`
   - Expected result: request routing, in-flight bookkeeping, and tab lifecycle transitions are clear and isolated.
   - Verification: background response routing tests and queue/inflight tests.
   - Entry condition: B3 までで reducer/command boundary が整理され、routing cleanup の対象が確定している。
   - Exit condition: request routing と tab lifecycle の責務が owner ごとに分離され、`tabs.rs` は lifecycle management に限定されている。

## 7. Detailed Task Breakdown
- [ ] entrypoint / owner boundary を明確化する。
- [ ] tab snapshot と live state の projection を一本化する。
- [ ] response apply を reducer boundary に寄せる。
- [ ] request routing と lifecycle を owner ごとに整理する。

## 8. Validation Plan
- Automated tests:
  - `cargo test`
  - `rust/src/app/tests/session_restore.rs`
  - `rust/src/app/tests/session_tabs.rs`
  - `rust/src/app/tests/index_pipeline/*`
  - `rust/src/app/tests/update_commands.rs`
- Manual checks:
  - tab switch / reorder / close
  - restore session and root switch
  - background indexing/search responses landing on the correct tab
- Performance or security checks:
  - frame budget during incremental indexing/search
  - memory growth during tab churn
- Regression focus:
  - stale response rejection
  - snapshot projection mismatch
  - preview/result routing drift

## 9. Rollback Plan
- Keep the old owner path available until the new projection/reducer path is verified.
- If a shell split causes instability, revert only the shell touch points while preserving the core boundary from slice A.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-idealization`, read the roadmap and then slice-a before starting shell work.
- Follow the documented order for shell decomposition and state ownership changes.
- Update the plan before changing order or scope.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12  Planned.
- 2026-04-12  B1 implemented: bootstrap/launch entrypoints were moved behind the bootstrap owner boundary and validated with `cargo test`.
- 2026-04-12  B2 implemented: canonical tab/state projection helpers were centralized in `tab_state.rs` and validated with `cargo test`.
- 2026-04-12  slice B review flagged scope/phase mismatch and missing B1 entry gate; plan updated before implementation.
- 2026-04-12  B3 implemented: response handling was consolidated into reducer boundaries and validated with `cargo test`.
- 2026-04-12  B4 implemented: action/sort request routing was moved out of `tabs.rs` into the worker bus owner boundary and validated with `cargo test`.

## 12. Communication Plan
- Return to user when the slice is reviewed and ready for implementation, or when a blocking issue requires plan update.

## 13. Completion Checklist
- [ ] Plan created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Shell decomposition completed
- [ ] Verification completed
- [ ] Temporary rule removed after completion

## 14. Final Notes
- This slice is the main place where the shell becomes thin; it should not be diluted into cosmetic module moves.
