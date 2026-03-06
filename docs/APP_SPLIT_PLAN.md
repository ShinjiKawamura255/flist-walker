# APP Split Plan

## Objective
- Goal: `rust/src/app.rs` の保守性を改善し、機能追加・不具合修正時の変更影響範囲を局所化する。
- Non-goal: `indexer.rs` / `search.rs` / `ui_model.rs` / `actions.rs` の全面再分割、機能仕様変更、UI 挙動変更。
- Target file: [app.rs](/mnt/d/work/flistwalker/rust/src/app.rs)

## Current assessment
- `rust/src/app.rs` は 9000 行超で、状態型、ワーカー起動、永続化、ショートカット、IME、描画、`Drop`、大量の unit test を同居させている。
- 最初に分割すべき対象は `app.rs` 本体と inline test であり、他の src/tests ファイルは現時点では優先度が低い。
- 既存の UI 応答性契約と request_id による最新応答採用契約は維持必須。

## Constraints
- 機能互換を維持する。ショートカット、FileList、query history、tab restore の挙動を変えない。
- Phase は順番に進める。後続 Phase は前段の `cargo test` グリーンを前提に着手する。
- 境界分割のために不要な `pub` / `pub(crate)` 拡大を行わない。
- test は integration test 化せず、原則として `app` モジュール配下の unit test として維持する。

## Phase plan

### Phase 0: Baseline capture
- Scope:
- 現行 `app.rs` の責務境界とテスト群を棚卸しする。
- `docs/TASKS.md` / `docs/WORKLOG.md` に開始状態を記録する。
- Deliverables:
- この文書、`docs/TASKS.md`、`docs/WORKLOG.md`
- Exit:
- 分割順と DoD が確定している。

### Phase 1: Test extraction
- Scope:
- `rust/src/app.rs` の `#[cfg(test)] mod tests` を `rust/src/app/tests/` 配下へ機能別に分割する。
- 共通 helper を `rust/src/app/tests/support.rs` に集約する。
- Candidate slices:
- `shortcuts`
- `query_history`
- `index_pipeline`
- `filelist_flow`
- `window_ime`
- `tabs`
- Deliverables:
- `rust/src/app/tests/*.rs`
- `rust/src/app/tests/mod.rs`
- Exit:
- production code の変更を最小化したまま `app.rs` から巨大 inline test を除去できている。
- `cargo test --manifest-path rust/Cargo.toml --locked` が通る。

### Phase 2: Worker boundary extraction
- Scope:
- Request/Response 型と `spawn_*_worker` 群を `rust/src/app/workers.rs` へ移す。
- `WorkerRuntime` と shutdown/join 補助も同じ境界へ寄せる。
- Candidate items:
- `SearchRequest` / `SearchResponse`
- `PreviewRequest` / `PreviewResponse`
- `ActionRequest` / `ActionResponse`
- `KindResolveRequest` / `KindResolveResponse`
- `FileListRequest` / `FileListResponse`
- `IndexRequest` / `IndexResponse`
- `spawn_search_worker` から `spawn_index_worker`
- Exit:
- `mod.rs` 側に UI ロジック以外のスレッド起動詳細が残りすぎていない。
- request_id 契約と supersede/cancel 契約のテストが維持される。

### Phase 3: Persistence and tab state extraction
- Scope:
- UI state の load/save、saved tab 変換、tab restore/sanitize を `rust/src/app/session.rs` または `tabs.rs` へ分離する。
- Candidate items:
- `UiState`, `LaunchSettings`, `SavedTabState`, `SavedWindowGeometry`
- `load_ui_state`, `load_launch_settings`, `save_ui_state`
- `restore_tabs_enabled`, `sanitize_saved_tabs`
- `saved_tab_state_from_*`
- Exit:
- セッション永続化関連の責務が `FlistWalkerApp` 本体から分離される。
- tab restore 系テストが維持される。

### Phase 4: Input and shortcut extraction
- Scope:
- ショートカット、query history、IME フォールバック、deferred shortcut 実行を `rust/src/app/input.rs` へ分離する。
- Candidate items:
- `handle_shortcuts*`
- `consume_gui_shortcut`, `consume_emacs_shortcut`
- `run_deferred_shortcuts`
- `mark_query_edited`, `navigate_query_history`, `commit_query_history_*`
- `process_query_input_events`, `apply_emacs_query_shortcuts`
- Exit:
- 入力系回帰テストが維持される。
- `update()` から入力処理の詳細が後退する。

### Phase 5: Render and panel extraction
- Scope:
- `update()` 内の panel 構築と dialog 表示を `rust/src/app/render.rs` へ整理する。
- Candidate items:
- top panel
- status panel
- overwrite/filelist dialogs
- central panel render 呼び出し
- Exit:
- `impl eframe::App for FlistWalkerApp` の `update()` が orchestration 中心になる。
- UI 表示変更なしで `cargo test` グリーンを維持する。

## Execution policy
- 1 Phase 1 PR/commit 相当の粒度を維持し、複数 Phase を同時に混ぜない。
- 各 Phase 開始前に `docs/TASKS.md` を更新し、終了時に `docs/WORKLOG.md` へ結果を残す。
- Phase ごとに関連 docs と AGENTS の一時運用方針に矛盾がないか確認する。

## Risks
- `FlistWalkerApp` の private state に強く依存する test を誤って integration test 側へ逃がすと、公開範囲が拡大する。
- worker 群と index/search pipeline の境界を早く分けすぎると、型の移動量が増えて差分が肥大化する。
- `update()` の分割は見た目以上に副作用が多いため、input/render を同時に触ると回帰リスクが上がる。

## Validation
- Required:
- `cargo fmt --manifest-path rust/Cargo.toml`
- `cargo test --manifest-path rust/Cargo.toml --locked`
- Optional:
- 影響 Phase に応じて GUI 手動確認を追加する。

## Done criteria
- `rust/src/app.rs` から tests が分離され、worker/session/input/render の少なくとも主要3境界が別モジュール化されている。
- `app.rs` は app state と orchestration を中心に読めるサイズまで縮小している。
- `docs/TASKS.md` と `docs/WORKLOG.md` に全 Phase の記録が残っている。
