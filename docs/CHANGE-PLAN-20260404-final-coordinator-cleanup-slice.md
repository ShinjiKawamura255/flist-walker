# CHANGE PLAN: Final Coordinator Cleanup Slice

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: final-coordinator-cleanup
- Parent Roadmap: `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md`
- Related Workstream: Final Coordinator Cleanup

## 1. Background
- request routing ownership と render UI orchestration が完了し、feature ごとの state transition は `tabs.rs`、`filelist.rs`、`update.rs`、`cache.rs`、`render.rs` へかなり寄った。
- それでも `app/mod.rs` には、frame ごとの poll/dispatch sequencing、viewport close 判定、repaint scheduling、window geometry/save timing、action/sort response の active-tab 側 apply、worker shutdown の exit glue が残っている。
- roadmap の次 workstream は、この `mod.rs` に残る cross-feature dispatch / coordinator concern を整理し、`FlistWalkerApp` を eframe hook と最終 coordinator に絞る段階である。

## 2. Goal
- `app/mod.rs` から frame/update/exit orchestration の open-coded sequence を縮小し、coordinator helper 経由で説明できる構造へ寄せる。
- active-tab 向け action/sort response consume、shutdown/exit、frame tick sequencing の owner 境界を明確化する。
- render/request routing/filelist/update の既存 feature 契約を変えずに、`mod.rs` の shared glue を減らす。

## 3. Scope
### In Scope
- `eframe::App::update()` に残る frame tick / poll / repaint / close sequencing の局所化
- `poll_action_response()` / `poll_sort_response()` の active-tab consume を owner helper 境界へ寄せる
- shutdown / exit / `Drop` に残る worker teardown と UI state persist ordering の局所化
- この slice で触った coordinator 境界に限る `DESIGN.md` / `TESTPLAN.md` / roadmap の局所同期

### Out of Scope
- `render.rs` の widget contract や shortcut 契約の変更
- root selector / query input / result interaction の再設計
- feature owner module (`filelist.rs`, `update.rs`, `tabs.rs`, `cache.rs`) の新しい責務追加
- index/search/filelist worker protocol の変更
- `TASKS.md` を含む docs closure workstream の最終同期
- plan 撤去や最終文書整形のような closure 専用 cleanup

## 4. Candidate Ownership Boundary
- `mod.rs` 側に残すもの:
  - `eframe::App` の entry point (`update`, `on_exit`, `Drop`)
  - 全 feature owner を束ねる最終 coordinator 呼び出し
- `mod.rs` から追い出すもの:
  - frame tick の open-coded poll 順序
  - active-tab action/sort response の直接 consume
  - shutdown / viewport close / UI state persist の分岐重複
- owner 候補:
  - action/sort の active-tab consume helper は `tabs.rs` / `cache.rs` 近傍へ寄せる
  - frame/update/exit sequencing は新規 helper (`coordinator` 相当) または `mod.rs` 内の専用 helper 群へ寄せる
- 境界ルール:
  - feature state mutation は既存 owner API を優先し、`mod.rs` から直接 field を触る分岐を増やさない
  - `update()` は「tick helper を呼ぶだけ」に近づけるが、`eframe` hook 自体は `mod.rs` に残す
  - Phase 2 以降でも render/input 契約や worker protocol 変更は混ぜない

## 5. Risks
- Risk: `update()` の tick 順序を崩して stale response ignore や repaint 契約が壊れる。
  - Impact: 高
  - Mitigation: `app_core` と `session_tabs` の既存 targeted regression を phase gate に入れ、frame 順序の変更は最小単位で進める。
- Risk: action/sort active-tab consume を誤って背景 tab helper と混線させ、request_id cleanup が二重化する。
  - Impact: 高
  - Mitigation: active-tab と background-tab の consume 境界を plan に明記し、response poll 移設時は stale response regression を再使用する。
- Risk: shutdown / `Drop` / `on_exit` を一気に触って worker teardown 順序や UI state persist が壊れる。
  - Impact: 中
  - Mitigation: viewport close / persist / join timeout の順序を helper 化してから置換し、既存 shutdown regression を gate に含める。
- Risk: slice が広がりすぎて docs closure workstream を先食いする。
  - Impact: 中
  - Mitigation: この slice は `mod.rs` の coordinator glue とその近接 helper に限定し、plan 撤去や docs 最終整理は次 workstreamへ残す。

## 6. Execution Strategy
1. Phase 1: docs と coordinator scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/mod.rs`, 必要なら新規 helper module
   - Action: frame/update/exit orchestration helper の型または seam を追加する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
   - Gate: response handling / shutdown ordering / repaint の挙動変更を入れない。挙動変更が入る時点で VM-002 へ昇格する。
2. Phase 2: response poll / dispatch cleanup
   - Files: `rust/src/app/mod.rs`, `rust/src/app/tabs.rs`, `rust/src/app/cache.rs`, 必要なら新規 helper module
   - Action: action/sort response の active-tab consume と frame tick の poll sequence を helper 経由へ寄せる。
   - Verification: `cd rust && cargo test app_core -- --nocapture`; `cd rust && cargo test session_tabs -- --nocapture`
3. Phase 3: shutdown / exit coordinator cleanup
   - Files: `rust/src/app/mod.rs`, 必要なら `rust/src/app/session.rs`, `rust/src/app/bootstrap.rs`, 新規 helper module
   - Action: `update()` の close path、`on_exit()`、`Drop`、worker shutdown/persist ordering を helper 経由へ寄せる。
   - Verification: `cd rust && cargo test app_core -- --nocapture`; `cd rust && cargo test shortcuts -- --nocapture`; `cd rust && cargo test apply_started_update_response_requests_app_close -- --nocapture`
4. Phase 4: regression 固定と docs 同期
   - Files: `rust/src/app/tests/app_core.rs`, `rust/src/app/tests/session_tabs.rs`, `rust/src/app/tests/shortcuts.rs`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
   - Action: coordinator cleanup 後の targeted regression を固定し、この slice で変更した coordinator 境界に限って `DESIGN.md` / `TESTPLAN.md` を局所同期する。
   - Verification: doc diff review; `rg` 参照整合確認; `cd rust && cargo test regression_gui_close_uses_short_worker_join_timeout_budget -- --nocapture`; `cd rust && cargo test stale_action_completion_is_ignored_by_request_id -- --nocapture`; `cd rust && cargo test late_sort_metadata_response_is_ignored_for_removed_tab -- --nocapture`; `cd rust && cargo test start_update_install_ignores_repeat_requests_after_first_click -- --nocapture`; `cd rust && cargo test background_tab_activation_consumes_pending_restore_refresh_once -- --nocapture`; `cd rust && cargo test`

## 7. Exit Criteria
- `app/mod.rs` の `update()` / `on_exit()` / `Drop` から、frame/update/exit orchestration の open-coded 分岐が縮小している。
- active-tab の action/sort response consume が owner helper 経由で説明可能になっている。
- render/input/worker protocol の既存契約を変えずに `mod.rs` の shared glue が減っている。
- targeted regression と `cargo test` が green である。

## 8. Review Notes
- 2026-04-04 initial review: render slice cleanup 後の next active slice として main thread で review した。今回のセッションでもユーザからサブエージェント委譲の明示がないため、`two-level-plan-driven-changes` の初回レビュー工程は main thread で代替した。
- Review findings:
  - `mod.rs` 全体 cleanup と書くと scope が広すぎるため、`update()` / `on_exit()` / `Drop` と active-tab response consume に限定した。
  - `action` / `sort` の active-tab consume と background-tab consume の境界を計画上で明示した。
  - shutdown / repaint / viewport close の regressions を phase gate に追加し、`close_requested_for_install` の close path も verification に含めた。
- 2026-04-04 subagent review:
  - docs closure workstream と混線しないよう、docs 更新はこの slice で触る coordinator 境界の局所同期に限定した。
  - `TASKS.md` と plan 撤去のような closure 専用 cleanup を out of scope へ戻した。
  - Phase 4 の docs 更新には doc diff review と `rg` 参照整合確認を必須 verification として追加した。
- 2026-04-04 convergence review: 上記修正後、scope は `Final Coordinator Cleanup` workstream に収まり、前 slice の render 契約とも干渉しないことを確認した。blocking issue はなし。

## 9. Temporary Rule Draft
- For `final-coordinator-cleanup`, read `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md` first, then `docs/CHANGE-PLAN-20260404-final-coordinator-cleanup-slice.md`.
- Follow the roadmap for ordering and dependencies, and follow this slice plan for implementation detail.
- If this slice changes the roadmap's scope or order, update the roadmap first.
- Remove the temporary rule and delete both plans after the covered work is complete.
