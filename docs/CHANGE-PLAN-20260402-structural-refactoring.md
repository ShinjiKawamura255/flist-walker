# CHANGE PLAN: FlistWalker 構造的リファクタリング

## Metadata
- Date: 2026-04-02
- Owner: Agent + User
- Target Project: FlistWalker (rust/)
- Scope Label: structural-refactoring
- Related Tickets/Issues: 評価レポート (2026-04-02) の P0/P1/P2 改善提案

## 1. Background

2026-04-02 のプロジェクト評価で以下の構造的問題が指摘された。

1. **テスト失敗 (P0)**: 2026-04-02 の評価時点では `cargo test --lib` で 2 件の panic が報告されていたが、現行ブランチの baseline 再確認では再現せず green だった。Phase 0 では failure 修正ではなく baseline 再確認と計画同期を行う。
2. **God Object `FlistWalkerApp`**: 1519 行 / 50+ フィールド / 70+ メソッド。分割は「ファイルの分割」に留まり「責務の分離」に至っていない。
3. **Glob import の不透明化**: `use cache::*` 等 14 本の wildcard re-export。シンボル由来が不明。
4. **ドメインモデルの欠如**: `Vec<PathBuf>` + `HashMap<PathBuf, EntryKind>` + `HashMap<PathBuf, SortMetadata>` の 3 重同期管理。
5. **`pub(super)` の過剰使用**: 構造体フィールドの不変条件が保護されていない。
6. **不要な clone**: `all_entries.as_ref().clone()` 等でフレーム毎に大量コピーの可能性。
7. **コードコメント / doc の希薄さ**: 関数の意図・不変条件が読者に伝わらない。
8. **テストファイル巨大化**: `index_pipeline.rs` 1626 行。

## 2. Goal

- cargo test --lib が 0 failures で通過する (main green)。
- `FlistWalkerApp` の直接フィールド数を 50+ → 25 以下に削減し、各サブシステムが独立した struct で不変条件を保護する。
- 全 glob import を明示 import に置き換え、シンボルのトレーサビリティを回復する。
- `Entry` ドメインモデルを導入し、3 重 HashMap 同期を解消する。
- テストの構造を改善し、機能単位の分割と doc コメントを追加する。
- 既存の検索性能・UI 応答性を劣化させない。

## 3. Scope

### In Scope
- `rust/src/app/mod.rs` の構造分割とフィールド集約
- テスト失敗の原因調査と修正
- Glob import → 明示 import への全面置換
- `Entry` ドメイン型の導入と段階的移行
- `pub(super)` の最小化とアクセサ導入
- 不要な clone の特定と修正
- 主要 public method への doc コメント追加
- テストファイルの機能単位分割
- docs/ の該当箇所更新 (ARCHITECTURE.md, DESIGN.md)

### Out of Scope
- 新機能追加
- コマンドパターン導入 (将来フェーズへ)
- プロパティベーステスト導入 (将来フェーズへ)
- インテグレーションテスト基盤構築
- パフォーマンス最適化 (劣化防止は Scope In)

## 4. Constraints and Assumptions

- **後方互換**: GUI の操作仕様・キーバインド・表示を変更しない。
- **性能基準**: perf regression テスト (`perf_regression_filelist_stream_matches_v0123_reference_budget`, `perf_walker_classification_is_faster_than_eager_metadata_resolution`) をリファクタ後も通過する。
- **AGENTS.md 遵守**: UI 応答性ポリシー (§3)、実装ガードレール (§5)、ドキュメントプロセス (§6) に従う。
- **段階的移行**: 各フェーズ完了時に `cargo test --lib` green を維持する。中間状態でもビルド・テストは通る。
- **作業環境**: Windows (pwsh) でテスト実行。WSL/Linux 向け検証は CI に委ねる。

## 5. Current Risks

- Risk: God Object 分割中にフィールド参照の取り違いで runtime regression が発生する。
  - Impact: テスト通過しても GUI で操作不能になる。
  - Mitigation: 各フェーズで手動 GUI smoke test を実施。フェーズ粒度を細かく保ち、巻き戻しコストを最小化。

- Risk: `Entry` 型移行中にパフォーマンス劣化。
  - Impact: 10 万件規模で検索応答が 100ms を超える。
  - Mitigation: perf regression テストを各フェーズ末に実行。`Entry` 移行は search hot path の最後に行う。

- Risk: glob import 排除で見落とした re-export があり、外部テストが壊れる。
  - Impact: CI red。
  - Mitigation: `cargo test --lib` + `cargo build` を各変更後に確認。

## 6. Execution Strategy

### Phase 0: テスト失敗の修正 (P0)
- Files/modules/components:
  - `rust/src/app/tests/index_pipeline.rs`
  - `rust/src/app/filelist.rs` (実装側に原因がある場合)
- Expected result:
  - `cargo test --lib` green baseline を再確認し、Phase 1 以降の前提を計画書へ反映する
- Verification:
  - `cargo test --lib` green
  - 失敗報告のあった 2 テストが現行ブランチで個別に pass

### Phase 1: Glob import の排除
- Files/modules/components:
  - `rust/src/app/mod.rs` (import 宣言)
  - `rust/src/app/state.rs`, `cache.rs`, `workers.rs`, `index_coordinator.rs`, `search_coordinator.rs`, `tab_state.rs`, `session.rs`, `input.rs`, `render.rs`
- Expected result:
  - 全 `use xxx::*` を `use xxx::{Specific, Items}` に置換
  - `#[allow(unused_imports)]` の除去
- Verification:
  - `cargo build` + `cargo test --lib` green
  - grep で `use.*::\*` が app/ 内に残っていないことを確認 (`use super::*` はサブモジュール内で許容するかを Phase 1 で判断)

### Phase 2: Worker チャネル群の `WorkerBus` 集約
- Files/modules/components:
  - `rust/src/app/worker_bus.rs` (新規)
  - `rust/src/app/mod.rs` (`FlistWalkerApp` フィールド削減)
  - `rust/src/app/bootstrap.rs` (WorkerBus 生成)
  - `rust/src/app/pipeline.rs`, `filelist.rs`, `update.rs`, `cache.rs` (アクセスパス変更)
- Expected result:
  - preview/action/sort/kind/filelist/update の 6 ペア (12 フィールド) + request_id/in_progress (12 フィールド) を `WorkerBus` に集約
  - `FlistWalkerApp` のフィールドが 24 個削減
- Verification:
  - `cargo test --lib` green
  - `FlistWalkerApp` struct のフィールド行数を計測 (目標: 70 行以下)

### Phase 3: UI 状態の `UiState` 集約
- Files/modules/components:
  - `rust/src/app/ui_state.rs` (新規)
  - `rust/src/app/mod.rs` (フィールド移動)
  - `rust/src/app/render.rs`, `input.rs`, `session.rs` (アクセスパス変更)
- Expected result:
  - runtime 側の `scroll_to_current`, `focus_query_requested`, `unfocus_query_requested`, `preview_resize_in_progress`, `pending_copy_shortcut`, `root_dropdown_highlight`, `tab_drag_state`, `ime_composition_active`, `prev_space_down`, `query_input_id` 等の UI フラグを `ui_state.rs` の新 struct に集約
  - `session.rs` に既存の永続化用 `UiState` があるため、Phase 3 の runtime struct 名は衝突回避のため別名 (`RuntimeUiState` 想定) を採用してよい
- Verification:
  - `cargo test --lib` green
  - GUI smoke test: タブ切替、検索、プレビュー、ピン留め

### Phase 4: Query/History 状態の `QueryState` 集約
- Files/modules/components:
  - `rust/src/app/query_state.rs` (新規)
  - `rust/src/app/mod.rs`
  - `rust/src/app/input.rs`, `pipeline.rs`, `tabs.rs`
- Expected result:
  - `query`, `query_history`, `query_history_cursor`, `query_history_draft`, `query_history_dirty_since`, `history_search_*`, `kill_buffer` を `QueryState` に集約
  - QueryState が履歴操作の不変条件 (最大 100 件、dirty tracking) を内部で保証
- Verification:
  - `cargo test --lib` green
  - テスト: `tests/query_history.rs` の全テスト pass

### Phase 5: `Entry` ドメインモデルの導入
- Files/modules/components:
  - `rust/src/entry.rs` (新規: `Entry` 構造体)
  - `rust/src/indexer.rs` (Entry 返却への段階移行)
  - `rust/src/search.rs` (Entry ベースの入力)
  - `rust/src/app/mod.rs`, `state.rs`, `pipeline.rs`, `cache.rs` (HashMap 同期の解消)
- Expected result:
  - `Vec<PathBuf>` + `HashMap<PathBuf, EntryKind>` + `HashMap<PathBuf, SortMetadata>` → `Vec<Entry>` に統合
  - `Entry { path: PathBuf, kind: Option<EntryKind>, sort_metadata: Option<SortMetadata> }`
  - 3 重同期管理の解消
- Verification:
  - `cargo test --lib` green
  - perf regression テスト通過
  - search/indexer のユニットテスト更新

### Phase 6: `pub(super)` の最小化とアクセサ導入
- Files/modules/components:
  - `rust/src/app/state.rs`, `index_coordinator.rs`, `search_coordinator.rs`, `tab_state.rs`, `cache.rs`
  - Phase 2-4 で導入した新 struct (`WorkerBus`, `UiState`, `QueryState`)
- Expected result:
  - フィールドは `pub(super)` → private に変更
  - 構造体が自身の不変条件を method で保証 (e.g., `IndexCoordinator::advance_request_id()`)
  - 外部からの直接フィールドアクセスは getter/setter 経由
- Verification:
  - `cargo test --lib` green
  - `grep -c "pub(super)" rust/src/app/` の件数が Phase 開始前の半分以下

### Phase 7: 不要な clone の削減
- Files/modules/components:
  - `rust/src/app/mod.rs` (L1257-1268 周辺の `clone()`)
  - `rust/src/app/pipeline.rs` (entries snapshot の clone)
  - `rust/src/app/tabs.rs` (tab snapshot の clone)
- Expected result:
  - `Arc<Vec<PathBuf>>` → iterator / slice 参照に置き換え可能な箇所を修正
  - フレームごとの不要なヒープ確保を排除
- Verification:
  - `cargo test --lib` green
  - perf regression テスト通過

### Phase 8: Doc コメント追加とテストファイル分割
- Files/modules/components:
  - `rust/src/app/mod.rs` — `FlistWalkerApp` の公開メソッドと重要な private メソッドに `///` doc コメント
  - `rust/src/app/tests/index_pipeline.rs` → `tests/index_filelist.rs`, `tests/index_walker.rs`, `tests/index_batch.rs` に分割
  - `docs/ARCHITECTURE.md` — リンク修正、新 module への追記
  - `docs/DESIGN.md` — DES-009 の構造記述を更新
- Expected result:
  - `FlistWalkerApp` の主要メソッドに意図・不変条件の doc コメント
  - テストファイルが機能単位に分割され、各 800 行以下
  - ARCHITECTURE.md のリンクが相対パスで有効
- Verification:
  - `cargo test --lib` green
  - `cargo doc --no-deps` — warning ゼロ
  - docs 手動レビュー

### Phase 9: 最終検証とクリーンアップ
- Files/modules/components:
  - 全ソースファイル
- Expected result:
  - `FlistWalkerApp` 直接フィールド数 25 以下
  - `cargo clippy -- -D warnings` clean
  - perf regression テスト通過
  - GUI smoke test: 全主要操作の動作確認
- Verification:
  - `cargo test --lib` — 全テスト green
  - `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`
  - `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
  - `cargo clippy -- -D warnings`
  - GUI 手動テスト (タブ、検索、FileList 作成、プレビュー、自己更新ダイアログ、セッション復元)

## 7. Detailed Task Breakdown

### Phase 0
- [x] P0-1: panic 報告のあった 2 テストを再確認し、現行ブランチでは再現しないことを確認
- [x] P0-2: Phase 0 の前提を現状の baseline に合わせて計画書へ反映
- [x] P0-3: `cargo test --lib` green を確認

### Phase 1
- [x] P1-1: `mod.rs` の glob import を明示 import に置換
- [x] P1-2: サブモジュール内の `use super::*` は後続 Phase 対象と判断し、Phase 1 では `mod.rs` 依存の明示化に限定
- [x] P1-3: `mod.rs` の `#[allow(unused_imports)]` を除去
- [x] P1-4: `cargo build` + `cargo test --lib` green

### Phase 2
- [x] P2-1: `WorkerBus` struct を設計 (preview/action/sort/kind/filelist/update のチャネルペアと request_id/in_progress)
- [x] P2-2: `worker_bus.rs` を新規作成
- [x] P2-3: `FlistWalkerApp` から該当フィールドを移動
- [x] P2-4: `bootstrap.rs` を `WorkerBus` 生成に対応
- [x] P2-5: `pipeline.rs`, `filelist.rs`, `update.rs`, `cache.rs`, `render.rs` のアクセスパスを更新
- [x] P2-6: テスト内の直接フィールドアクセスを更新
- [x] P2-7: `cargo test --lib` green

### Phase 3
- [x] P3-1: `UiState` struct を設計
- [x] P3-2: `ui_state.rs` を新規作成
- [x] P3-3: `FlistWalkerApp` から UI フラグを移動
- [x] P3-4: `render.rs`, `input.rs` のアクセスパスを更新
- [x] P3-5: テスト更新
- [x] P3-6: `cargo test --lib` green
- [x] P3-7: GUI smoke test は headless で実施不能なため、Phase 9 の手動確認へ集約

### Phase 4
- [ ] P4-1: `QueryState` struct を設計 (履歴管理の不変条件を encapsulate)
- [ ] P4-2: `query_state.rs` を新規作成
- [ ] P4-3: `FlistWalkerApp` から query/history フィールドを移動
- [ ] P4-4: `input.rs`, `pipeline.rs`, `tabs.rs` のアクセスパスを更新
- [ ] P4-5: テスト更新
- [ ] P4-6: `cargo test --lib` green

### Phase 5
- [ ] P5-1: `Entry` struct を `rust/src/entry.rs` に設計
- [ ] P5-2: `indexer.rs` の返却値を `Entry` ベースに段階移行
- [ ] P5-3: `search.rs` の入力を `Entry` 対応に更新
- [ ] P5-4: `app/mod.rs` の `all_entries`, `entries`, `entry_kinds` を `Vec<Entry>` に統合
- [ ] P5-5: `pipeline.rs`, `cache.rs` の HashMap 同期コードを除去
- [ ] P5-6: テスト更新
- [ ] P5-7: `cargo test --lib` green + perf regression テスト

### Phase 6
- [ ] P6-1: Phase 2-4 の新 struct のフィールドを private 化
- [ ] P6-2: `IndexCoordinator`, `SearchCoordinator` のフィールドを private 化 + メソッド追加
- [ ] P6-3: `state.rs`, `tab_state.rs` の `pub(super)` を最小化
- [ ] P6-4: テスト内の直接アクセスをメソッド経由に置き換え
- [ ] P6-5: `cargo test --lib` green

### Phase 7
- [ ] P7-1: `all_entries.as_ref().clone()` の使用箇所を特定 (grep)
- [ ] P7-2: iterator 化またはスライス参照化で不要 clone を排除
- [ ] P7-3: `cargo test --lib` green + perf regression テスト

### Phase 8
- [ ] P8-1: `FlistWalkerApp` の主要メソッドに doc コメントを追加 (最低 30 メソッド)
- [ ] P8-2: `index_pipeline.rs` を機能単位に分割 (各 800 行以下)
- [ ] P8-3: `ARCHITECTURE.md` のリンクを相対パスに修正 + 新モジュール追記
- [ ] P8-4: `DESIGN.md` の DES-009 を更新
- [ ] P8-5: `cargo doc --no-deps` warning ゼロ

### Phase 9
- [ ] P9-1: `FlistWalkerApp` フィールド数の最終計測 (目標: 25 以下)
- [ ] P9-2: `cargo clippy -- -D warnings` clean
- [ ] P9-3: perf regression テスト (2 本) 通過
- [ ] P9-4: GUI smoke test (全主要操作)
- [ ] P9-5: 一時ルールの削除と計画書の削除

## 8. Validation Plan

- Automated tests:
  - 各フェーズ末: `cargo test --lib` (333+ tests, 0 failed)
  - Phase 5, 7, 9: perf regression テスト (ignored) を `--nocapture` で実行
  - Phase 9: `cargo clippy -- -D warnings`
- Manual checks:
  - Phase 3, 9: GUI smoke test (タブ操作、検索、FileList 作成、プレビュー、self-update ダイアログ)
- Performance checks:
  - `perf_regression_filelist_stream_matches_v0123_reference_budget`
  - `perf_walker_classification_is_faster_than_eager_metadata_resolution`
- Regression focus:
  - FileList 作成ワークフロー (Phase 0 で壊れていた箇所)
  - タブ切替時の状態同期 (Phase 2-4 でアクセスパスが変わる)
  - 検索応答性 (Phase 5, 7 でデータ構造が変わる)

## 9. Rollback Plan

- 各フェーズは独立した git commit (またはブランチ) で管理する。
- フェーズ N で regression が見つかった場合、フェーズ N の commit を revert して N-1 の状態に戻す。
- Phase 5 (Entry 型) は最もリスクが高いため、Phase 5 開始前に tag `pre-entry-refactor` を打つ。
- データ・設定への影響: なし (ユーザ向けのファイル形式変更はない)。

## 10. Temporary `AGENTS.md` Rule Draft

```md
## Temporary Change Plan Rule
- For `structural-refactoring`, read `docs/CHANGE-PLAN-20260402-structural-refactoring.md` before starting implementation.
- Execute the work in the documented order (Phase 0 → Phase 9) unless the plan is updated first.
- Each Phase MUST end with `cargo test --lib` green. Do not proceed to the next Phase with test failures.
- If scope, order, or risk changes, update the change plan before continuing.
- When modifying `rust/src/app/mod.rs` field layout, verify the resulting field count against the Phase target.
- Remove this section from `AGENTS.md` after Phase 9 completion checklist is satisfied.
```

## 11. Progress Log
- 2026-04-02 20:19 Planned.
- 2026-04-02 Phase 0 completed. `cargo test --lib` は現行ブランチで green（327 passed, 0 failed, 3 ignored）だったため、failure 修正ではなく baseline 再確認と計画同期のみを実施。
- 2026-04-02 Phase 1 completed. `rust/src/app/mod.rs` の wildcard import を明示 import へ置換し、test 専用 helper import は `rust/src/app/tests/app_core.rs` へ局所化した。`cargo build` と `cargo test --lib` を実行済み。
- 2026-04-02 Phase 2 completed. `rust/src/app/worker_bus.rs` を追加し、preview/action/sort/kind/filelist/update の worker channel と app-level request/in-progress state を `WorkerBus` へ集約した。`bootstrap.rs` と app/tests 一式のアクセス経路を更新し、`cargo test --lib` は green（327 passed, 0 failed, 3 ignored）。`FlistWalkerApp` の直接フィールド数は 68 行まで減少。
- 2026-04-02 Phase 3 着手前に計画更新。`session.rs` に永続化用 `UiState` が既に存在するため、runtime 側の UI 集約 struct は `RuntimeUiState` などの別名で導入する方針へ修正。
- 2026-04-02 Phase 3 completed. `rust/src/app/ui_state.rs` に `RuntimeUiState` を追加し、focus/scroll/IME/preview panel/window geometry debounce/tab drag などの runtime UI 状態を `self.ui` へ集約した。`render.rs` / `input.rs` / `session.rs` / `tabs.rs` / tests を更新し、`cargo test --lib` は green（327 passed, 0 failed, 3 ignored）。`FlistWalkerApp` の直接フィールド数は 50 行まで減少。GUI smoke は headless のため Phase 9 へ集約。

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.
- Phase 依存関係: P0 は独立。P1 → P2 → P3 → P4 は順序依存 (フィールド移動が累積)。P5 は P4 完了後。P6 は P2-P4 完了後。P7 は P5 完了後。P8 は P6 完了後。P9 は全フェーズ完了後。
- Phase 5 の Entry 型移行は最大のリスクポイント。indexer → search → app の順で段階的に移行し、各ステップで既存テストを維持する。
