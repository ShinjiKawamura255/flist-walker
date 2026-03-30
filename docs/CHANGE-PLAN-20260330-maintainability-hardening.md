# CHANGE PLAN: Maintainability Hardening

## Metadata
- Date: 2026-03-30
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: maintainability-hardening
- Related Tickets/Issues: なし

## 1. Background
- 本プロジェクトは、検索仕様、クロスプラットフォーム運用、自己更新、GUI 応答性、回帰防止まで含めて高い完成度に達している。
- 一方で、今後の継続開発を難しくする構造課題が見えている。特に [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) へ状態管理と orchestration が集中しており、機能追加・修正・不具合調査のたびに広範囲の影響確認が必要になる。
- ドキュメント運用にも追跡性の劣化が見られる。`FR-022` の重複、`TC-065` 以降の複数重複、`docs/TASKS.md` が参照する `docs/APP_SPLIT_PLAN.md` の欠落など、ID と参照の信頼性を損ねる状態がある。
- これらは即時の障害ではないが、保守性と判断速度を継続的に下げる種類の負債であり、次の改善サイクルでは新機能より優先して制御する価値がある。

## 2. Goal
- `app.rs` の責務境界をさらに明確化し、主要な変更が局所的に閉じる構造へ寄せる。
- SDD/TDD 文書の ID と参照整合を回復し、文書を一次情報として引き続き信頼できる状態へ戻す。
- テスト運用を「大量にある」状態から「変更対象ごとに適切に回す」状態へ整理し、保守時の判断コストを下げる。
- 変更完了後に、次の条件を満たすことを成功条件とする。
- `app.rs` の役割が「UI 統括と状態遷移の薄い層」に近づいている。
- docs の重複 ID と欠落参照が解消されている。
- 構造変更に対して unit / integration / perf の実行順が明文化されている。

## 3. Scope
### In Scope
- `rust/src/app.rs` と `rust/src/app/*.rs` の責務再棚卸し
- `docs/REQUIREMENTS.md` / `docs/SPEC.md` / `docs/DESIGN.md` / `docs/TESTPLAN.md` の ID 重複と参照不整合の修正
- `docs/TASKS.md` を含む計画・作業記録系 docs の存在確認と整合回復
- 変更対象ごとの検証順序と回帰観点の再整理

### Out of Scope
- 検索仕様そのものの変更
- 新しい UI 機能や更新機能の追加
- release asset 命名規則や自己更新方式の刷新
- ネットワークドライブ最適化や配布インストーラ対応

## 4. Constraints and Assumptions
- UI 応答性ポリシーは最優先で維持し、重い I/O や計算を UI スレッドへ戻さない。
- `request_id` ベースで古い応答を破棄する既存契約は維持する。
- 検索演算子（`'`, `!`, `^`, `$`）の後方互換を壊さない。
- 変更中も `cargo test` を最低限維持し、インデクシング経路に触れた段階では ignored perf test も回す。
- 過去の `app.rs` 分割作業は一部完了している前提だが、`docs/TASKS.md` が参照する `docs/APP_SPLIT_PLAN.md` は現時点で見当たらない。このため、過去計画の存在を前提にした参照は一度棚卸しする。

## 5. Current Risks
- Risk: `app.rs` が状態・UI・永続化・タブ・キャッシュ・更新導線を広く抱えており、修正時の影響面積が大きい
  - Impact: 小変更でも回帰確認範囲が広がり、レビューと不具合調査が遅くなる
  - Mitigation: 状態束を feature 単位へ分け、`app.rs` を orchestration 主体へ寄せる
- Risk: docs の ID 重複と参照欠落により、仕様とテストの対応が信用しづらくなる
  - Impact: 変更時に「どの仕様をどのテストで守るか」の判断が曖昧になる
  - Mitigation: ID 再採番、参照先実在確認、トレース抜粋の更新を同一変更で実施する
- Risk: テスト本数が多くても、変更対象に応じた実行セットが明確でないと検証の質が人依存になる
  - Impact: 必要な perf / integration が抜ける、または不要な全量実行で開発テンポが落ちる
  - Mitigation: 変更種別ごとの検証マトリクスを計画書と `TESTPLAN.md` に残す
- Risk: 既存の作業記録 docs に欠落参照がある
  - Impact: 将来のエージェントや開発者が誤った前提で作業を始める
  - Mitigation: 欠落ファイルの再作成ではなく、まず参照元の意図と実在を確認してから整理する

## 6. Execution Strategy
1. Deep Review and Baseline Freeze
   - Files/modules/components: [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs), [rust/src/app/input.rs](/mnt/d/work/flistwalker/rust/src/app/input.rs), [rust/src/app/render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs), [rust/src/app/session.rs](/mnt/d/work/flistwalker/rust/src/app/session.rs), [rust/src/app/workers.rs](/mnt/d/work/flistwalker/rust/src/app/workers.rs), [docs/TASKS.md](/mnt/d/work/flistwalker/docs/TASKS.md)
   - Expected result: 現在の責務分布、依存方向、残存肥大化ポイント、欠落ドキュメント参照を明文化する
   - Verification: レビュー結果を設計メモまたは計画更新へ反映し、対象境界を合意できる状態にする
2. Documentation Integrity Repair
   - Files/modules/components: [docs/REQUIREMENTS.md](/mnt/d/work/flistwalker/docs/REQUIREMENTS.md), [docs/SPEC.md](/mnt/d/work/flistwalker/docs/SPEC.md), [docs/DESIGN.md](/mnt/d/work/flistwalker/docs/DESIGN.md), [docs/TESTPLAN.md](/mnt/d/work/flistwalker/docs/TESTPLAN.md), [docs/TASKS.md](/mnt/d/work/flistwalker/docs/TASKS.md)
   - Expected result: 重複 ID、欠落参照、トレース抜粋の不整合を解消し、文書同士の参照を回復する
   - Verification: `rg` による重複 ID 検出が解消し、参照される docs が実在する
3. App State Segmentation
   - Files/modules/components: [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs), `rust/src/app/state*.rs` 候補, `rust/src/app/tabs*.rs` 候補, `rust/src/app/filelist*.rs` 候補
   - Expected result: `FlistWalkerApp` のフィールド群を feature ごとの state 構造体へ再編し、`app.rs` の責務を縮小する
   - Verification: `app.rs` のフィールド数と feature 横断更新箇所が減り、既存テストが通る
4. Orchestration Boundary Cleanup
   - Files/modules/components: [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs), [rust/src/app/input.rs](/mnt/d/work/flistwalker/rust/src/app/input.rs), [rust/src/app/render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs), [rust/src/app/workers.rs](/mnt/d/work/flistwalker/rust/src/app/workers.rs)
   - Expected result: 入力処理、描画、worker 連携、filelist dialog/update prompt の state transition を境界化し、`update()` と event handling の追跡性を上げる
   - Verification: 複数 feature にまたがるメソッドが減り、主要フローのユニットテスト追加・修正が局所化される
5. Validation Matrix Hardening
   - Files/modules/components: [docs/TESTPLAN.md](/mnt/d/work/flistwalker/docs/TESTPLAN.md), [AGENTS.md](/mnt/d/work/flistwalker/AGENTS.md)
   - Expected result: 変更種別ごとのテスト実行セットを定義し、通常 test と ignored perf test の切り替え条件を明確にする
   - Verification: 変更内容から必要コマンドを一意に選べる

## 7. Detailed Task Breakdown
- [x] `app.rs` の責務を state / orchestration / lifecycle / feature-specific flow に分類し、残存肥大化箇所を一覧化する
- [x] `docs/REQUIREMENTS.md` の `FR-022` 重複を解消し、後続の trace を再採番する
- [x] `docs/TESTPLAN.md` の `TC-065/066/067/082/083/084` 重複を解消し、関連する Regression Guard と Traceability を更新する
- [x] [docs/TASKS.md](/mnt/d/work/flistwalker/docs/TASKS.md) が参照する欠落ドキュメントの扱いを決め、参照の削除・置換・再作成のいずれかへ整理する
- [x] `FlistWalkerApp` のフィールドを feature 単位に束ねる再編案を設計する
- [x] filelist 作成系、update prompt 系、tab/session 系の state transition を別モジュールへ切り出す順序を確定する
- [ ] 変更種別ごとの検証マトリクスを `TESTPLAN.md` へ反映する
- [ ] フェーズごとに `cargo test` と必要な ignored perf test を実行し、結果を記録する

## 8. Validation Plan
- Automated tests:
  - 各フェーズで `cd rust && cargo test`
  - `rust/src/indexer.rs`、[rust/src/app/workers.rs](/mnt/d/work/flistwalker/rust/src/app/workers.rs)、[rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) のインデクシング経路に触れた場合は ignored perf test 2 本を追加実行
- Manual checks:
  - GUI 起動、query 入力、tab 切替、Create File List、更新ダイアログ抑止、プレビュー表示を最小フローで確認
- Performance or security checks:
  - FileList 初期ロードと Walker 分類が perf 回帰していないこと
  - `.ps1` 実行抑止、root 外パス拒否、自己更新抑止 flag が既存契約のまま維持されること
- Regression focus:
  - current row 維持
  - query focus 中の shortcut
  - background tab と active tab の状態分離
  - stale worker response 無視
  - Create File List cancel と root 変更競合

## 9. Rollback Plan
- docs 整合修正はコード変更と分離して revert 可能にする。
- state 構造の再編は phase ごとに独立コミット可能な単位へ分け、問題が出た phase のみ戻せるようにする。
- worker / indexing 契約に触れた変更は、perf test と既存 unit test が揃って失敗した場合にその phase を丸ごと戻す。
- `app.rs` からの切り出し先が不安定な場合は、機能追加を止めて境界再設計を優先し、半端な中間状態を長く保持しない。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `maintainability-hardening`, read [docs/CHANGE-PLAN-20260330-maintainability-hardening.md] before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-03-30 23:00 Planned after repository review.
- 2026-03-30 23:00 Baseline observations:
- `cargo test` green (`289 passed, 3 ignored`) and CLI integration tests green (`10 passed`).
- [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) remains the largest module at 4761 lines.
- `FR-022` is duplicated in [docs/REQUIREMENTS.md](/mnt/d/work/flistwalker/docs/REQUIREMENTS.md).
- `TC-065/066/067/082/083/084` are duplicated in [docs/TESTPLAN.md](/mnt/d/work/flistwalker/docs/TESTPLAN.md).
- [docs/TASKS.md](/mnt/d/work/flistwalker/docs/TASKS.md) references `docs/APP_SPLIT_PLAN.md`, but that file is currently missing.
- 2026-03-31 Phase 1 baseline freeze:
- `app.rs` の責務は大きく 5 群に残存している: tab state capture/apply、index/search/action/sort/preview/kind/filelist/update の poll 系、preview cache と request routing、Create File List の state transition、worker shutdown と UI state persistence。
- `AppTabState` が query/history/index/result/preview/notice/request tracking を横断して抱えており、feature 単位 state へ再編する余地が大きい。
- `render.rs` / `input.rs` / `session.rs` / `workers.rs` への分離は進んでいるが、`update()` から呼ばれる orchestration 本体は依然として [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) に集中している。
- 欠落参照は `docs/TASKS.md` 内の `docs/APP_SPLIT_PLAN.md` に限定して確認でき、次フェーズでは再作成ではなく参照元整理を優先する。
- フェーズ順は計画どおり維持する。次は docs integrity repair を先行し、その後に state segmentation へ進む。
- 2026-03-31 Phase 2 documentation integrity repair:
- `FR-022` 重複は `FR-022/023/024` へ再採番し、`.ps1` 実行抑止 / macOS manual-only / 更新ダイアログ抑止の責務を分離した。
- `TC-065/066/067/082/083/084` の重複は `TC-087` 以降へ再採番し、Regression Guard / DESIGN / RELEASE / `rust/README.md` の参照も追随させた。
- [docs/TASKS.md](/mnt/d/work/flistwalker/docs/TASKS.md) の欠落参照は、存在しない `docs/APP_SPLIT_PLAN.md` の再作成ではなく、一般化した計画 docs 参照表現へ置換して整理した。
- 検証として `cd rust && cargo test` を実行し、`289 passed, 3 ignored` と CLI integration `10 passed` を確認した。
- 2026-03-31 Phase 3 app state segmentation:
- 新規 [rust/src/app/state.rs](/mnt/d/work/flistwalker/rust/src/app/state.rs) を追加し、Create File List 系 state を `FileListWorkflowState`、自己更新系 state を `UpdateState` へ束ねた。
- [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) のトップレベル field から filelist / update の request tracking・dialog state・progress flag を外し、feature 境界を明示した。
- [rust/src/app/tests/app_core.rs](/mnt/d/work/flistwalker/rust/src/app/tests/app_core.rs) と [rust/src/app/tests/index_pipeline.rs](/mnt/d/work/flistwalker/rust/src/app/tests/index_pipeline.rs) は新しい state 束に追随させ、既存回帰観点を維持した。
- 検証として `cd rust && cargo test`、`cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`、`cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture` を実行し、通常 test / perf test ともに green を確認した。
- 2026-03-31 Phase 4 orchestration boundary cleanup:
- 新規 [rust/src/app/filelist.rs](/mnt/d/work/flistwalker/rust/src/app/filelist.rs) と [rust/src/app/update.rs](/mnt/d/work/flistwalker/rust/src/app/update.rs) を追加し、Create File List / update prompt の state transition と worker response 処理を `app.rs` から分離した。
- [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) は index/search など横断 orchestration を残しつつ、filelist / update の詳細遷移を子モジュールへ委譲する形へ寄せた。
- 既存の [rust/src/app/input.rs](/mnt/d/work/flistwalker/rust/src/app/input.rs) と [rust/src/app/render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs) からは、同じメソッド名のまま分離後の実装を呼び続けられることを確認した。
- 検証として `cargo fmt`、`cd rust && cargo test`、`cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`、`cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture` を実行し、通常 test / perf test ともに green を確認した。

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- まず直すべきは docs の ID / 参照整合であり、その後に `app.rs` の state segmentation を進める。
- `app.rs` の行数だけを KPI にせず、feature ごとの変更が局所化されるかを主指標にする。
- 過去の分割計画の参照欠落は、闇雲に補完せず、現存 docs に統合できるならそちらを優先する。
