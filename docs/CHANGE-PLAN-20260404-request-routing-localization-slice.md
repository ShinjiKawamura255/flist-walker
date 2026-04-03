# CHANGE PLAN: Request Routing Localization Slice

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: request-routing-localization
- Parent Roadmap: `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md`
- Related Workstream: Request Routing Ownership

## 1. Background
- 現在の `RequestTabRoutingState` は [state.rs](/mnt/d/work/flistwalker/rust/src/app/state.rs#L669) に preview/action/sort の routing map をまとめて保持している。
- 実際の owner は分散しており、preview は [cache.rs](/mnt/d/work/flistwalker/rust/src/app/cache.rs)、action/sort は [mod.rs](/mnt/d/work/flistwalker/rust/src/app/mod.rs) に近い。
- tab close cleanup では `clear_for_tab()` を追加して shared bag の後始末をまとめたが、owner 自体はまだ shared state のままである。
- 上位 roadmap の最初の workstream として、この shared bag を owner 近接へ寄せる。

## 2. Goal
- `RequestTabRoutingState` の owner を局所化し、preview/action/sort の routing をそれぞれ責務の近い module へ寄せる。
- `FlistWalkerApp` は shared bag を直接保持せず、owner 経由で request-tab binding を扱う。
- tab close cleanup や stale response ignore の既存契約を維持する。

## 3. Scope
### In Scope
- preview routing の owner localization
- action/sort routing の owner localization
- `close_tab_index()` / stale response path と整合する cleanup API の再整理
- `DESIGN.md` / `TESTPLAN.md` の更新

### Out of Scope
- preview/action/sort worker protocol の変更
- `render.rs` の dialog / panel orchestration cleanup
- index/search queue や request_id 契約の全面見直し

## 4. Candidate Ownership Boundary
- Preview routing:
  - `cache.rs` 側 owner へ寄せる
- Action/sort routing:
  - `mod.rs` 直下の shared bag ではなく、action/sort の責務近辺へ寄せる
- `FlistWalkerApp` 側に残すもの:
  - request 発行の入口
  - owner API 呼び出し
- 境界ルール:
  - routing map は owner module が持つ
  - `close_tab_index()` は owner API を呼んで cleanup する
  - stale response ignore は従来どおり request_id / routing に基づいて行う

## 5. Risks
- Risk: owner 移動の途中で stale response ignore が崩れ、別 tab へ誤適用される。
  - Impact: 高
  - Mitigation: preview/action/sort の stale response 回帰を phase gate に入れる。
- Risk: close cleanup API が owner 移動で分散しすぎ、`close_tab_index()` の可読性が逆に下がる。
  - Impact: 中
  - Mitigation: close path から呼ぶ cleanup API は owner ごとに 1 entry に揃える。
- Risk: 上位 roadmap を無視して `render.rs` 側の整理まで同時に進めてしまう。
  - Impact: 中
  - Mitigation: この slice は request routing owner localization に限定する。

## 6. Execution Strategy
1. Phase 1: docs と routing ownership scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/state.rs`, 必要なら owner module
   - Action: owner boundary と API scaffolding を定義する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
2. Phase 2: preview routing owner localization
   - Files: `rust/src/app/cache.rs`, `rust/src/app/state.rs`, 必要なら `rust/src/app/tabs.rs`
   - Action: preview routing を owner 側へ寄せる。
   - Verification: `cd rust && cargo test app_core -- --nocapture`; `cd rust && cargo test session_tabs -- --nocapture`
3. Phase 3: action/sort routing owner localization
   - Files: `rust/src/app/mod.rs`, `rust/src/app/state.rs`, 必要なら `rust/src/app/tabs.rs`
   - Action: action/sort routing を owner 側へ寄せる。
   - Verification: `cd rust && cargo test app_core -- --nocapture`; `cd rust && cargo test session_tabs -- --nocapture`
4. Phase 4: cleanup API 整理と regression 固定
   - Files: `rust/src/app/tabs.rs`, `rust/src/app/tests/app_core.rs`, `rust/src/app/tests/session_tabs.rs`
   - Action: close cleanup と stale response regression を owner-localized 形に揃える。
   - Verification: `cd rust && cargo test close_tab_clears_filelist_and_request_routing_for_removed_tab -- --nocapture`; `cd rust && cargo test close_tab_ignores_late_background_responses_for_removed_tab -- --nocapture`; `cd rust && cargo test stale_action_completion_is_ignored_by_request_id -- --nocapture`; `cd rust && cargo test`

## 7. Exit Criteria
- `RequestTabRoutingState` が shared bag として `FlistWalkerApp` に残っていない、または薄い facade まで縮小している。
- preview/action/sort の owner がコードと docs で説明可能になっている。
- close cleanup と stale response ignore regression が green である。

## 8. Temporary Rule Draft
- For `request-routing-localization`, read `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md` first, then `docs/CHANGE-PLAN-20260404-request-routing-localization-slice.md`.
- Follow the roadmap for ordering and dependencies, and follow this slice plan for implementation detail.
- If this slice changes the roadmap's scope or order, update the roadmap first.
- Remove the temporary rule and delete both plans after the covered work is complete.
