# CHANGE PLAN: Render UI Orchestration Slice

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: render-ui-orchestration
- Parent Roadmap: `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md`
- Related Workstream: Render/UI Orchestration

## 1. Background
- request routing localization が完了し、preview/action/sort の request-tab binding は `cache.rs` / `tabs.rs` owner API を経由するようになった。
- 現在の `render.rs` には、描画だけでなく top action button からの副作用起動、FileList dialog/update dialog の確定処理、tab reorder / close / switch などの state transition 起動が残っている。
- roadmap の次 workstream は `render.rs` に残る dialog / action / reorder 周辺の coordinator 整理であり、ここを進める前提が整った。

## 2. Goal
- `render.rs` を「描画と UI 入力の取得」に近づけ、dialog/action/reorder 周辺の state transition を command / intent 境界へ寄せる。
- `FlistWalkerApp` は render phase 中に副作用を直書きする箇所を減らし、描画後に command / intent を dispatch する coordinator に近づける。
- tab reorder、dialog confirm/cancel、top action 起動の既存契約と回帰テストを維持する。

## 3. Scope
### In Scope
- top action button 群の command / intent 化
- FileList dialog 群の confirm / cancel command 化
- update dialog の confirm / later / suppress command 化
- tab bar の switch / close / reorder / accent menu 周辺の command / intent 化
- `DESIGN.md` / `TESTPLAN.md` の更新

### Out of Scope
- root change workflow の再分割
- root selector dropdown / browse / default root / saved root list の orchestration 変更
- search/index/pipeline worker protocol の変更
- `mod.rs` 全体の最終 cleanup
- query shortcut や結果リスト操作の契約変更

## 4. Candidate Ownership Boundary
- `render.rs` 側に残すもの:
  - egui widget 描画
  - click / drag / dialog button など UI 入力の収集
  - render-local command / intent の組み立て
- `render.rs` から追い出すもの:
  - confirm / cancel 後の app state mutation
  - action/create-filelist/refresh/tab reorder の直接起動
  - dialog 表示後に続く branchy state transition
- 今回 `render.rs` に残す直接 mutation:
  - root selector dropdown の open/close
  - browse/default root/saved root list まわりの root change 起動
  - query/history input 編集と shortcut 適用
- `mod.rs` / owner module 側に残すもの:
  - render command / intent の dispatch
  - 各 feature owner (`tabs.rs`, `filelist.rs`, `update.rs`) への橋渡し
- 境界ルール:
  - `render.rs` は command / intent を返し、できるだけ直接 state mutation しない
  - tab reorder は drag hit-test を `render.rs` に残し、reorder 実行自体は command dispatch に寄せる
  - dialog の confirm / cancel は描画中に実行せず、描画後の command dispatch で反映する

## 5. Risks
- Risk: 描画中の直接 mutation を外す過程で button / dialog の 1 frame 挙動が変わり、二重実行や取りこぼしが起きる。
  - Impact: 高
  - Mitigation: action button、FileList dialog、update dialog の既存回帰を phase gate に入れる。
- Risk: tab reorder の drag state と close/switch の競合を崩し、drag release 時の reorder 契約が壊れる。
  - Impact: 高
  - Mitigation: reorder は hit-test と drag tracking を `render.rs` に残し、command dispatch だけを切る。`session_tabs` / `render_tests` を回帰 gate に含める。
- Risk: この slice で `mod.rs` final cleanup まで進めて scope が膨らむ。
  - Impact: 中
  - Mitigation: この slice は `render.rs` 起点の orchestration に限定し、owner module の新設や最終 coordinator cleanup は次 workstream に残す。
- Risk: root selector / query input の既存直接 mutation まで巻き込んでしまい、root change や shortcut の既存 slice と干渉する。
  - Impact: 中
  - Mitigation: root selector と query/history input は明示的に out of scope とし、この slice では top action / dialog / tab bar に限定する。

## 6. Execution Strategy
1. Phase 1: docs と render command scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/render.rs`, 必要なら `rust/src/app/mod.rs`
   - Action: render-local command / intent 型と dispatch seam を定義する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
   - Gate: dialog/action/reorder の挙動変更を入れない。挙動変更が必要になった時点でこの Phase は VM-002 へ昇格し `cd rust && cargo test` を実行する。
2. Phase 2: top action / dialog intent localization
   - Files: `rust/src/app/render.rs`, `rust/src/app/mod.rs`, 必要なら `rust/src/app/filelist.rs`, `rust/src/app/update.rs`
   - Action: top action button 群と FileList / update dialog の確定処理を command dispatch 経由へ寄せる。
   - Verification: `cd rust && cargo test app_core -- --nocapture`; `cd rust && cargo test render_tests -- --nocapture`; `cd rust && cargo test search_filelist -- --nocapture`
3. Phase 3: tab bar reorder / close / switch intent localization
   - Files: `rust/src/app/render.rs`, `rust/src/app/mod.rs`, `rust/src/app/tabs.rs`
   - Action: tab bar の switch / close / reorder 起動を command dispatch 経由へ寄せる。
   - Verification: `cd rust && cargo test session_tabs -- --nocapture`; `cd rust && cargo test render_tests -- --nocapture`
4. Phase 4: regression 固定と docs 同期
   - Files: `rust/src/app/tests/render_tests.rs`, `rust/src/app/tests/session_tabs.rs`, `rust/src/app/tests/app_core.rs`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
   - Action: dialog/action/reorder の targeted regression を command-based 実装に合わせて固定し、docs を最終同期する。
   - Verification: `cd rust && cargo test background_tab_activation_consumes_pending_restore_refresh_once -- --nocapture`; `cd rust && cargo test move_tab_preserves_per_tab_state_carryover_after_reorder -- --nocapture`; `cd rust && cargo test filelist_use_walker_dialog_text_describes_background_execution -- --nocapture`; `cd rust && cargo test confirm_pending_overwrite_starts_filelist_creation -- --nocapture`; `cd rust && cargo test start_update_install_ignores_repeat_requests_after_first_click -- --nocapture`; `cd rust && cargo test top_action_labels_show_default_create_label_when_idle -- --nocapture`; `cd rust && cargo test`

## 7. Exit Criteria
- `render.rs` 内の top action / dialog / reorder 周辺で、描画後に dispatch すべき state transition が command / intent 経由になっている。
- `render.rs` から `create_filelist`、`request_index_refresh`、`move_tab`、`close_tab_index`、update confirm/cancel のような直接起動が縮小し、描画コードと state transition の境界が docs で説明可能になっている。
- root selector と query/history input の既存直接 mutation はこの slice の対象外として維持され、scope 逸脱がない。
- dialog/action/reorder の targeted regression と `cargo test` が green である。

## 8. Review Notes
- 2026-04-04 initial review: main thread で roadmap / active slice をレビューした。ユーザからサブエージェント委譲の明示がないため、`two-level-plan-driven-changes` の初回レビュー工程は main thread で代替した。
- Adopted:
  - root selector / query input を out of scope として明示した。
  - FileList / update dialog の behavior regression を verification に追加した。
  - command 化の対象を top action / dialog / tab bar に限定した。
- 2026-04-04 convergence review: 上記修正後、scope は Render/UI Orchestration workstream に収まり、次の実装着手先として妥当であることを確認した。blocking issue はなし。

## 9. Temporary Rule Draft
- For `render-ui-orchestration`, read `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md` first, then `docs/CHANGE-PLAN-20260404-render-ui-orchestration-slice.md`.
- Follow the roadmap for ordering and dependencies, and follow this slice plan for implementation detail.
- If this slice changes the roadmap's scope or order, update the roadmap first.
- Remove the temporary rule and delete both plans after the covered work is complete.
