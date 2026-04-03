# CHANGE PLAN: Tab Activation and Background Restore Slice

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: tab-activation-background-restore
- Related Tickets/Issues: God Object follow-up

## 1. Background
- `FileList`、`Update`、`root change`、shared tab lifecycle helper の各 slice により、`FlistWalkerApp` の横断責務は段階的に縮小した。
- それでも `pending_restore_refresh`、`trigger_restore_refresh_for_active_tab()`、restored background tab の lazy refresh、background tab の search/preview/index 応答保持は、`tabs.rs` と `pipeline.rs` に分散したまま残っている。
- 現在の設計では、active tab は即時 refresh、background tab は `pending_restore_refresh` を保持して初回 activation 時に lazy refresh する契約がある。これは [DESIGN.md](/mnt/d/work/flistwalker/docs/DESIGN.md#L162) と [session_tabs.rs](/mnt/d/work/flistwalker/rust/src/app/tests/session_tabs.rs#L282) に現れている。
- 次の slice は、この `tab activation / background restore` 契約だけを切り出し、request routing 全面や close-specific cleanup には踏み込まない。

## 2. Why This Slice Next
- shared lifecycle helper 抽出により、tab switch/create の順序制御は整理された。
- その上に残っている未分離の主要責務が、activation 時の restore refresh 判断と background tab 状態保持である。
- `pending_restore_refresh` は active/background の切替 seam であり、ここを切ることで `tabs.rs` と `pipeline.rs` の責務境界をさらに明確にできる。
- 一方で `close_tab_index` の request routing cleanup や pipeline 全面再設計まで同時に触ると scope が広がりすぎるため、この計画では除外する。

## 3. Goal
- `pending_restore_refresh` と activation 時の lazy refresh 判断を専用 slice として分離する。
- `FlistWalkerApp` は高レベルな activation intent と command dispatch を担当し、restore decision と state transition は dedicated helper / command 境界へ寄せる。
- background tab の search/preview/index 応答保持そのものは既存 `pipeline.rs` / tab snapshot に残し、active/background の entries 混線防止と activation 時 lazy refresh 契約を維持する。

## 4. Scope
### In Scope
- `pending_restore_refresh` の active/background 契約整理
- `trigger_restore_refresh_for_active_tab()` の command 境界化
- restored background tab activation 時の lazy refresh 判断と起動
- activation seam で必要な state transition 整理
- `DESIGN.md` / `TESTPLAN.md` への ownership boundary と phase validation 追記

### Out of Scope
- background tab の search/preview/index 応答保持ロジック本体の再設計
- `close_tab_index` の request routing cleanup 全体
- index/search pipeline の queue/inflight 全面再設計
- tab reorder / drag-and-drop
- root change / updater / filelist の仕様変更

## 5. Candidate Ownership Boundary
- `TabRestoreDecision` helper / command boundary が扱うもの:
  - active tab activation 時に lazy refresh が必要かどうかの判断
  - `pending_restore_refresh` の activation seam での立て下ろし
  - restore-related high-level command emission
- `FlistWalkerApp` 側に残すもの:
  - egui event 受付
  - helper が返した command の最終 dispatch
  - notice / focus / persistence 実行
- `pipeline.rs` 側に残すもの:
  - index request allocate と queue/inflight bookkeeping
  - 実際の `request_index_refresh()` / `request_background_index_refresh_for_tab()` 実行
  - background tab の search/preview/index 応答保持ロジック本体
- 境界ルール:
  - helper は restore-refresh の要否と state transition だけを扱い、request map や queue 本体は持たない
  - response retention の実体は既存 `pipeline.rs` / tab snapshot に残し、この slice では ownership を動かさない
  - `switch_to_tab_index()` / `create_new_tab()` の activation seam を優先対象とし、`move_tab()` / `close_tab_index()` の固有 cleanup はこの slice に持ち込まない

## 6. Risks
- Risk: `pending_restore_refresh` の立て下ろしを誤ると、background tab の初回 activation で refresh が起きない、または二重 refresh になる。
  - Impact: 高
  - Mitigation: `switching_to_restored_background_tab_triggers_lazy_refresh` を phase gate に固定する。
- Risk: background tab の search/preview/index 応答を active tab へ誤適用すると、一覧や preview が混線する。
  - Impact: 高
  - Mitigation: background retention 系の既存回帰テストを明示 gate に入れる。
- Risk: restore-refresh を helper に寄せる過程で `tabs.rs` の focus/lifecycle helper と再び責務が混線する。
  - Impact: 中
  - Mitigation: lifecycle 順序制御と restore-refresh 判断を別 helper / command として分ける。

## 7. Execution Strategy
1. Phase 1: docs と restore command scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/tabs.rs` または `rust/src/app/pipeline.rs`
   - Action: `TabRestoreCommand` 群と ownership boundary を定義する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
2. Phase 2: activation / lazy refresh decision 抽出
   - Files: `rust/src/app/tabs.rs`, `rust/src/app/pipeline.rs`
   - Action: `pending_restore_refresh` と activation 時 restore-refresh 判断を helper / command に寄せる。
   - Verification: `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test app_core -- --nocapture`
3. Phase 3: command dispatch 結線と targeted regression 固定
   - Files: `rust/src/app/tabs.rs`, `rust/src/app/pipeline.rs`, 必要なら `rust/src/app/tests/session_tabs.rs`
   - Action: restore-related command dispatch を 1 箇所へ寄せ、activation seam と background retention の複合回帰を固定する。
   - Verification: `cd rust && cargo test switching_to_restored_background_tab_triggers_lazy_refresh -- --nocapture`; `cd rust && cargo test background_tab_search_and_preview_responses_are_retained -- --nocapture`; `cd rust && cargo test background_tab_index_batches_do_not_override_active_tab_entries -- --nocapture`; `cd rust && cargo test background_tab_activation_consumes_pending_restore_refresh_once -- --nocapture`; `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test`

## 8. Detailed Tasks
- [ ] `DESIGN.md` に tab activation / background restore slice の ownership boundary を追加
- [ ] `TESTPLAN.md` に phase validation を追加
- [ ] `TabRestoreCommand` 群を定義
- [ ] activation 時の restore-refresh 判断 helper を切り出す
- [ ] `pending_restore_refresh` の立て下ろしを helper / command 経由へ整理する
- [ ] background tab retention の既存契約が崩れていないことを確認する
- [ ] activation seam と background retention をまたぐ複合回帰を 1 本追加する

## 9. Implementation Checkpoints
- Checkpoint 1: `pending_restore_refresh` の設計変更は行わず、まず ownership と command 境界だけを変える。
- Checkpoint 2: `trigger_restore_refresh_for_active_tab()` の実際の refresh 実行は pipeline 側に残す。
- Checkpoint 3: background tab の search/preview/index 応答保持ロジック本体は pipeline 側に残す。
- Checkpoint 4: shared lifecycle helper と restore-refresh helper を混ぜない。
- Checkpoint 5: `close_tab_index` の request routing cleanup はこの slice に持ち込まない。
- Checkpoint 6: `switch_to_tab_index()` / `create_new_tab()` の activation seam を優先し、`move_tab()` / `close_tab_index()` の固有分岐は別 slice に残す。

## 10. Validation Plan
- Automated:
  - `cd rust && cargo check`
  - `cd rust && cargo test switching_to_restored_background_tab_triggers_lazy_refresh -- --nocapture`
  - `cd rust && cargo test background_tab_search_and_preview_responses_are_retained -- --nocapture`
  - `cd rust && cargo test background_tab_index_batches_do_not_override_active_tab_entries -- --nocapture`
  - `cd rust && cargo test background_tab_activation_consumes_pending_restore_refresh_once -- --nocapture`
  - `cd rust && cargo test session_tabs -- --nocapture`
  - `cd rust && cargo test app_core -- --nocapture`
  - `cd rust && cargo test`
- Manual:
  - restored background tab へ初回 switch して lazy refresh を確認
  - background tab を保持したまま active tab を連続切替し、preview/list の混線がないことを確認
- Regression focus:
  - background tab activation の lazy refresh
  - background tab search/preview retention
  - background index batches が active tab entries を上書きしない
  - retained search/preview/index state を持つ background tab を activation したときの複合 seam

## 11. Rollback Plan
- Phase ごとの小コミットを維持し、必要なら `git revert` で戻す。
- 部分失敗時は対象ファイル単位の `git restore <file>` を使い、他差分を巻き込まない。

## 12. Exit Criteria
- `pending_restore_refresh` と activation 時 restore-refresh 判断の責務境界がコードと docs で明確になっている。
- background retention / lazy refresh の targeted regression と `session_tabs`, `app_core`, `cargo test` が green である。
- background response retention の ownership を動かさずに、tab activation seam を独立に扱える状態になっている。

## 13. Temporary Rule Draft
- For `tab-activation-background-restore`, read `docs/CHANGE-PLAN-20260404-tab-activation-background-restore.md` before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove the temporary rule from `AGENTS.md` and delete this change plan after the work is complete.
