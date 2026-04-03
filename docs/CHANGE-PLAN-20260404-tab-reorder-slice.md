# CHANGE PLAN: Tab Reorder Slice

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: tab-reorder
- Related Tickets/Issues: God Object follow-up

## 1. Background
- `FileList`、`Update`、`root change`、shared tab lifecycle、tab activation/background restore、tab close cleanup の各 slice により、`FlistWalkerApp` の横断責務は段階的に縮小した。
- それでも `move_tab()` には、tab drag state cleanup、active tab state sync、active tab identity の再解決、fallback apply の順序がまとまって残っている。
- 現在の設計では、tab drag UI 自体は `render.rs` が扱い、実際の reorder は [tabs.rs](/mnt/d/work/flistwalker/rust/src/app/tabs.rs#L742) の `move_tab(from, to)` に集約されている。
- 次の slice は、この reorder seam だけを切り出し、close cleanup や request routing 所有の再設計には踏み込まない。

## 2. Why This Slice Next
- `tabs.rs` に残る tab lifecycle 系の責務は、`move_tab()` の reorder 順序制御が中心になった。
- `move_tab()` は `sync_active_tab_state()`、`tab.id` ベースの active tab 再解決、`apply_tab_state()` を open-code しており、shared lifecycle helper と同じ種類の「順序制御」がまだ残っている。
- この seam を dedicated helper / command 境界へ寄せることで、tab lifecycle 系の分解をほぼ閉じられる。
- 一方で drag pointer hit-test や `render_tab_bar` の UI イベント全体まで同時に扱うと scope が広がるため、この計画では除外する。

## 3. Goal
- `move_tab()` の reorder-specific state transition を専用 slice として分離する。
- `FlistWalkerApp` は reorder request の入口と `Vec<AppTabState>` 更新を担当し、drag-state cleanup、active tab identity 再解決、reordered active tab apply は dedicated helper / command 境界へ寄せる。
- reorder 後に active tab の root/query/filter/result/focus state を取り違えない契約を維持する。

## 4. Scope
### In Scope
- `move_tab()` の drag-state cleanup
- reorder 前 `sync_active_tab_state()` の順序固定
- active tab id ベースの再解決
- reorder 後の active tab apply
- `DESIGN.md` / `TESTPLAN.md` への ownership boundary と phase validation 追記

### Out of Scope
- `render.rs` の drag gesture / hit-test / pointer tracking 全体
- `close_tab_index()` の cleanup
- request routing state の所有再編
- tab activation/background restore の仕様変更

## 5. Candidate Ownership Boundary
- `TabReorderCommand` / helper が扱うもの:
  - reorder 前の drag-state cleanup
  - active tab id の capture と reordered index の再解決
  - reorder 後の active tab apply command
- `FlistWalkerApp` 側に残すもの:
  - invalid/no-op guard
  - `sync_active_tab_state()` の呼び出し入口
  - `Vec<AppTabState>` からの remove/insert
  - `move_tab()` の public entry point
- `render.rs` 側に残すもの:
  - drag gesture 検知
  - `tab_drop_index()` / `update_tab_drag_state()` / `reorder_tab` 決定
- 境界ルール:
  - helper は reorder-specific state transition だけを扱い、drag pointer geometry は持たない
  - request routing / close cleanup / activation-refresh は既存 slice に残す
  - `move_tab()` と `close_tab_index()` の helper は混ぜない

## 6. Risks
- Risk: active tab id の再解決を誤ると、reorder 後に別 tab の state が active として適用される。
  - Impact: 高
  - Mitigation: existing `move_tab_*` regression を phase gate に固定する。
- Risk: drag-state cleanup を helper 化する過程で render 側との責務が混線する。
  - Impact: 中
  - Mitigation: `render.rs` の gesture/hit-test は scope 外と明記し、`move_tab()` の reorder seam だけを扱う。
- Risk: reorder helper に close cleanup や activation-refresh を持ち込むと、新しい God Object になる。
  - Impact: 中
  - Mitigation: helper は reorder-specific state transition に限定する。

## 7. Execution Strategy
1. Phase 1: docs と reorder command scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/tabs.rs`
   - Action: `TabReorderCommand` 群と ownership boundary を定義する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
2. Phase 2: reorder helper 抽出
   - Files: `rust/src/app/tabs.rs`
   - Action: `move_tab()` の drag-state cleanup、active tab id 再解決、apply を helper / command に寄せる。
   - Verification: `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test app_core -- --nocapture`
3. Phase 3: command dispatch 結線と targeted regression 固定
   - Files: `rust/src/app/tabs.rs`, 必要なら `rust/src/app/tests/session_tabs.rs`
   - Action: reorder command dispatch を 1 箇所へ寄せ、active identity / state carryover / drag-state / no-op 回帰を固定する。
   - Verification: `cd rust && cargo test move_tab_reorders_tabs_and_preserves_active_tab_identity -- --nocapture`; `cd rust && cargo test move_tab_updates_active_index_when_other_tab_crosses_it -- --nocapture`; `cd rust && cargo test move_tab_preserves_per_tab_state_carryover_after_reorder -- --nocapture`; `cd rust && cargo test move_tab_clears_drag_state_on_direct_reorder -- --nocapture`; `cd rust && cargo test move_tab_ignores_invalid_or_noop_indices -- --nocapture`; `cd rust && cargo test tab_drag_above_threshold_reorders_on_release -- --nocapture`; `cd rust && cargo test tab_drag_below_threshold_does_not_reorder_on_release -- --nocapture`; `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test`

## 8. Detailed Tasks
- [ ] `DESIGN.md` に tab reorder slice の ownership boundary を追加
- [ ] `TESTPLAN.md` に phase validation を追加
- [ ] `TabReorderCommand` 群を定義
- [ ] reorder-specific helper を切り出す
- [ ] active tab id の再解決を helper / command 経由へ整理する
- [ ] reorder 後の per-tab state carryover を直接 assert する targeted regression を追加する
- [ ] direct `move_tab()` path で drag-state が残らない targeted regression を追加する
- [ ] reorder 後 apply と drag-state cleanup が既存 lifecycle / close cleanup と混ざっていないことを確認する

## 9. Implementation Checkpoints
- Checkpoint 1: `render.rs` の drag gesture / `tab_drop_index()` ロジックは scope 外に保つ。
- Checkpoint 2: `move_tab()` の invalid/no-op guard は entry point に残す。
- Checkpoint 3: active tab identity は index ではなく `tab.id` ベースで再解決する。
- Checkpoint 4: reorder helper は close cleanup や activation-refresh を持たない。
- Checkpoint 5: reorder 後の `apply_tab_state()` は active tab だけに限定する。

## 10. Validation Plan
- Automated:
  - `cd rust && cargo check`
  - `cd rust && cargo test move_tab_reorders_tabs_and_preserves_active_tab_identity -- --nocapture`
  - `cd rust && cargo test move_tab_updates_active_index_when_other_tab_crosses_it -- --nocapture`
  - `cd rust && cargo test move_tab_preserves_per_tab_state_carryover_after_reorder -- --nocapture`
  - `cd rust && cargo test move_tab_clears_drag_state_on_direct_reorder -- --nocapture`
  - `cd rust && cargo test move_tab_ignores_invalid_or_noop_indices -- --nocapture`
  - `cd rust && cargo test tab_drag_above_threshold_reorders_on_release -- --nocapture`
  - `cd rust && cargo test tab_drag_below_threshold_does_not_reorder_on_release -- --nocapture`
  - `cd rust && cargo test session_tabs -- --nocapture`
  - `cd rust && cargo test`
- Manual:
  - drag reorder 後に active tab の root/query が保たれることを確認
  - no-op drag release で tab order が変わらないことを確認
- Regression focus:
  - active tab identity の保持
  - reorder 後の query/result/focus など per-tab state carryover
  - drag-state cleanup
  - invalid/no-op reorder の no-op 保持

## 11. Rollback Plan
- Phase ごとの小コミットを維持し、必要なら `git revert` で戻す。
- 部分失敗時は対象ファイル単位の `git restore <file>` を使い、他差分を巻き込まない。

## 12. Exit Criteria
- `move_tab()` の reorder-specific state transition と render gesture handling の責務境界がコードと docs で明確になっている。
- move/reorder 回帰、`session_tabs`、`cargo test` が green である。
- 後続で request routing 所有の再編をしなくても、tab reorder seam を独立に扱える状態になっている。

## 13. Temporary Rule Draft
- For `tab-reorder`, read `docs/CHANGE-PLAN-20260404-tab-reorder-slice.md` before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove the temporary rule from `AGENTS.md` and delete this change plan after the work is complete.
