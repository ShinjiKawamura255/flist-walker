# CHANGE PLAN: App Core Segmentation

## Metadata
- Date: 2026-03-31
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: app-core-segmentation
- Related Tickets/Issues: なし

## 1. Background
- `maintainability-hardening` により filelist / update 系の state transition は [rust/src/app/filelist.rs](/mnt/d/work/flistwalker/rust/src/app/filelist.rs) と [rust/src/app/update.rs](/mnt/d/work/flistwalker/rust/src/app/update.rs) へ分離されたが、[rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) は依然として 4131 行あり、主要な orchestration と state 定義が集中している。
- とくに `AppTabState` が query history、履歴検索、検索結果、preview、kind 解決、sort 状態まで横断して保持しており、タブまわりの小変更でも `app.rs` の広い範囲を読まないと安全に触れない。
- `new_with_launch` も worker 起動、初期 state 構築、launch/session 復元を一度に行っており、起動時の仕様追加や回帰調査の影響面が広い。
- cache 群（preview / highlight / sort metadata）も `app.rs` に残っており、表示都合の state と検索 orchestration の責務が混線している。

## 2. Goal
- `app.rs` を「最終的な app coordinator」に近づけ、feature ごとの state と helper を子モジュールへ段階分離する。
- 行数削減そのものではなく、変更時に読むべき責務範囲を局所化する。
- 変更完了後に、次の状態を成功条件とする。
- `AppTabState` の横断責務が複数 struct に分離され、query/history/result/preview-kind の境界が明示されている。
- app 起動/bootstrap の処理が専用モジュールまたは builder に退避し、`new_with_launch` の直接初期化量が減っている。
- cache と sort/query-history の helper 群が `app.rs` 直下から外れ、回帰テストが既存契約を維持している。

## 3. Scope
### In Scope
- [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) の残存 state 定義と helper の再棚卸し
- `AppTabState` の内部責務分割
- launch/bootstrap と worker runtime 初期化の境界切り出し
- preview/highlight/sort metadata cache、および query history / history search の補助 state 切り出し
- 上記変更に伴う [docs/DESIGN.md](/mnt/d/work/flistwalker/docs/DESIGN.md) と [docs/TESTPLAN.md](/mnt/d/work/flistwalker/docs/TESTPLAN.md) の更新

### Out of Scope
- 検索演算子や ranking 契約の変更
- FileList / updater の新機能追加
- 新たな GUI 機能追加やレイアウト変更
- インデクシングアルゴリズムそのものの刷新

## 4. Constraints and Assumptions
- UI 応答性ポリシーを維持し、重い I/O や計算は UI スレッドに戻さない。
- `request_id` による stale response 破棄契約は維持する。
- 検索演算子（`'`, `!`, `^`, `$`, `|`）と query history のユーザ観測挙動は変えない。
- `rust/src/app.rs` と `rust/src/app/workers.rs` に触れるため、通常 `cargo test` に加えて VM-003 の ignored perf test 実行が必要になる可能性が高い。
- 既存の `app/filelist.rs`、`app/update.rs`、`app/session.rs`、`app/state.rs` との責務境界を壊さず、重複 state を生まないようにする。

## 5. Current Risks
- Risk: `AppTabState` の過密化により、tab / history / result / preview の変更が相互干渉しやすい
  - Impact: 小変更でも `capture_active_tab_state` / `apply_tab_state` / tab restore 系の回帰確認が広がる
  - Mitigation: tab state を query-history、result-selection、preview-kind などの束へ分割する
- Risk: `new_with_launch` の肥大化により、起動時回帰の原因追跡が難しい
  - Impact: worker 起動順、saved state 復元、初期 request 発火の変更が一箇所に集中してレビューしづらい
  - Mitigation: launch/bootstrap builder を導入し、worker wiring と state seed を分離する
- Risk: cache helper が `app.rs` に残ることで、表示最適化と検索/ソート制御の責務が混ざる
  - Impact: preview/highlight/sort cache の変更が unrelated な app flow と混線する
  - Mitigation: cache state と cache helper を専用 module へ切り出す
- Risk: 分割途中で tab restore や query history 永続化契約を崩す
  - Impact: 起動直後の tab 復元、履歴保存、background tab lazy refresh の回帰
  - Mitigation: 既存 TC-039/040/041/045/054/060/064 を軸にテストを先に追随させる

## 6. Execution Strategy
1. Deep Review and Boundary Definition
   - Files/modules/components: [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs), [rust/src/app/session.rs](/mnt/d/work/flistwalker/rust/src/app/session.rs), [rust/src/app/state.rs](/mnt/d/work/flistwalker/rust/src/app/state.rs), [docs/DESIGN.md](/mnt/d/work/flistwalker/docs/DESIGN.md)
   - Expected result: `app.rs` に残る責務を tab state / bootstrap / cache / polling helper に分類し、分離順序を固定する
   - Verification: 計画書の progress log に境界整理結果を反映し、着手順を確定する
2. Tab State Segmentation
   - Files/modules/components: [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs), `rust/src/app/tab_state.rs` 候補, [rust/src/app/tests/app_core.rs](/mnt/d/work/flistwalker/rust/src/app/tests/app_core.rs), [rust/src/app/tests/session_tabs.rs](/mnt/d/work/flistwalker/rust/src/app/tests/session_tabs.rs)
   - Expected result: `AppTabState` を少なくとも query/history、result/selection、preview/kind の束へ分け、capture/apply/restore を局所化する
   - Verification: tab restore / tab move / query history 系テストが通り、`AppTabState` の field 群が縮小する
3. Bootstrap and Worker Wiring Cleanup
   - Files/modules/components: [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs), `rust/src/app/bootstrap.rs` 候補, [rust/src/app/workers.rs](/mnt/d/work/flistwalker/rust/src/app/workers.rs)
   - Expected result: `new_with_launch` の worker 起動、launch 復元、初期 state seed を専用 builder/helper へ切り出す
   - Verification: app 起動経路のテストが通り、`new_with_launch` の direct field 初期化量が減る
4. Cache and Helper Segmentation
   - Files/modules/components: [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs), `rust/src/app/cache.rs` 候補, [rust/src/app/render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs)
   - Expected result: preview/highlight/sort metadata cache と関連 helper を専用 module へ寄せる
   - Verification: preview / highlight / sort regression が既存 test で維持され、`app.rs` の cache helper 群が減る
5. Documentation and Validation Matrix Refresh
   - Files/modules/components: [docs/DESIGN.md](/mnt/d/work/flistwalker/docs/DESIGN.md), [docs/TESTPLAN.md](/mnt/d/work/flistwalker/docs/TESTPLAN.md), [AGENTS.md](/mnt/d/work/flistwalker/AGENTS.md)
   - Expected result: 新しい責務境界と必要検証セットを docs に追記し、作業完了後に一時ルールを除去できる
   - Verification: docs が実装後の構造と一致し、計画の完了条件を満たす

## 7. Detailed Task Breakdown
- [ ] `app.rs` の残存責務を tab state / bootstrap / cache / polling helper に分類し、分離順序を確定する
- [ ] `AppTabState` の field 群を feature 単位 struct へ切り出す設計を作る
- [ ] tab capture/apply/restore の実装とテストを新しい tab state 束へ追随させる
- [ ] `new_with_launch` の worker wiring と launch/session seed を別 module または builder へ切り出す
- [ ] preview/highlight/sort metadata cache とその helper を `app.rs` から移す
- [ ] `docs/DESIGN.md` と `docs/TESTPLAN.md` に新しい責務境界と回帰観点を反映する
- [ ] フェーズごとに `cargo test` と必要な ignored perf test を実行し、結果を記録する

## 8. Validation Plan
- Automated tests:
  - 各コード変更フェーズで `cd rust && cargo test`
  - `rust/src/app.rs` または [rust/src/app/workers.rs](/mnt/d/work/flistwalker/rust/src/app/workers.rs) の indexing path に影響する場合は VM-003 として ignored perf test 2 本を追加実行
- Manual checks:
  - GUI 起動、tab 復元、tab 切替、query history (`Ctrl+R` 含む)、preview 表示、sort 切替を最小フローで確認
- Performance or security checks:
  - index/search の stale response 破棄が維持されること
  - FileList / Walker の初期 index perf が既存基準を下回らないこと
- Regression focus:
  - TC-039 / TC-040 / TC-041 / TC-045 / TC-054 / TC-060 / TC-064
  - current row 維持
  - query focus 中 shortcut
  - background tab lazy refresh
  - preview cache / highlight cache の無制限増加防止

## 9. Rollback Plan
- tab state 分割、bootstrap 分割、cache 分割はフェーズごとに独立コミットにし、問題が出たフェーズだけ戻せるようにする。
- `new_with_launch` と worker wiring を跨ぐ変更で起動回帰が出た場合は、そのフェーズを丸ごと revert して tab state 分割とは分離する。
- indexing perf に影響が出た場合は cache/helper 分割を止め、`app.rs` 側へ一時的に戻してから境界を再設計する。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- `app-core-segmentation` に着手する前に [docs/CHANGE-PLAN-20260331-app-core-segmentation.md](/mnt/d/work/flistwalker/docs/CHANGE-PLAN-20260331-app-core-segmentation.md) を読むこと。
- 実装順序、検証順序、リスク対応は上記計画書に従うこと。
- 対象範囲、実施順、リスク判断を変更する場合は、実装より先に計画書を更新すること。
- この一時セクションは、計画対象の作業完了後に削除すること。
```

## 11. Progress Log
- 2026-03-31 08:15 Planned after post-hardening review.
- 2026-03-31 08:15 Baseline observations:
- [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) は 4131 行。
- `AppTabState` が query history、履歴検索、result、preview、kind 解決、sort 状態を横断して保持している。
- `new_with_launch` が worker 起動、launch 復元、初期 state seed、初回 index request を一括で扱っている。
- preview/highlight/sort metadata cache の helper 群が [rust/src/app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) に残っている。
- 2026-03-31 08:35 Phase 1 deep review and boundary definition:
- `app.rs` の残存責務は 4 群へ整理できる。
- 第1群 `tab state`: `AppTabState`、`capture_active_tab_state`、`apply_tab_state`、`initialize_tabs_from_saved`、`create_new_tab`、`move_tab` が query/history/result/preview/kind/index 状態をまとめて持っている。
- 第2群 `bootstrap`: `from_launch`、`new_with_launch`、saved roots / UI state / window geometry helper が launch 復元、worker wiring、永続 state seed を横断している。
- 第3群 `cache and kind/search helper`: preview/highlight/sort metadata cache、kind resolution queue、incremental search refresh helper が表示最適化と検索 orchestration の境界にまたがっている。
- 第4群 `polling/orchestration core`: `poll_index_response`、background index 応答、request queue 制御、`eframe::App::update` は最終 coordinator として当面 `app.rs` に残す。
- 実施順は、まず tab state を分けて `capture/apply/restore` の責務を局所化し、その後 bootstrap、cache helper、最後に docs 更新の順で固定する。
- Phase 2 の具体対象は、`AppTabState` を `TabQueryState`、`TabResultState`、`TabIndexState` のような束へ再編し、履歴検索・selection・sort の field copy を減らすこととする。

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- 行数削減は副次効果とし、まずは「変更時に読む範囲の局所化」を優先する。
- `app.rs` の coordinator 性を維持しつつ、state と helper を子 module へ段階退避する。
- 完了前に恒久知見が出た場合は、この計画書ではなく [docs/DESIGN.md](/mnt/d/work/flistwalker/docs/DESIGN.md) と [docs/TESTPLAN.md](/mnt/d/work/flistwalker/docs/TESTPLAN.md) に移す。
