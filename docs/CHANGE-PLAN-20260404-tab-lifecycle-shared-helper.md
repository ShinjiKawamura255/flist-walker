# CHANGE PLAN: Tab Lifecycle Shared Helper Extraction

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: tab-lifecycle-shared-helper
- Related Tickets/Issues: God Object follow-up

## 1. Background
- `FileList`、`Update`、`root change` はそれぞれ縦スライスとして `FlistWalkerApp` から一段切り離した。
- 次の候補は `tab activation / background restore` だが、既存の `tabs.rs` では `switch_to_tab_index` が `sync_active_tab_state`、`compact_inactive_tab_state`、`apply_tab_state`、`restore_results_from_compacted_tab`、`trigger_restore_refresh_for_active_tab` を束ねる lifecycle ハブになっている。
- 同じ経路は `create_new_tab` と `close_tab_index` でも部分的に再利用されており、tab activation だけを先に分離しようとすると、out-of-scope の tab lifecycle 変更を巻き込むか、ロジック重複を増やす危険がある。
- 初回レビューでは、`close_tab_index` を同じ helper に含めると request routing cleanup と active-tab fallback まで抱え込み、helper が広がりすぎるという指摘が出た。
- そのため次の段階では、`tabs.rs` の shared lifecycle transition のうち「active tab 離脱前処理」と「target tab 入場処理」だけを helper と command 境界へ整理し、その後に `tab activation / background restore` slice へ進める。

## 2. Why This Slice Next
- `switch_to_tab_index` と `create_new_tab` に共有される deactivate/activate 順序を解消しないまま次の slice に進むと、tab lifecycle の中心ロジックが複数箇所に分散する。
- shared helper を先に切ることで、後続の `tab activation` と `background restore` を narrower な scope で扱える。
- この段階では request routing や background refresh、close-specific cleanup までは触らず、tab lifecycle の順序制御を整理することが主目的である。

## 3. Goal
- `tabs.rs` の shared lifecycle transition を helper と command 境界へ寄せる。
- `switch_to_tab_index` と `create_new_tab` が同じ deactivate/activate helper を使う状態にし、後続の tab activation slice の下地を作る。
- `close_tab_index` は call-site 固有 wrapper のまま残し、close-specific cleanup は shared helper に入れない。必要なら active tab fallback 適用側だけで shared activate helper を利用する。
- 既存の tab switch/create/close 契約、query focus 契約、compaction/restore 契約を維持する。

## 4. Scope
### In Scope
- active tab を離脱する際の共通 deactivate helper 抽出
- target tab を入場させる際の共通 activate helper 抽出
- `switch_to_tab_index` と `create_new_tab` から共通 helper を呼ぶ形への整理
- `close_tab_index` は close 固有 cleanup を維持したまま、必要最小限の active fallback 適用だけを shared activate helper に寄せる検討
- 必要最小限の `TabLifecycleCommand` scaffolding 導入
- `DESIGN.md` / `TESTPLAN.md` に ownership boundary と phase validation を追記

### Out of Scope
- background restore refresh / active-background request preemption
- `pending_restore_refresh` の設計変更
- `close_tab_index` の request routing cleanup 全体の共通化
- tab reorder / drag-and-drop ロジックの変更
- root change / updater / filelist の仕様変更

## 5. Candidate Ownership Boundary
- `TabLifecycleHelper` が扱うもの:
  - active tab 離脱前の sync / compact
  - target tab 入場時の apply / restore
  - focus request の共通遷移
- `FlistWalkerApp` 側に残すもの:
  - egui event 受付
  - tab ID 採番
  - worker bus / pipeline への最終 dispatch
  - persistence 実行
- `tabs.rs` 内で残すもの:
  - `AppTabState` の data model
  - `capture_active_tab_state`, `apply_tab_state`, `compact_inactive_tab_state`, `restore_results_from_compacted_tab` の具体処理
  - `close_tab_index` の request routing cleanup と removed tab 後始末
- 境界ルール:
  - helper は tab lifecycle の順序制御だけを持ち、新しい request routing state を抱え込まない
  - `trigger_restore_refresh_for_active_tab` はこの slice で実装変更しない。呼び出し位置を維持したまま、後続 slice で扱う seam として残す

## 6. Risks
- Risk: tab switch/create の共通化範囲を広げすぎると、close-specific cleanup や new-tab 初期化が helper に流れ込む。
  - Impact: 高
  - Mitigation: helper は deactivate/activate の共通部分だけに絞り、create/close 固有分岐は各 call site に残す。
- Risk: `trigger_restore_refresh_for_active_tab` を helper に抱え込みすぎると、後続の background restore slice と境界が競合する。
  - Impact: 中
  - Mitigation: この計画では restore refresh を untouched seam として扱い、明示的な回帰テストで保持を確認する。
- Risk: lifecycle helper 抽出で `session_tabs` の restore 契約が崩れる。
  - Impact: 高
  - Mitigation: `session_tabs` を phase gate に入れ、background tab retention、active tab entries 保持、focus flag 回帰を継続確認する。

## 7. Execution Strategy
1. Phase 1: docs と lifecycle command scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/tabs.rs`
   - Action: `TabLifecycleCommand` 群と shared helper の ownership boundary を定義する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
2. Phase 2: shared lifecycle helper 抽出
   - Files: `rust/src/app/tabs.rs`
   - Action: `switch_to_tab_index` と `create_new_tab` から deactivate/activate helper を呼ぶ形へ整理し、`close_tab_index` は close-specific cleanup を維持したまま active fallback だけ共有できるかを検討する。
   - Verification: `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test app_core -- --nocapture`
3. Phase 3: lifecycle command dispatch と targeted regression 固定
   - Files: `rust/src/app/tabs.rs`, 必要なら `rust/src/app/mod.rs`
   - Action: helper が返す command を dispatch し、tab lifecycle 回帰点を固定する。
   - Verification: `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test ctrl_t_creates_new_tab_and_activates_it -- --nocapture`; `cd rust && cargo test ctrl_w_closes_current_tab_and_keeps_last_tab -- --nocapture`; `cd rust && cargo test inactive_tab_results_are_compacted_and_restored_on_activation -- --nocapture`; `cd rust && cargo test switching_to_restored_background_tab_triggers_lazy_refresh -- --nocapture`; `cd rust && cargo test background_tab_search_and_preview_responses_are_retained -- --nocapture`; `cd rust && cargo test background_tab_index_batches_do_not_override_active_tab_entries -- --nocapture`; `cd rust && cargo test`

## 8. Detailed Tasks
- [ ] `DESIGN.md` に shared tab lifecycle helper の ownership boundary を追加
- [ ] `TESTPLAN.md` に phase validation を追加
- [ ] `TabLifecycleCommand` 群を定義
- [ ] active tab 離脱前処理の helper を切り出す
- [ ] target tab 入場処理の helper を切り出す
- [ ] `switch_to_tab_index` と `create_new_tab` を共通 helper ベースへ整理する
- [ ] `close_tab_index` は close 固有 cleanup を維持しつつ、active fallback 適用側だけ helper 利用可否を判断する
- [ ] tab lifecycle の既存回帰が崩れていないことを確認する

## 9. Implementation Checkpoints
- Checkpoint 1: `capture_active_tab_state`, `apply_tab_state`, `compact_inactive_tab_state` 自体の責務は変えず、順序制御だけを切り出す。
- Checkpoint 2: `create_new_tab` 固有の tab 初期化は共通 helper に無理に押し込まない。
- Checkpoint 3: `close_tab_index` の active index 調整と fallback tab 適用は既存契約を維持する。
- Checkpoint 4: `focus_query_requested` / `unfocus_query_requested` の遷移は tab switch/create/close で変えない。
- Checkpoint 5: `results_compacted` と `restore_results_from_compacted_tab` の契約を壊さない。
- Checkpoint 6: `trigger_restore_refresh_for_active_tab` は後続 slice のために独立した呼び出し意図として残し、この計画では挙動変更しない。
- Checkpoint 7: `close_tab_index` の request routing cleanup は helper へ寄せない。
- Checkpoint 8: tab switch/create/close 後の `focus_query_requested` / `unfocus_query_requested` を targeted regression で確認する。

## 10. Validation Plan
- Automated:
  - `cd rust && cargo check`
  - `cd rust && cargo test session_tabs -- --nocapture`
  - `cd rust && cargo test app_core -- --nocapture`
  - `cd rust && cargo test ctrl_t_creates_new_tab_and_activates_it -- --nocapture`
  - `cd rust && cargo test ctrl_w_closes_current_tab_and_keeps_last_tab -- --nocapture`
  - `cd rust && cargo test inactive_tab_results_are_compacted_and_restored_on_activation -- --nocapture`
  - `cd rust && cargo test switching_to_restored_background_tab_triggers_lazy_refresh -- --nocapture`
  - `cd rust && cargo test background_tab_search_and_preview_responses_are_retained -- --nocapture`
  - `cd rust && cargo test background_tab_index_batches_do_not_override_active_tab_entries -- --nocapture`
  - tab switch/create/close 後の `focus_query_requested` / `unfocus_query_requested` を断言する targeted regression を追加または既存 test に追加
  - `cd rust && cargo test`
- Manual:
  - 新規 tab 作成 -> tab switch -> tab close
  - compact 済み tab を再度 active にして結果復元を確認
- Regression focus:
  - tab switch 後の root/query/result 復元
  - new tab 作成時の初期状態
  - close 後の active tab 維持
  - inactive compaction と restore
  - tab switch/create/close 後の focus flag 維持
  - restore-refresh seam と background-tab retention が崩れていない

## 11. Rollback Plan
- Phase ごとの小コミットを維持し、必要なら `git revert` で戻す。
- 部分失敗時は対象ファイル単位の `git restore <file>` を使い、他差分を巻き込まない。

## 12. Exit Criteria
- `switch_to_tab_index` と `create_new_tab` が共有 helper を介して lifecycle 順序を扱う。
- `close_tab_index` は close 固有 cleanup を維持したまま、必要最小限の active fallback 適用だけを共有 helper に寄せるか、寄せない理由が計画に沿って明確である。
- `session_tabs`, `app_core`, targeted tab lifecycle tests, `cargo test` が green である。
- 後続の `tab activation / background restore` slice に進むための shared boundary が docs とコード上で明確になっている。

## 13. Temporary Rule Draft
- For `tab-lifecycle-shared-helper`, read `docs/CHANGE-PLAN-20260404-tab-lifecycle-shared-helper.md` before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove the temporary rule from `AGENTS.md` and delete this change plan after the work is complete.
