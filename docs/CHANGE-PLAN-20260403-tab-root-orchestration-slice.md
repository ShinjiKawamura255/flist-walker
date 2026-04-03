# CHANGE PLAN: Root Change Orchestration Slice

## Metadata
- Date: 2026-04-03
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: root-change-orchestration-slice
- Related Tickets/Issues: God Object follow-up

## 1. Background
- `FileList` と `Update` はそれぞれ `Manager + command dispatch` の縦スライスとして `FlistWalkerApp` から分離できた。
- 一方で `FlistWalkerApp` には、root 変更に伴う state reset、stale cleanup、reindex request の調停がまだ残っている。
- この root change 責務は `mod.rs` の root switch、`pipeline.rs` の index/search refresh、`tabs.rs` の current tab sync と密結合している。
- サブエージェントレビューの結果、「tab activation / root switch / background restore」を 1 本にまとめると新しい God Object を作る危険が高いと判断した。
- そのため次の slice は `root change` 単独に絞り、background restore と tab lifecycle 共通化は後続計画へ分離する。

## 2. Why This Slice Next
- `FlistWalkerApp` の横断責務のうち、次に大きい塊が tab/root orchestration である。
- ただし `switch_to_tab_index` は `create_new_tab` / `close_tab_index` と同じ lifecycle ハブを共有しており、tab activation だけを安全に切り出すには先に別の helper 抽出が必要である。
- root change は `apply_root_change` を中心に、selection/pinned/preview cleanup、stale filelist confirmation cleanup、reindex request をひとかたまりの縦フローとして持っている。
- そのため最初の slice は `root change` 単独の方が、実装コストと安全性のバランスがよい。

## 3. Goal
- `root switch` を `FlistWalkerApp` 直書きから切り出し、専用 orchestrator が command を返す構造へ寄せる。
- `FlistWalkerApp` は egui 入力、worker bus dispatch、notice / persistence / viewport close など app-level 実行に限定し、feature state transition の直接操作を減らす。
- root change に伴う stale state 掃除、tab ごとの root 同期、reindex request 契約を維持する。

## 4. Scope
### In Scope
- root 変更時の state reset、stale selection/pinned/preview の掃除、reindex request
- root dropdown / browse dialog から root change transition へ入る経路の整理
- root change 起点の stale filelist cleanup と tab state sync
- 上記に必要な command 型と orchestrator 導入
- `DESIGN.md` / `TESTPLAN.md` への ownership boundary と検証観点の追記

### Out of Scope
- active tab switch / background restore refresh
- tab reorder / drag-and-drop / close / new tab creation 全体の再設計
- root dropdown / browse dialog の UI レイアウト変更
- indexer / searcher / updater 本体仕様の変更
- tab accent, preview 表示, sort UI の機能追加

## 5. Candidate Ownership Boundary
- `RootChangeOrchestrator` が所有するもの:
  - root change 時の reset 対象判定
  - root change 起点で必要な stale cleanup 判定
  - reindex と notice の高レベル command emission
- `FlistWalkerApp` 側に残すもの:
  - egui event 受付、dialog open/close、focus 切替
  - worker bus への最終 dispatch
  - `mark_ui_state_dirty()` / `persist_ui_state_now()` / notice 更新
  - saved roots / session 永続化の実行
- `pipeline.rs` / coordinator との境界:
  - request_id allocate と queue/inflight bookkeeping は既存 coordinator に残す
  - orchestrator は `request_index_refresh` 相当の高レベル意図だけを返し、実際の queue/inflight 更新は `pipeline.rs` が行う
- `tabs.rs` との境界:
  - active tab capture/apply/restore は触らない
  - root change 後に現在 tab へ同期する必要がある state だけを扱う

## 6. Risks
- Risk: root change で旧 root 由来の selection/pinned/preview/filelist confirmation が残ると、状態混線が起きる。
  - Impact: 高
  - Mitigation: `apply_root_change` が現在担っている cleanup 群を同じ単位で移し、既存 root change 回帰を phase gate に入れる。
- Risk: orchestrator が index/search coordinator の内部状態まで抱え込むと、第二の God Object になる。
  - Impact: 高
  - Mitigation: coordinator の queue/request map は残し、判断と command emission だけを移す。
- Risk: root dropdown / browse dialog からの経路だけ先に変えて、他の root change call site が旧実装に残ると二重経路になる。
  - Impact: 中
  - Mitigation: root change entry point を `apply_root_change` 系 1 箇所へ寄せてから orchestrator 化する。

## 7. Execution Strategy
1. Phase 1: docs と orchestration command scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/tabs.rs` または `rust/src/app/state.rs`
   - Action: `RootChangeCommand` 群と ownership boundary を先に定義する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
2. Phase 2: root change orchestration の抽出
   - Files: `rust/src/app/mod.rs`, `rust/src/app/pipeline.rs`, `rust/src/app/tabs.rs`
   - Action: root change の cleanup / tab sync / reindex request を orchestrator へ寄せる。
   - Verification: `cd rust && cargo test app_core -- --nocapture`; `cd rust && cargo test query_history -- --nocapture`
3. Phase 3: command dispatch 結線と root-change regression 固定
   - Files: `rust/src/app/mod.rs`, `rust/src/app/pipeline.rs`, `rust/src/app/tests/index_pipeline/search_filelist.rs`
   - Action: root change entry point を command dispatch へ寄せ、root change 回帰点の test を明示固定する。
   - Verification: `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test root_change_cancels_pending_filelist_overwrite_confirmation -- --nocapture`; `cd rust && cargo test deferred_filelist_is_canceled_when_root_changes -- --nocapture`; `cd rust && cargo test root_change_clears_stale_selection_state -- --nocapture`; `cd rust && cargo test`

## 8. Detailed Tasks
- [ ] `DESIGN.md` に Root change orchestration slice の ownership boundary を追加
- [ ] `TESTPLAN.md` に phase ごとの validation を追加
- [ ] `RootChangeCommand` 群を定義
- [ ] root change の reset/reindex/notice を orchestrator 経由へ寄せる
- [ ] stale filelist cleanup と current tab sync を root change slice に含める
- [ ] root change の既存 call site を 1 つの entry point に寄せる
- [ ] root change 関連の既存契約が崩れていないことを確認する

## 9. Implementation Checkpoints
- Checkpoint 1: `IndexCoordinator` / `SearchCoordinator` の request map 自体は移さず、orchestrator は判断だけ行う。
- Checkpoint 2: root change 時の `pinned_paths`, `current_row`, `preview`, preview request state の掃除契約を維持する。
- Checkpoint 3: root change 後も `sync_active_tab_state()` による current tab root 同期が失われない。
- Checkpoint 4: root change orchestrator は `request_index_refresh` の高レベル意図までに留め、queue/inflight 操作へ直接触れない。
- Checkpoint 5: stale filelist confirmation cleanup は root switch slice の中で回帰させない。
- Checkpoint 6: root dropdown / browse dialog は UI trigger のみを持ち、state transition 本体は orchestrator に寄せる。
- Checkpoint 7: query history navigation reset と history search reset が root change 後も維持される。

## 10. Validation Plan
- Automated:
  - `cd rust && cargo check`
  - `cd rust && cargo test app_core -- --nocapture`
  - `cd rust && cargo test query_history -- --nocapture`
  - `cd rust && cargo test session_tabs -- --nocapture`
  - `cd rust && cargo test root_change_cancels_pending_filelist_overwrite_confirmation -- --nocapture`
  - `cd rust && cargo test deferred_filelist_is_canceled_when_root_changes -- --nocapture`
  - `cd rust && cargo test root_change_clears_stale_selection_state -- --nocapture`
  - `cd rust && cargo test`
  - `cd rust && cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`
  - `cd rust && cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
- Manual:
  - root dropdown から root 切替
  - browse dialog から root 切替
  - root change 後に旧 root の preview / pinned / current row が残らないことを確認
- Regression focus:
  - root change 後に旧 root の current/pinned/preview が残らない
  - root change 後に stale filelist confirmation が残らない
  - root change 後も query/history reset と current tab sync が崩れない
  - session restore と tab switch の既存契約を巻き込んで壊していない

## 11. Rollback Plan
- Phase ごとの小コミットを維持し、必要なら `git revert` で戻す。
- 部分失敗時は対象ファイル単位の `git restore <file>` を使い、他の未完了差分を巻き込まない。

## 12. Exit Criteria
- `FlistWalkerApp` から tab/root 切替の直接 state transition が減っている。
- `app_core`, `query_history`, `session_tabs`, `index_pipeline::search_filelist`, `cargo test` が green である。
- indexing/search orchestration の perf regression テストを含めて、既存の UI responsiveness 契約を壊していない。
- docs が新しい ownership boundary と validation 方針を説明できる状態になっている。

## 13. Temporary Rule Draft
- For `root-change-orchestration-slice`, read `docs/CHANGE-PLAN-20260403-tab-root-orchestration-slice.md` before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove the temporary rule from `AGENTS.md` and delete this change plan after the work is complete.
