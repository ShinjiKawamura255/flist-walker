# EFRAME Upgrade Notes

## Scope
- Current: `eframe 0.24.x`
- Investigated target band: `0.25.x` through `0.29.x`
- Goal of this note: 実アップグレード前に、FlistWalker 側の影響箇所と移行順序を固定する

## Release Summary
- `0.25.x`
  - 入力まわりの key enum と text API に breaking change が入っている。
  - 影響候補: keyboard shortcut、IME、text edit 周辺。
- `0.29.x`
  - `winit 0.30` 系へ更新。
  - `NativeOptions::follow_system_theme` / `default_theme` は `egui::Options` 側へ移動。
  - web runner 系の変更はあるが、FlistWalker の native build には直接関係しない。

## FlistWalker Impact Scan
ローカル grep で、アップグレード時に再確認が必要な箇所を洗い出した。

- `NativeOptions`, `ViewportBuilder`, `run_native`
  - [main.rs](/mnt/d/work/flistwalker/rust/src/main.rs)
- `eframe::App` 実装
  - [mod.rs](/mnt/d/work/flistwalker/rust/src/app/mod.rs)
- `TopBottomPanel`, `SidePanel`, `Frame::none`, `Rounding::same`, `Margin::symmetric`
  - [render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs)
- `ComboBox::from_id_source`
  - [render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs)
- shortcut / key handling
  - [input.rs](/mnt/d/work/flistwalker/rust/src/app/input.rs)
- window geometry / viewport 操作
  - [session.rs](/mnt/d/work/flistwalker/rust/src/app/session.rs)
  - [main.rs](/mnt/d/work/flistwalker/rust/src/main.rs)

## Risk Assessment
- High
  - `winit 0.30` 追従で native window / viewport 初期化の修正が入る可能性が高い。
  - 現在の multi-display / window geometry 安定化コードに回帰リスクがある。
- Medium
  - shortcut / IME 周辺の key enum 変更で Windows 入力回りが崩れる可能性がある。
  - `render.rs` の panel / frame builder API に名前変更や deprecation が入る可能性がある。
- Low
  - CLI 側は `eframe` に依存しないため直接影響は限定的。

## Proposed Migration Order
1. `eframe`, `egui`, `egui-winit`, `winit` の changelog 差分を version ごとに再確認する。
2. `main.rs` の `NativeOptions` / `ViewportBuilder` / `run_native` を先に直す。
3. `render.rs` の panel / frame / combo box API を追従する。
4. `input.rs` と IME/shortcut テストを更新する。
5. window geometry 回帰を GUI 手動確認で詰める。

## Recommended Verification
- `cargo test`
- ignored perf tests
  - `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`
  - `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
- GUI 手動確認
  - 起動 / 終了
  - 検索
  - タブ切り替え / ドラッグ
  - preview resize
  - window geometry restore
  - IME 入力

## Effort Estimate
- 調査と最小コンパイル通過: 0.5 から 1 日
- GUI 回帰修正込み: 1 から 2 日
- Windows multi-display 周りで追加調整が出た場合: +0.5 から 1 日

## Recommendation
- `eframe` アップグレードは別 change plan に切り出すべき。
- 先にこの文書の影響箇所を起点に小さな spike branch を作り、`0.24 -> 0.29` を一気に上げるより `0.24 -> 0.25/0.26 -> 0.29` の差分確認を挟む方が安全。
