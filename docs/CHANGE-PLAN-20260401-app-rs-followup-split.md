# CHANGE PLAN: app.rs follow-up split

## Metadata
- Date: 2026-04-01
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: app-rs-followup-split
- Related Tickets/Issues: なし

## 1. Background
- `rust/src/app.rs` は前回の分割で state/session 系の責務を一部外出しできたが、tab lifecycle、index/search orchestration、preview/highlight cache 操作が依然として密集している。
- 現状でも機能上の破綻はないが、変更頻度の高い領域が同居しているため、挙動変更時の影響範囲が広く、レビューと回帰確認のコストが高い。

## 2. Goal
- `FlistWalkerApp` を coordinator として維持しつつ、変更頻度の高いロジックを責務単位で段階的に外出しする。
- `app.rs` の責務を縮小し、tab 管理、index/search pipeline、cache/highlight 操作の境界がコードと docs から読める状態にする。
- 各 Phase を独立に検証・コミットできる粒度で進め、途中段階でも常に `cargo test --locked` green を維持する。

## 3. Scope
### In Scope
- `rust/src/app.rs` に残る tab lifecycle 管理の分離
- `rust/src/app.rs` に残る index/search orchestration の分離
- preview/highlight/cache helper の整理
- 上記に対応する `docs/DESIGN.md` / `docs/TESTPLAN.md` / `docs/TASKS.md` の同期
- 作業期間中のみ有効な `AGENTS.md` 一時ルールの追加と、完了後の削除

### Out of Scope
- 検索仕様、FileList 仕様、UI 操作仕様の変更
- worker protocol や request/response 契約の再設計
- 新規機能追加
- release workflow、CLI 契約、perf workflow の再変更

## 4. Constraints and Assumptions
- 既存の GUI/CLI 挙動互換を維持する。
- `rust/src/app.rs`、`rust/src/app/workers.rs` の index/search 経路に触れる場合は、通常の `cargo test --locked` に加えて ignored perf テスト 2 本を実行する。
- 大規模 rename や module 再配置は、1 Phase で 1 責務に限定し、途中で scope を広げない。
- docs は恒久情報だけを残し、一時計画は完了後に削除する。

## 5. Current Risks
- Risk:
  - 分割の途中で module 間の visibility や依存方向が崩れ、かえって追跡しにくくなる。
  - Impact:
    - compile error の増加、テスト修正の局所化失敗、分割意図の不鮮明化。
  - Mitigation:
    - 1 Phase ごとに責務境界を固定し、公開範囲は `pub(super)` を基本に絞る。
- Risk:
  - index/search pipeline の分割中に incremental refresh や request_id 契約を壊す。
  - Impact:
    - stale response の混入、UI 巻き戻り、perf regression。
  - Mitigation:
    - pipeline Phase では既存 test 群を先に維持し、必要に応じて ignored perf テストも併走させる。
- Risk:
  - cache/highlight の整理で root 切替や filter 切替時の invalidation を壊す。
  - Impact:
    - highlight 不整合、preview の stale 表示、メモリ解放漏れ。
  - Mitigation:
    - cache 破棄条件を整理して docs へ反映し、既存回帰テストを維持する。

## 6. Execution Strategy
1. Phase 1: tab lifecycle split
   - Files/modules/components:
     - `rust/src/app.rs`
     - `rust/src/app/tab_state.rs` または新規 `rust/src/app/tabs.rs`
     - 必要に応じて関連 test module
   - Expected result:
     - tab 初期化、保存、切替、移動、タイトル計算などの責務が `app.rs` から外れ、`app.rs` には coordinator 呼び出しだけが残る。
   - Verification:
     - `cargo test --locked`
2. Phase 2: index/search orchestration split
   - Files/modules/components:
     - `rust/src/app.rs`
     - 新規 `rust/src/app/pipeline.rs` 相当
     - 必要に応じて `rust/src/app/workers.rs`
   - Expected result:
     - index queue、search enqueue/poll、incremental refresh の流れが専用 module に移り、request_id 契約が局所化される。
   - Verification:
     - `cargo test --locked`
     - `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`
     - `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
3. Phase 3: preview/highlight/cache cleanup
   - Files/modules/components:
     - `rust/src/app.rs`
     - `rust/src/app/cache.rs`
     - 必要に応じて新規 helper module
   - Expected result:
     - preview/highlight/cache policy と invalidation 条件が `app.rs` から外れ、cache state と操作の境界が明示される。
   - Verification:
     - `cargo test --locked`
4. Phase 4: docs sync and cleanup
   - Files/modules/components:
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/TASKS.md`
     - `AGENTS.md`
   - Expected result:
     - 恒久 docs が新しい責務境界を反映し、一時ルールと change plan を削除できる状態になる。
   - Verification:
     - docs 整合確認
     - 必要に応じて `cargo test --locked` 再実行

## 7. Detailed Task Breakdown
- [x] P-001 tab lifecycle の責務棚卸しと分割先の確定
- [x] P-002 tab lifecycle の module 分離と test green
- [x] P-003 index/search pipeline の責務棚卸しと分割先の確定
- [x] P-004 index/search pipeline の module 分離と test/perf green
- [ ] P-005 preview/highlight/cache の責務整理と module 分離
- [ ] P-006 docs 同期、一時 `AGENTS.md` ルール削除、change plan 削除

## 8. Validation Plan
- Automated tests:
  - 各 Phase で `cargo test --locked`
  - Phase 2 では ignored perf テスト 2 本を追加実行
- Manual checks:
  - docs の責務記述が code 配置と一致することをレビュー
- Performance or security checks:
  - pipeline Phase で perf regression を確認
- Regression focus:
  - tab restore/switch/close/move
  - stale index/search response の遮断
  - incremental refresh
  - preview/highlight invalidation

## 9. Rollback Plan
- 各 Phase は独立コミットとし、問題があれば該当 Phase の commit を単独で revert 可能にする。
- pipeline 分離で問題が出た場合は、Phase 2 だけを戻して Phase 1 の tab 分離を維持してよい。
- docs cleanup は最終 Phase でまとめ、コード rollback 時に docs だけが先行しないようにする。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-rs-followup-split`, read [docs/CHANGE-PLAN-20260401-app-rs-followup-split.md] before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-01 00:00 Planned.
- 2026-04-01 00:00 Phase 1 completed. Moved tab lifecycle helpers from `rust/src/app.rs` to `rust/src/app/tabs.rs` and verified with `cargo test --locked`.
- 2026-04-01 00:00 Phase 2 completed. Moved index/search queue, poll, and incremental refresh helpers from `rust/src/app.rs` to `rust/src/app/pipeline.rs` and verified with `cargo test --locked` plus the two ignored perf tests.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- `app.rs` の追加分割は行数削減より責務境界の明確化を優先する。
- 各 Phase で「ついでの責務移動」を避け、予定外の分割が必要なら先にこの plan を更新する。
