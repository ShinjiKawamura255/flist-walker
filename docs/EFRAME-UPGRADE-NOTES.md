# EFRAME Upgrade Notes

## Scope
- Current: `eframe 0.29.1`
- Investigated target band: `0.25.x` through `0.29.x`
- Goal of this note: `0.24.x -> 0.29.1` 移行で実際に影響した箇所と、残る確認ポイントを残す

## Release Summary
- `0.25.x`
  - 入力まわりの key enum と text API に breaking change が入っている。
  - 実影響: keyboard shortcut、IME、text edit 周辺の追従が必要だった。
- `0.29.x`
  - `winit 0.30` 系へ更新。
  - `NativeOptions::follow_system_theme` / `default_theme` は `egui::Options` 側へ移動。
  - root viewport 初期化と `run_native` の app creator 戻り型追従が必要だった。
  - web runner 系の変更はあるが、FlistWalker の native build には直接関係しない。

## FlistWalker Impact Scan
実アップグレードで影響が出た箇所。

- `NativeOptions`, `ViewportBuilder`, `run_native`
  - [main.rs](/mnt/d/work/flistwalker/rust/src/main.rs)
  - `ViewportBuilder` を helper 化し、title/app_id/min size/restore geometry をテストで固定した
- `eframe::App` 実装
  - [mod.rs](/mnt/d/work/flistwalker/rust/src/app/mod.rs)
- `TopBottomPanel`, `SidePanel`, `Frame::none`, `Rounding::same`, `Margin::symmetric`
  - [render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs)
- `ComboBox::from_id_source`
  - [render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs)
  - `from_id_salt` へ更新済み
- shortcut / key handling
  - [input.rs](/mnt/d/work/flistwalker/rust/src/app/input.rs)
  - `ImeEvent`, `physical_key`, `TextEditState::cursor` API へ更新済み
- window geometry / viewport 操作
  - [session.rs](/mnt/d/work/flistwalker/rust/src/app/session.rs)
  - [main.rs](/mnt/d/work/flistwalker/rust/src/main.rs)
  - geometry capture/restore ロジックは既存実装を維持、startup viewport 構築だけ明示化

## Risk Assessment
- High
  - `winit 0.30` 追従で native window / viewport 初期化の修正が必要だった。
  - multi-display / window geometry の最終確認は GUI compositor がある環境で継続して必要。
- Medium
  - shortcut / IME 周辺の key enum 変更で Windows 入力回りが崩れる可能性がある。
  - `render.rs` の panel / frame builder API に名前変更や deprecation が入る可能性がある。
- Low
  - CLI 側は `eframe` に依存しないため直接影響は限定的。

## Executed Migration Order
1. `eframe 0.29.1` へ依存更新し、compile blocker になった `main.rs` / `render.rs` / `input.rs` を先に追従した。
2. IME / shortcut / text cursor まわりの unit test を `egui 0.29` API に合わせて更新した。
3. root viewport 初期化を helper 化し、restore geometry と icon 適用を test で固定した。
4. `cargo test` と `cargo clippy --all-targets -- -D warnings` を通した。
5. GUI 手動確認は compositor 不在のため未完了。

## Recommended Verification
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- ignored perf tests
  - index 経路変更時のみ実施
- GUI 手動確認
  - 起動 / 終了
  - 検索
  - タブ切り替え / ドラッグ
  - preview resize
  - window geometry restore
  - IME 入力

## Remaining Verification
- compositor がある Linux / Windows 環境で以下を手動確認する
  - window geometry restore
  - multi-display 移動後の再起動
  - IME 入力
  - preview resize
