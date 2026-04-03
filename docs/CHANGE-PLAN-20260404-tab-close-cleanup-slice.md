# CHANGE PLAN: Tab Close Cleanup Slice

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: tab-close-cleanup
- Related Tickets/Issues: God Object follow-up

## 1. Background
- `FileList`、`Update`、`root change`、shared tab lifecycle、tab activation/background restore の各 slice により、`FlistWalkerApp` の横断責務は段階的に縮小した。
- それでも `close_tab_index()` には、tab removal そのものに加えて filelist pending dialog cleanup、index/search request routing cleanup、preview/action/sort request routing cleanup、memory sample invalidation がまとまって残っている。
- この cleanup 群は [tabs.rs](/mnt/d/work/flistwalker/rust/src/app/tabs.rs) に集中している一方、所有者は `FileListManager`、`IndexCoordinator`、`SearchCoordinator`、`RequestTabRoutingState` に分散している。
- 次の slice は、`close_tab_index()` の close-specific cleanup だけを分離し、active tab fallback / activation は既存 lifecycle helper に残す。

## 2. Why This Slice Next
- `tab activation / background restore` まで切り出した結果、`tabs.rs` に残る最も大きい横断責務が close-specific cleanup になった。
- `close_tab_index()` は複数 subsystem の request/pending state を直接 `retain/remove` しており、責務境界が見えにくい。
- この cleanup を dedicated helper / command 境界へ寄せることで、次段の tab reorder や request routing 局所化に進みやすくなる。
- 一方で active tab fallback、tab apply、focus 復元まで同時に動かすと scope が広がるため、この計画では除外する。

## 3. Goal
- `close_tab_index()` の close-specific cleanup を専用 slice として分離する。
- `FlistWalkerApp` は tab removal と fallback active tab 決定を担当し、close 時の subsystem cleanup は dedicated helper / command 境界へ寄せる。
- close された tab に紐づく stale request / pending state / cached routing が残らない契約を維持する。

## 4. Scope
### In Scope
- close 時の filelist pending cleanup
- close 時の index/search request routing cleanup
- close 時の preview/action/sort routing cleanup
- close 後の memory sample invalidation
- `DESIGN.md` / `TESTPLAN.md` への ownership boundary と phase validation 追記

### Out of Scope
- tab removal 後の active tab fallback / activation 順序そのもの
- `move_tab()` の reorder lifecycle
- `pipeline.rs` 全面の queue/inflight 再設計
- root change / updater / filelist / restore-refresh の仕様変更

## 5. Candidate Ownership Boundary
- `TabCloseCleanupCommand` / helper が扱うもの:
  - close 対象 tab id に紐づく filelist pending state cleanup
  - close 対象 tab id に紐づく request routing / queue / background state cleanup
  - close 後の memory sample invalidation command
- `FlistWalkerApp` 側に残すもの:
  - last-tab guard
  - active tab state sync
  - `Vec<AppTabState>` からの remove
  - fallback active tab index の決定
  - fallback tab の activation
- 各 subsystem 側に残すもの:
  - `FileListManager`: pending dialog / deferred filelist state の所有
  - `IndexCoordinator`: request_tabs / pending_queue / latest_request_ids / background_states の所有
  - `SearchCoordinator`: search request_tabs の所有
  - `RequestTabRoutingState`: preview/action/sort request routing の所有
- 境界ルール:
  - helper は close 対象 tab id を入力に cleanup command を返し、tab fallback logic は持たない
  - request_id の allocate / queue dispatch / worker 応答処理は既存 coordinator / pipeline に残す
  - `move_tab()` はこの slice の対象外とし、close-specific cleanup と混ぜない

## 6. Risks
- Risk: close 後に stale request routing が残り、background response が存在しない tab へ飛ぶ。
  - Impact: 高
  - Mitigation: close 後の routing cleanup を targeted regression に固定する。
- Risk: `pending_queue` / `background_states` cleanup を誤ると、別 tab の background indexing を巻き込む。
  - Impact: 高
  - Mitigation: close 対象 tab id だけを除去する boundary を明示し、background indexing 回帰を gate に入れる。
- Risk: cleanup helper が fallback active tab logic まで抱え込むと、新しい God Object になる。
  - Impact: 中
  - Mitigation: helper は cleanup 専用とし、activation は既存 lifecycle helper に残す。

## 7. Execution Strategy
1. Phase 1: docs と close cleanup command scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/tabs.rs`
   - Action: `TabCloseCleanupCommand` 群と ownership boundary を定義する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
2. Phase 2: close-specific cleanup helper 抽出
   - Files: `rust/src/app/tabs.rs`, 必要なら `rust/src/app/state.rs`
   - Action: `close_tab_index()` 内の cleanup 群を helper / command に寄せる。
   - Verification: `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test app_core -- --nocapture`
3. Phase 3: command dispatch 結線と targeted regression 固定
   - Files: `rust/src/app/tabs.rs`, 必要なら `rust/src/app/tests/session_tabs.rs`, `rust/src/app/tests/app_core.rs`
   - Action: close cleanup command dispatch を 1 箇所へ寄せ、routing / pending cleanup と late-response hazard の回帰を固定する。
   - Verification: `cd rust && cargo test ctrl_w_closes_current_tab_and_keeps_last_tab -- --nocapture`; `cd rust && cargo test close_tab_invalidates_memory_cache_for_immediate_resample -- --nocapture`; `cd rust && cargo test close_tab_clears_filelist_and_request_routing_for_removed_tab -- --nocapture`; `cd rust && cargo test close_tab_ignores_late_background_responses_for_removed_tab -- --nocapture`; `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test app_core -- --nocapture`; `cd rust && cargo test`

## 8. Detailed Tasks
- [ ] `DESIGN.md` に tab close cleanup slice の ownership boundary を追加
- [ ] `TESTPLAN.md` に phase validation を追加
- [ ] `TabCloseCleanupCommand` 群を定義
- [ ] filelist pending cleanup を helper / command 経由へ整理する
- [ ] request routing / queue cleanup を helper / command 経由へ整理する
- [ ] removed tab 向け routing/pending state が close 後に残らない targeted regression を追加する
- [ ] removed tab 向け late background response が無視される targeted regression を追加する
- [ ] close-specific cleanup と fallback activation が混ざっていないことを確認する

## 9. Implementation Checkpoints
- Checkpoint 1: last-tab guard と fallback active tab 決定は `close_tab_index()` に残す。
- Checkpoint 2: `FileListManager` / `IndexCoordinator` / `SearchCoordinator` / `RequestTabRoutingState` の ownership 自体は動かさない。
- Checkpoint 3: `pending_queue` / `background_states` cleanup は close 対象 tab の request だけを除去する。
- Checkpoint 4: `move_tab()` にはこの helper を流用しない。
- Checkpoint 5: memory sample invalidation は close cleanup に含めるが、UI focus 復元は含めない。

## 10. Validation Plan
- Automated:
  - `cd rust && cargo check`
  - `cd rust && cargo test ctrl_w_closes_current_tab_and_keeps_last_tab -- --nocapture`
  - `cd rust && cargo test close_tab_invalidates_memory_cache_for_immediate_resample -- --nocapture`
  - `cd rust && cargo test close_tab_clears_filelist_and_request_routing_for_removed_tab -- --nocapture`
  - `cd rust && cargo test close_tab_ignores_late_background_responses_for_removed_tab -- --nocapture`
  - `cd rust && cargo test session_tabs -- --nocapture`
  - `cd rust && cargo test app_core -- --nocapture`
  - `cd rust && cargo test`
- Manual:
  - background request を持つ tab を閉じたあと、別 tab の search/preview/indexing が継続することを確認
  - close 後に preview/list が stale tab 由来へ巻き戻らないことを確認
- Regression focus:
  - last-tab guard を壊さない
  - close 後の routing / pending cleanup
  - close 後に遅れて届く background response が removed tab へ適用されない
  - fallback active tab の state 適用

## 11. Rollback Plan
- Phase ごとの小コミットを維持し、必要なら `git revert` で戻す。
- 部分失敗時は対象ファイル単位の `git restore <file>` を使い、他差分を巻き込まない。

## 12. Exit Criteria
- `close_tab_index()` の close-specific cleanup と fallback activation の責務境界がコードと docs で明確になっている。
- close 後の routing / pending cleanup 回帰と `session_tabs`, `app_core`, `cargo test` が green である。
- 後続で request routing の所有を再編しなくても、tab close seam を独立に扱える状態になっている。

## 13. Temporary Rule Draft
- For `tab-close-cleanup`, read `docs/CHANGE-PLAN-20260404-tab-close-cleanup-slice.md` before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove the temporary rule from `AGENTS.md` and delete this change plan after the work is complete.
