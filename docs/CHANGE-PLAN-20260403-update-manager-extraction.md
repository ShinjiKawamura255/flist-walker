# CHANGE PLAN: Update Manager Extraction

## Metadata
- Date: 2026-04-03
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: update-manager-extraction
- Related Tickets/Issues: God Object follow-up

## 1. Background
- `FileList` スライスでは、`FileListManager` を導入して request lifecycle、stale 応答吸収、cancel 処理を `FlistWalkerApp` から切り離しました。
- 次の候補として `tabs.rs`、`pipeline.rs`、`update.rs` を比較すると、`tabs` と `pipeline` は tab snapshot、query/history、request routing、background refresh など横断責務が多く、切り離し時の巻き込み範囲が大きいです。
- 一方 `update.rs` は、startup check / install request / prompt / failure dialog / close request という 1 本の縦フローを持ち、`UpdateState` も他の機能 state から比較的独立しています。
- ただし update state は `update.rs` だけで完結しておらず、`render.rs` の dialog 操作、`session.rs` の skipped/suppress 永続化、`mod.rs` の close orchestration にもまたがっています。
- そのため、God Object 解消の次手としては `Update` を `UpdateManager` に切り離すのが、実装コストと安全性のバランスが最も良いと判断します。

## 2. Why This Slice Next
- `update.rs` は約 200 行で、責務が update worker request/response と dialog state に集中している。
- tab/request routing のような複数機能共有状態を直接持たないため、`FileList` と同じ「manager + command dispatch」パターンを再利用しやすい。
- `start_update_install` と `poll_update_response` には、request_id 管理、worker dispatch、notice 更新、prompt state 更新、close request など God Object 由来の混在があり、分離効果が明確に出る。
- UI 影響範囲は update dialog と notice に限定され、`tabs` や `pipeline` より回帰面積が小さい。

## 3. Goal
- `UpdateState` を包む `UpdateManager` を導入し、update worker request lifecycle と response settle を `FlistWalkerApp` から分離する。
- `FlistWalkerApp` は update command の dispatch と app-level orchestration だけを担当し、prompt/failure/install state の直接操作を減らす。
- startup check / install request / stale response 無視 / suppress skip persistence / install close request の契約を維持する。

## 4. Scope
### In Scope
- `UpdateState` を `UpdateManager` で包む。
- update worker request の開始、send failure、response settle を manager メソッドへ移す。
- `Update` 用のカテゴリ化 command（例: `UpdateUiCommand`, `UpdateWorkerCommand`, `UpdateAppCommand`）を導入する。
- `request_id` の allocate / pending / stale ignore を manager 側に閉じる。
- `render.rs` の update dialog 操作を manager command 境界に合わせて整理する。
- `session.rs` の skipped/suppress 永続化と `mod.rs` の close_requested_for_install 処理を、manager 導入後の ownership に合わせて追従させる。
- `DESIGN.md` / `TESTPLAN.md` に update manager 境界と検証観点を追記する。

### Out of Scope
- updater 本体 (`rust/src/updater.rs`) の仕様変更。
- GitHub Releases 判定ロジックや署名検証ロジックの変更。
- tab/session/pipeline の設計変更。
- update dialog の UI レイアウト変更。

## 5. Ownership Boundary
- `UpdateManager` が所有するもの:
  - `next_request_id`, `pending_request_id`, `in_progress`
  - `prompt`, `check_failure`, `skipped_target_version`, `suppress_check_failure_dialog`
  - update worker request/response に対する stale 判定
- `FlistWalkerApp` 側に残すもの:
  - `persist_ui_state_now()` / `mark_ui_state_dirty()` の実行
  - `close_requested_for_install` を反映したアプリ終了 orchestration
  - worker bus への最終 dispatch
  - dialog の表示位置や egui レンダリング
- `render.rs` との境界:
  - render layer は dialog 描画と入力取得だけを担当する。
  - prompt/failure/install_started の遷移は manager command を通して反映し、`render.rs` からの `update_state` 直接 mutation を段階的に減らす。
- `session.rs` との境界:
  - skipped/suppress は manager が保持してよいが、save/load 自体は app/session に残す。
  - persistence 実行タイミングは `UpdateAppCommand` を通して `FlistWalkerApp` が決定する。

## 6. Risks
- Risk: stale update response を誤って取り込むと、旧 request が prompt や failure dialog を上書きする。
  - Impact: 中
  - Mitigation: `UpdateManager::settle_response` で pending request_id と一致しない応答は吸収する。
- Risk: install 開始済みフラグの遷移を壊すと、二重起動や prompt 復帰バグが出る。
  - Impact: 高
  - Mitigation: `start_update_install` の state transition を manager 側で一元化し、既存 unit test を維持する。
- Risk: app-level persistence を manager に抱え込むと責務境界が曖昧になる。
  - Impact: 中
  - Mitigation: persistence と close request は `UpdateAppCommand` として app 側に委譲する。

## 7. Execution Strategy
1. Phase 1: docs と command scaffolding
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `rust/src/app/update.rs`
   - Action: update manager 境界を docs に追記し、`UpdateCommand` 群を追加する。
   - Verification: doc diff review, `rg` 参照整合確認, `cd rust && cargo check`
2. Phase 2: `UpdateManager` の導入
   - Files: `rust/src/app/state.rs`, `rust/src/app/mod.rs`, `rust/src/app/update.rs`, `rust/src/app/session.rs`
   - Action: `UpdateState` を包む manager を追加し、request lifecycle と stale settle を manager へ移す。
   - Verification: `cd rust && cargo test app_core -- --nocapture`; update persistence/close path に関わる test を明示確認
3. Phase 3: command dispatch への結線
   - Files: `rust/src/app/update.rs`, `rust/src/app/render.rs`, `rust/src/app/mod.rs`
   - Action: update manager が返す command を `FlistWalkerApp` が dispatch する形へ寄せる。
   - Verification: `cd rust && cargo test`; update 関連 test の green を確認

## 8. Detailed Tasks
- [ ] `DESIGN.md` に Update manager 境界を追加
- [ ] `TESTPLAN.md` に update manager phase の検証条件を追加
- [ ] `UpdateCommand` 群を定義
- [ ] `UpdateManager` を `state.rs` に追加
- [ ] `request_startup_update_check` / `start_update_install` を manager 経由へ変更
- [ ] `poll_update_response` を manager settle + command dispatch へ変更
- [ ] `session.rs` の skipped/suppress 永続化境界を manager 導入後も維持する
- [ ] `mod.rs` の close_requested_for_install 経路が回帰していないことを確認する
- [ ] update dialog / startup failure の既存 test を通す

## 9. Implementation Checkpoints
- Checkpoint 1: `close_requested_for_install` は manager が直接 window close せず、app command として返す。
- Checkpoint 2: `persist_ui_state_now` を manager に持ち込まない。
- Checkpoint 3: `prompt.install_started` の遷移は request send failure と response failure の両方で回帰しない。
- Checkpoint 4: suppress/skip 系の persistence は app が実行し、manager は意図だけ返す。
- Checkpoint 5: update worker unavailable 時の notice は現行文言を維持する。
- Checkpoint 6: `render.rs` に残る update state の直接 mutation は、install/prompt/failure の主要遷移で manager command 経由へ置き換える。
- Checkpoint 7: `session.rs` の save/load と `mod.rs` の viewport close は、manager 導入後も責務境界が崩れていないことを確認する。

## 10. Validation Plan
- Automated:
  - `cd rust && cargo check`
  - `cd rust && cargo test app_core -- --nocapture`
  - `cd rust && cargo test session_tabs -- --nocapture`
  - `cd rust && cargo test`
- Manual:
  - update prompt の Later / Install / skip checkbox
  - startup failure dialog の dismiss / suppress
- Regression focus:
  - stale response 無視
  - install 開始後の二重クリック無効化
  - close request for install
  - skipped/suppress の永続化 round-trip

## 11. Rollback Plan
- Phase ごとの小コミットを維持し、必要なら `git revert` で戻す。
- 途中失敗時は対象ファイル単位の `git restore <file>` を使い、他変更は巻き込まない。

## 12. Exit Criteria
- `UpdateManager` 導入後も `cargo test` が green である。
- `FlistWalkerApp` から update-specific な pending/stale/install state 直接操作が減っている。
- docs が実装境界と検証条件を説明できる状態になっている。

## 13. Final Notes
- `Update` を終えた後の次候補は `Tab/Root orchestration` だが、これは update より大きい横断 refactor になるため、別計画として切り出す。
