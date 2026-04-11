# CHANGE PLAN: Slice C GUI Regression Automation Expansion

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice
- Parent Plan: `docs/CHANGE-PLAN-20260411-roadmap-regression-release-followup.md`
- Child Plan(s): none
- Scope Label: gui-regression-automation-expansion
- Related Tickets/Issues: none
- Inherited Execution Mode: standard
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-11 初版: structural GUI smoke のうち event-driven 化しやすい操作を対象に phase を切り出した。
  - 2026-04-11 実現性レビュー: feasible。Slice C の成果は manual smoke 全面削減ではなく、owner test module に自動化の足場を増やすこととして扱う。
  - 2026-04-11 粒度レビュー: docs-only phase と実装 phase が混在するが、Phase 1 は実装前の mapping gate、Phase 4 は slice closure gate として扱うことで許容する。必要以上に 3 段化はしない。
  - 2026-04-11 収束レビュー: 着手可能。重大な blocker はなく、`self-update dialog` 自動化を scope 外とする期待値も明確。

## 1. Background
- `ideal-architecture` の structural refactoring は完了したが、GUI の主要操作はまだ `docs/TESTPLAN.md` の manual smoke に依存する比率が高い。
- 既存の app tests には `render_tests.rs`、`shortcuts.rs`、`session_tabs.rs`、`index_pipeline/*` など owner 境界がすでにあり、manual smoke の一部は event-driven test へ移せる。
- 先に移しやすい flow を固定しておくと、後続の UI 変更時に「触ったが手で見るしかない」範囲を減らせる。

## 2. Goal
- structural GUI smoke の代表操作を owner test module へ落とし、manual-only 項目を一部削減する。
- 追加する test は render/input/tab/filelist の owner 境界に沿って置き、`app_core.rs` へ戻さない。
- docs 側では manual smoke と automated coverage の境界を更新し、今後どこまで自動化済みかを説明可能にする。

## 3. Scope
### In Scope
- `docs/TESTPLAN.md` の structural GUI smoke step 2, 4, 5, 6, 7 のうち event-driven 化しやすい部分
- `rust/src/app/tests/{render_tests,shortcuts,session_tabs,index_pipeline/*}.rs`
- 必要最小限の app helper 追加

### Out of Scope
- 実 GUI を操作する e2e harness の新規導入
- multi-display / OS 固有挙動の手動試験置き換え
- self-update dialog の自動化

## 4. Constraints and Assumptions
- UI 応答性、request_id stale discard、root/tab state 分離の既存契約は変えない。
- テスト追加が主であり、feature behavior を変える変更は避ける。
- owner test module policy を守り、cross-cutting でない regression を `app_core.rs` へ戻さない。

## 5. Execution Strategy
1. Phase 1: manual smoke と既存 test の対応表を固定する
   - Files/modules/components:
     - `docs/TESTPLAN.md`
     - `rust/src/app/tests/mod.rs`
     - `rust/src/app/tests/*`
   - Expected result:
     - どの smoke step をどの owner test module へ落とすかが明確になり、以後の実装 phase の scope を固定できる
   - Verification:
     - doc diff review
2. Phase 2: root/tab/render interaction の regression test を補強する
   - Files/modules/components:
     - `rust/src/app/tests/{render_tests.rs,session_tabs.rs,shortcuts.rs}`
     - 必要時 `rust/src/app/{render.rs,tabs.rs,input.rs}`
   - Expected result:
     - root change、tab interaction、sort/render まわりの manual-heavy flow の一部が test 化される
   - Verification:
     - `cd rust && cargo test`
3. Phase 3: filelist dialog / background flow の regression test を補強する
   - Files/modules/components:
     - `rust/src/app/tests/index_pipeline/{dialogs_and_inflight.rs,search_filelist.rs,filelist_lifecycle.rs}`
     - 必要時 `rust/src/app/filelist.rs`
   - Expected result:
     - Create File List dialog と background walker 準備 flow の主要操作が test 化される
   - Verification:
     - `cd rust && cargo test`
4. Phase 4: docs と roadmap gate を同期する
   - Files/modules/components:
     - `docs/TESTPLAN.md`
     - `docs/CHANGE-PLAN-20260411-roadmap-regression-release-followup.md`
   - Expected result:
     - 自動化済み範囲と manual-only 範囲が docs で説明可能になり、Slice C 完了後に Slice D 詳細化の前提が揃う
   - Verification:
     - doc diff review

## 6. Phase Execution Policy
- active phase は上から順に 1 つずつ完了させる。順序を変える場合は先に本計画を更新する。
- 各 phase 完了時は、完了条件、検証結果、残課題を本計画へ記録し、git 管理下では phase 単位コミットを作成する。

## 7. Detailed Task Breakdown
- [x] manual smoke と既存 test の対応表を固定する
- [x] root/tab/render interaction の regression test を補強する
- [x] filelist dialog / background flow の regression test を補強する
- [ ] TESTPLAN と roadmap を同期する

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - docs 同期時は対象 smoke step が automated/manual のどちらかへ整理されていることを確認する
- Regression focus:
  - root change での state cleanup
  - tab 切替/close/reorder での state 混線
  - FileList dialog と background walker 準備 flow
  - sort/render interaction の継続操作性

## 9. Rollback Plan
- Phase 1 と Phase 4 は docs-only として単独 rollback 可能にする。
- Phase 2 と Phase 3 は test 追加主体で進め、helper 変更が必要でも phase ごとに commit を分ける。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- `regression-release-followup` の対応では、実装前に以下の計画書を上から順に読むこと。
- `docs/CHANGE-PLAN-20260411-roadmap-regression-release-followup.md`
- `docs/CHANGE-PLAN-20260411-slice-c-gui-regression-automation-expansion.md`
- roadmap の `Execution Mode: standard` と `Execution Mode Policy` に従うこと。
- phase 実行は原則として subagent へ委譲し、main agent は orchestrator / reviewer として計画更新、レビュー反映、完了判定、コミットを担当すること。
- 実装順と確認順は計画書に従い、scope / order / risk を変える場合は先に計画書を更新すること。
- この一時ルールは計画対応の完了後に削除すること。
```

## 11. Progress Log
- 2026-04-11 00:00 Slice C 初版を作成した。
- 2026-04-11 00:00 初回レビューを反映し、Slice C の goal を「manual smoke 全面置換」ではなく「自動化の足場づくり」へ補正した。Phase 1 は mapping gate、Phase 4 は slice closure gate と位置付けた。
- 2026-04-11 00:00 Phase 1 として、`TESTPLAN.md` に structural GUI smoke step と owner test module の対応表を追加し、Phase 2/3 の automation target を固定した。docs diff review で整合を確認した。
- 2026-04-11 00:00 Phase 2 として、`render_tests.rs` と `session_tabs.rs` に tab close/move と root dropdown selection cleanup の regression test を追加し、`cargo test` green を確認した。
- 2026-04-11 00:00 Phase 3 として、`search_filelist.rs` に root change 時の ancestor/use-walker confirmation cleanup regression test を追加し、`cargo test` green を確認した。

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Review completed and reflected
- [x] Temporary `AGENTS.md` rule added
- [x] manual smoke と既存 test の対応表を固定する
- [x] root/tab/render interaction の regression test を補強する
- [x] filelist dialog / background flow の regression test を補強する
- [ ] Each completed phase committed separately
- [ ] Verification completed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion
