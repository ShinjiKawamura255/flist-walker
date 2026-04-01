# CHANGE PLAN: eframe Upgrade

## Metadata
- Date: 2026-04-01
- Owner: agent
- Target Project: FlistWalker
- Scope Label: eframe-upgrade
- Related Docs:
  - [EFRAME-UPGRADE-NOTES.md](/mnt/d/work/flistwalker/docs/EFRAME-UPGRADE-NOTES.md)
  - [ARCHITECTURE.md](/mnt/d/work/flistwalker/docs/ARCHITECTURE.md)

## 1. Background
FlistWalker は現在 `eframe 0.24.x` を使っている。既存の [EFRAME-UPGRADE-NOTES.md](/mnt/d/work/flistwalker/docs/EFRAME-UPGRADE-NOTES.md) で、`0.25.x` から `0.29.x` までの差分と、FlistWalker 側の影響箇所は整理済みである。

今回の目的は、調査メモを実装計画に落とし込み、`eframe` / `egui` 更新を段階的に進められる状態を作ることにある。特に native window / viewport 初期化、shortcut / IME、multi-display geometry の回帰を抑えながら進める必要がある。

## 2. Goal
- `eframe 0.24.x` から新しい `eframe` 系列へ安全に更新する。
- `main.rs`, `render.rs`, `input.rs`, `session.rs` の API 追従を段階的に行う。
- 既存の GUI 応答性、request_id 契約、window geometry 安定化を維持する。
- 変更後も `cargo test`、perf regression test、主要 GUI 手動確認を通す。

## 3. Scope
### In Scope
- `rust/Cargo.toml` / `rust/Cargo.lock` の `eframe` 関連依存更新
- `main.rs` の `NativeOptions` / `ViewportBuilder` / `run_native` 調整
- `render.rs` の panel / frame / combo box API 追従
- `input.rs` の shortcut / key enum 追従
- `session.rs` の window geometry / viewport 周辺調整
- 必要な docs 更新

### Out of Scope
- UI/UX の新機能追加
- `eframe` とは無関係な依存更新
- release workflow の変更
- 別 GUI framework への移行

## 4. Constraints and Assumptions
- 変更は phase ごとにコンパイル・テスト可能な状態で終える。
- `rust/src/app/workers.rs` や index 経路に波及した場合は、AGENTS の VM-003 に従って perf テストを実行する。
- GUI の主要操作は phase 境界で手動確認する。
- OSS 依存が変わるので `THIRD_PARTY_NOTICES.txt` と `docs/OSS_COMPLIANCE.md` の更新確認が必要。
- 一時的な deprecation 回避より、最終的に warning-free を優先する。

## 5. Risks
- Risk: `winit 0.30` 起因で native window geometry が崩れる
  - Impact: 起動位置、multi-display、preview resize の回帰
  - Mitigation: viewport 変更を Phase 2 に隔離し、GUI 手動確認を必須化

- Risk: key enum 変更で shortcut / IME が壊れる
  - Impact: Windows 日本語入力や主要ショートカットの退行
  - Mitigation: Phase 3 で `input.rs` と関連テストを集中更新

- Risk: `egui` widget API 変更の影響範囲が広い
  - Impact: `render.rs` の広範な修正
  - Mitigation: Phase 1 で dependency 更新と最小コンパイル通過だけに絞り、描画修正は Phase 2 に分離

## 6. Execution Strategy

### Phase 1: Dependency Bump and Compile Gate
- Files/modules/components:
  - `rust/Cargo.toml`
  - `rust/Cargo.lock`
  - `THIRD_PARTY_NOTICES.txt`
  - `docs/OSS_COMPLIANCE.md`
- Expected result:
  - `eframe` 系依存が更新される
  - まずはコンパイルエラー一覧を最小化し、追従対象を固定する
- Verification:
  - `cargo test`
  - `cargo clippy --all-targets -- -D warnings`
  - OSS compliance チェック

### Phase 2: Native Window / Viewport Migration
- Files/modules/components:
  - `rust/src/main.rs`
  - `rust/src/app/session.rs`
  - `rust/src/app/mod.rs`
- Expected result:
  - `NativeOptions`, `ViewportBuilder`, `run_native`, geometry restore が新 API に追従
  - 起動/終了/位置復元が維持される
- Verification:
  - `cargo test`
  - GUI 手動確認:
    - 起動 / 終了
    - ウィンドウ位置・サイズ復元
    - multi-display 移動後の再起動

### Phase 3: Render / Input Migration
- Files/modules/components:
  - `rust/src/app/render.rs`
  - `rust/src/app/input.rs`
  - 必要なら `rust/src/app/tests/*.rs`
- Expected result:
  - panel / frame / combo box / keyboard handling が新 API に追従
  - shortcut / IME 関連テストが更新される
- Verification:
  - `cargo test`
  - GUI 手動確認:
    - query 入力
    - IME 入力
    - tab 操作
    - preview resize

### Phase 4: Regression Sweep and Cleanup
- Files/modules/components:
  - 影響を受けた Rust source 一式
  - 関連 docs
- Expected result:
  - warning-free で最終状態に揃う
  - 必要なら `docs/ARCHITECTURE.md` / `docs/EFRAME-UPGRADE-NOTES.md` を更新
- Verification:
  - `cargo test`
  - `cargo clippy --all-targets -- -D warnings`
  - VM-003 perf tests if indexing path changed
  - GUI 手動確認の最終 sweep

## 7. Detailed Task Breakdown

### Phase 1
- [x] 1-1: `eframe` / `egui` 系 version を更新
- [x] 1-2: `Cargo.lock` を再生成
- [x] 1-3: `THIRD_PARTY_NOTICES.txt` を更新
- [x] 1-4: `docs/OSS_COMPLIANCE.md` の確認を反映
- [x] 1-5: `cargo test`
- [x] 1-6: `cargo clippy --all-targets -- -D warnings`

### Phase 2
- [ ] 2-1: `main.rs` の viewport 初期化を更新
- [ ] 2-2: `session.rs` の geometry restore を更新
- [ ] 2-3: compile/test を通す
- [ ] 2-4: GUI 手動確認を記録

### Phase 3
- [ ] 3-1: `render.rs` の deprecated / changed API を更新
- [ ] 3-2: `input.rs` の key handling を更新
- [ ] 3-3: 関連 unit test を更新
- [ ] 3-4: `cargo test`
- [ ] 3-5: GUI 手動確認を記録

### Phase 4
- [ ] 4-1: warning-free 状態へ cleanup
- [ ] 4-2: 必要な docs を同期
- [ ] 4-3: `cargo test`
- [ ] 4-4: `cargo clippy --all-targets -- -D warnings`
- [ ] 4-5: perf tests if needed
- [ ] 4-6: 一時ルールと本計画書を削除

## 8. Validation Plan
- Automated:
  - 各 Phase 完了時に `cargo test`
  - Phase 1 / 4 で `cargo clippy --all-targets -- -D warnings`
  - index 経路に波及した場合は VM-003 perf テスト
- Manual:
  - Phase 2: window/viewport
  - Phase 3: shortcut / IME / preview resize
  - Phase 4: end-to-end sweep

## 9. Rollback Plan
- Phase 1 は依存更新だけなので単独 revert 可能
- Phase 2 と Phase 3 は分けて revert 可能
- 問題が出た場合は dependency bump を維持したまま source 側だけ戻すのではなく、phase 単位で戻す

## 10. Done Criteria
- `eframe` 更新後に `cargo test` と `cargo clippy --all-targets -- -D warnings` が通る
- 必要な GUI 手動確認を完了している
- OSS/compliance 文書が同期している
- `AGENTS.md` の一時ルールとこの change plan を削除して終了できる

## 11. Temporary `AGENTS.md` Rule Draft
- For `eframe-upgrade`, read `docs/CHANGE-PLAN-20260401-eframe-upgrade.md` before starting implementation.
- Execute the work in the documented order (Phase 1 → 2 → 3 → 4) unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Phase 2 以降は `cargo test` に加えて GUI 手動確認を行うこと。
- `Cargo.toml` / `Cargo.lock` を変更するため、OSS compliance docs を同一変更で確認すること。
- Remove this section from `AGENTS.md` after the planned work is complete.
