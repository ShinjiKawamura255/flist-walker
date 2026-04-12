# CHANGE PLAN: Architecture Idealization Roadmap

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Mode: standard
- Execution Mode Policy: Review each slice before execution; keep the roadmap open until the closure slice validates the target architecture. Phase execution should be delegated to subagents by default when implementation starts, while the main agent stays responsible for updating the roadmap, selecting the next slice, and resolving review feedback. If the goal is still unmet after the closure slice, append another slice rather than shrinking the target architecture.
- Parent Plan: none
- Child Plan(s):
  - [docs/CHANGE-PLAN-20260412-slice-a-core-boundary.md](./CHANGE-PLAN-20260412-slice-a-core-boundary.md)
  - [docs/CHANGE-PLAN-20260412-slice-b-shell-decomposition.md](./CHANGE-PLAN-20260412-slice-b-shell-decomposition.md)
  - [docs/CHANGE-PLAN-20260412-slice-c-closure.md](./CHANGE-PLAN-20260412-slice-c-closure.md)
- Scope Label: architecture-idealization
- Related Tickets/Issues: none
- Review Status: レビュー中
- Review Notes:
  - 初回レビューを反映中。
  - High: Slice C に実装 scope が残っていたため、closure slice として不適切だった。
  - Medium: Slice B は実行可能だが、phase の entry/exit が計画上もっと明確であるべきだった。
  - High: slice A は contract stabilization として妥当だが、完了条件を明示化する必要があった。
  - 本計画は理想形を削らずに示すことを優先する。
  - roadmap は standard 運用とし、closure slice で goal 達成と継続可否を判定する。
  - phase 実行は原則 subagent 委譲、main agent は orchestration/review に集中する。

## 1. Background
- 現状の実装は、検索・インデクシング・タブ・更新・プレビューの関心事が `app/` 上位層へ広く集まり、coordinator が state machine と snapshot manager を兼ねている。
- `search` と `indexer` の核は比較的良いが、`tabs.rs`、`pipeline.rs`、`state.rs`、`mod.rs` が大きく、責務境界よりも運用都合で形を保っている。
- この状態では、機能追加のたびに state copy、request routing、response reduction の漏れが起きやすく、拡張コストが雪だるま式に増える。

## 2. Goal
- 理想形は、`search` / `indexer` / `query` / `ui_model` / `path_utils` を pure core として維持し、`app/` は thin shell と explicit reducer/owner 群に分解された構造になること。
- UI thread はイベント受け取りと描画に専念し、重い I/O と検索/インデックス処理はワーカーへ閉じ込める。
- tab snapshot は canonical state から一方向に投影され、手書きの双方向コピーや広範囲な同期ロジックは残さない。
- request_id と command/event によって、古い応答が新しい state を巻き戻さないことを保証する。
- すべての恒久的な設計判断は、最終的に `docs/ARCHITECTURE.md` / `docs/DESIGN.md` / `docs/TESTPLAN.md` にも反映される。
- roadmap の最終 slice は `closure slice` とし、理想形の達成確認と、未達なら追加 slice が必要かどうかの判断を担う。

## 3. Scope
### In Scope
- `rust/src/search/*`、`rust/src/indexer/*`、`rust/src/query.rs`、`rust/src/entry.rs`、`rust/src/ui_model.rs`、`rust/src/path_utils.rs` の core 境界の固定。
- `rust/src/app/mod.rs`、`rust/src/app/state.rs`、`rust/src/app/tab_state.rs`、`rust/src/app/tabs.rs`、`rust/src/app/pipeline.rs`、`rust/src/app/pipeline_owner.rs`、`rust/src/app/result_reducer.rs`、`rust/src/app/result_flow.rs`、`rust/src/app/preview_flow.rs`、`rust/src/app/search_coordinator.rs`、`rust/src/app/index_coordinator.rs`、`rust/src/app/worker_bus.rs`、`rust/src/app/ui_state.rs`、`rust/src/app/query_state.rs` の shell 再編。
- tab/session/feature state の canonical ownership 再定義。
- command/event/reducer 境界の明示化。
- architecture docs と test plan の追随更新。

### Out of Scope
- 機能追加そのもの。
- 検索仕様の後方互換を壊す変更。
- Windows ビルド/配布経路の変更。
- インストーラや配布形態の再設計。

## 4. Constraints and Assumptions
- 検索演算子、FileList 優先、walker fallback、request_id の後方互換は維持する。
- 10 万件候補での応答性は維持または改善する。理想形であってもパフォーマンスを下げない。
- Windows/macOS/Linux の主要 OS を継続サポートする。
- 既存のテスト矩陣を壊さず、必要なら validation を追加してから進める。
- 変更は大規模だが、段階的に切り替え可能である前提で進める。

## 5. Current Risks
- Risk: `AppShellState` / `AppRuntimeState` / `AppTabState` の三層投影が不整合になる。
  - Impact: tab switch、restore、background response routing が壊れる。
  - Mitigation: canonical state を先に固定し、投影は single source of truth からのみ生成する。
- Risk: command/reducer 分離の途中で一時的にコードが肥大化する。
  - Impact: 読みやすさが落ち、レビューが難しくなる。
  - Mitigation: slice ごとに完了条件を狭くせず、各 slice の境界で収束レビューを行う。
- Risk: state copy を減らす過程で UI 応答性が落ちる。
  - Impact: 体感性能の低下。
  - Mitigation: frame budget と incremental 更新を維持し、perf 回帰テストを必須にする。

## 6. Execution Strategy
1. Slice A: Core boundary and contract stabilization
   - Files/modules/components: `search/*`, `indexer/*`, `query.rs`, `entry.rs`, `ui_model.rs`, `path_utils.rs`, `app/worker_protocol.rs`
   - Expected result: domain core が shell 依存なしで成立し、検索/インデックス/表示補助の契約が明示される。
   - Verification: core unit tests, search/indexer regression tests, contract/compatibility assertions.
   - Entry condition: core API 表、pure/public 関数一覧、shell 依存禁止対象が文書化されている。
   - Exit condition: core boundary が実装とテストで一致し、presentation/helper 依存が残っていない。
2. Slice B: Shell decomposition and state ownership
   - Files/modules/components: `app/mod.rs`, `state.rs`, `tab_state.rs`, `tabs.rs`, `pipeline.rs`, `pipeline_owner.rs`, `result_reducer.rs`, `result_flow.rs`, `preview_flow.rs`, `search_coordinator.rs`, `index_coordinator.rs`, `worker_bus.rs`, `ui_state.rs`, `query_state.rs`
   - Expected result: shell が boot / routing / lifecycle / reducer / render の薄い層へ分解され、state ownership が一方向に定まる。
   - Verification: app unit tests, tab/session tests, pipeline tests, snapshot/restore tests.
   - Entry condition: slice A が review 済みで、shell 側から参照してよい core API が固定されている。
   - Exit condition: tab/session projection と command/reducer boundary が一致し、coordinator 直下の責務が entrypoint/dispatch のみに絞られている。
3. Slice C: Closure slice
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
   - Expected result: 理想形が実装・文書・テストで一致しているかを確認し、roadmap を閉じるか継続 slice を定義する。
   - Verification: full validation matrix, targeted perf regression checks, closure review notes.
   - Entry condition: slice A と slice B の実装・検証が完了し、残る論点が goal validation に限定されている。
   - Exit condition: roadmap の goal 達成/未達が明文化され、継続要否が記録されている。

## 7. Detailed Task Breakdown
- [ ] 固定すべき core 契約を列挙し、shell からの依存を排除する。
- [ ] app shell を boot / routing / reducer / render / persistence に分解する。
- [ ] tab snapshot の canonical projection を一本化する。
- [ ] command/event/reducer の責務を切り分け、応答の巻き戻りを防ぐ。
- [ ] closure slice で理想形と実装・文書・テストの整合を確認する。

## 8. Validation Plan
- Automated tests:
  - `cargo test`
  - `cargo test --lib` の app/search/indexer 系回帰
  - 影響がある場合は existing app tests と search tests の targeted run
- Manual checks:
  - tab switch / restore / background response / update prompt の一連操作
  - root change と FileList flow の整合確認
- Performance or security checks:
  - 100k candidates の search latency 回帰確認
  - 初期インデクシングと incremental update の frame budget 確認
- Regression focus:
  - stale response rejection
  - tab snapshot sync mismatch
  - active/background routing drift
  - UI freeze during indexing/search

## 9. Rollback Plan
- Slice 単位で戻せるようにする。core boundary、shell decomposition、closure の各 slice は独立した検証/rollback 境界を持つ。
- `state` と `tabs` の投影見直しは、canonical state を保持したまま投影層だけを戻せる形にする。
- データ移行はないため、ロールバックはコードと docs の巻き戻しで足りる。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-idealization`, read the relevant change plan documents before starting implementation.
- Read them in this order:
  - `docs/CHANGE-PLAN-20260412-roadmap-architecture-idealization.md`
  - `docs/CHANGE-PLAN-20260412-slice-a-core-boundary.md`
  - `docs/CHANGE-PLAN-20260412-slice-b-shell-decomposition.md`
  - `docs/CHANGE-PLAN-20260412-slice-c-closure.md`
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Phase execution is delegated to subagents by default; the main agent acts as orchestrator and reviewer.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Keep the roadmap open until the closure slice has been completed and the goal-validation result has been recorded.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12  Planned.
- 2026-04-12  Slice A completed: core boundary extraction and contract tightening landed, verified by `cargo test`.

## 12. Communication Plan
- Return to user when:
  - plan creation and review are complete
  - all phases are complete
  - a phase cannot continue without resolving a blocking problem
- If the project is under git control, commit when a completed phase forms an independent verification/rollback unit.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] If the project is under git control, each commit corresponds to an independent verification/rollback unit, and grouped phases are documented in the plan
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This roadmap intentionally keeps the ideal architecture intact instead of trimming it to what is easy today.
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.
