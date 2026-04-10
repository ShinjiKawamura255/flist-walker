# CHANGE PLAN: Slice A App Coordinator Reduction

## Metadata
- Date: 2026-04-10
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice
- Parent Plan: `docs/CHANGE-PLAN-20260410-roadmap-ideal-architecture.md`
- Child Plan(s): none
- Scope Label: app-coordinator-reduction
- Related Tickets/Issues: none
- Inherited Execution Mode: autonomous
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-10 実現性レビュー: feasible。致命的な blocker はなし。
  - 2026-04-10 粒度レビュー: phase 順序は妥当。Phase 1 の完了条件をより定量化すべきとの指摘あり。
  - 2026-04-10 反映: `Review Status` 表記を日本語へ統一した。
  - 2026-04-10 反映: Phase ごとの完了条件を、対象 helper / routing / lifecycle seam 単位で判定できる形へ具体化した。
  - 2026-04-11 plan-driven-changes 更新追従: 親 roadmap の `Execution Mode` に従うこと、subslice は初期作成せず slice review でのみ追加検討することを明記した。
  - 2026-04-11 実行方針更新: 親 roadmap の `Execution Mode` を `autonomous` へ切り替えたため、本 slice も継続実装前提へ更新した。

## 1. Background
- `FlistWalkerApp` は既に複数 module へ分解されているが、`mod.rs` 側に state、policy、orchestration、trace helper、runtime shutdown、tab/update/filelist の横断判断が多く残っている。
- 現状は test density で支えられているが、変更のたびに `mod.rs` と複数 owner を往復して理解する必要があり、局所変更の心理的コストが高い。
- 理想形へ寄せる第一歩として、`mod.rs` をさらに「UI framework adapter + top-level coordinator」へ薄くする必要がある。

## 2. Goal
- `rust/src/app/mod.rs` の責務を縮小し、`FlistWalkerApp` が保持する state と direct decision を減らす。
- tab/index/search/update/filelist/session などの state transition で、owner module が最終判断を持つ比率を増やす。
- 挙動は変えず、少なくとも既存 test と perf guard が green のまま着地する。

## 3. Scope
### In Scope
- `FlistWalkerApp` に残す責務の明文化
- `mod.rs` に残っている state transition / helper / routing の owner module への移動
- Slice A の完了に必要な app tests と docs 更新

### Out of Scope
- 検索仕様変更
- 新機能追加
- GUI デザイン変更
- release policy の本格見直し
- 3 段目 plan 追加

## 4. Constraints and Assumptions
- UI 応答性と stale response 吸収契約は不変。
- active tab / background tab / request routing の意味論は維持する。
- `mod.rs` を一気に分割し切ろうとせず、phase ごとに「責務の束」を移す。
- indexing path に触れる可能性が高いため、VM-003 検証を前提に置く。
- Slice A だけで理想形を完了させず、後続 slice のための stable seam を作ることを優先する。
- この slice は roadmap の `Execution Mode: autonomous` 配下で進め、完了後は roadmap 更新を先に行い、blocking issue がなければ次 slice へ継続する。
- 3 段目計画は初期作成しない。active phase 列が大きすぎると slice review で判定された場合のみ `subslice` を追加する。

## 5. Current Risks
- Risk:
  - `mod.rs` からコードを移しても state の実 ownership が曖昧なまま残る
  - Impact:
    - ファイル移動だけで複雑さが温存される
  - Mitigation:
    - 各 phase で「どの owner が最終判断を持つか」を文章とコードの両方で固定する
- Risk:
  - 複数 feature を同時に剥がしてレビュー不能になる
  - Impact:
    - 変更差分が大きくなり、途中で regression を切り分けにくくなる
  - Mitigation:
    - phase を owner 境界単位に分け、各 phase 完了時に green を確認する
- Risk:
  - `mod.rs` 内の trace / shutdown / startup helper の扱いを後回しにして coordinator の見通しが改善しない
  - Impact:
    - 行数だけ減っても読みやすさが改善しない
  - Mitigation:
    - startup/shutdown/frame lifecycle も対象に含め、top-level orchestration seam を明示する

## 6. Execution Strategy
1. Phase 1: coordinator の責務境界を固定する
   - Files/modules/components:
     - `rust/src/app/mod.rs`
     - `docs/ARCHITECTURE.md`
     - `docs/DESIGN.md`
   - Expected result:
     - `FlistWalkerApp` に残す責務と owner module へ委譲すべき責務が明文化される
     - 少なくとも `startup/bootstrap`, `frame update cycle`, `shutdown/persist`, `tab routing`, `filelist/update dialog dispatch`, `trace helper` の 6 区分で owner 候補が棚卸しされる
   - Verification:
     - docs diff review
     - `mod.rs` の対象 helper 一覧を plan または実装メモで追跡可能にする
2. Phase 2: state transition の owner を増やす
   - Files/modules/components:
     - `rust/src/app/{tabs,pipeline,pipeline_owner,index_coordinator,search_coordinator,update,filelist,session,state}.rs`
     - `rust/src/app/mod.rs`
   - Expected result:
     - `mod.rs` から owner module へ state transition / routing helper が移り、direct mutation が減る
     - 少なくとも `tab switch/reorder/close`, `filelist confirmation/apply`, `update request/response handling`, `search/index refresh dispatch` のいずれか 2 系統以上が owner module 側へ寄る
   - Verification:
     - `cd rust && cargo test`
     - 必要時 VM-003 perf guard
3. Phase 3: frame lifecycle と shutdown/startup seam を整理する
   - Files/modules/components:
     - `rust/src/app/mod.rs`
     - `rust/src/app/{bootstrap,session,worker_runtime}.rs`
   - Expected result:
     - startup, update cycle, exit, drop fallback の責務がさらに読みやすくなる
     - `update()`, `on_exit()`, `Drop` 周辺で top-level orchestration と owner 呼び分けが追いやすくなる
   - Verification:
     - `cd rust && cargo test`
     - app exit / persistence / worker join regression test の維持確認
4. Phase 4: docs と validation matrix を同期する
   - Files/modules/components:
     - `docs/ARCHITECTURE.md`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - 必要時 `AGENTS.md`
   - Expected result:
     - Slice A の構造改善が docs と検証基準へ反映される
   - Verification:
     - doc diff review
     - ID / file reference 整合確認

## 6.1 Phase Execution Policy
- active phase は上から順に 1 つずつ完了させる。phase の順序変更や統合が必要なら、先に本計画を更新する。
- 各 phase 完了時は、少なくとも完了条件、検証結果、残課題を記録し、git 管理下では phase 単位コミット可否を判断する。
- Phase 2 の開始前後で、phase 列の粒度が粗すぎる、または検証境界が曖昧だと判定された場合のみ `subslice` 追加を再評価する。
- Slice A 完了後は本計画だけで閉じず、親 roadmap へ結果、当初想定との差分、後続 slice への引き継ぎ事項を反映する。
- `Execution Mode: autonomous` のため、Phase 1 完了後も blocking issue がない限り本 slice 内の次 phase へ進める。

## 7. Detailed Task Breakdown
- [x] `FlistWalkerApp` に残す責務の定義を fixed point として文書化する
- [x] `mod.rs` 内の helper / state transition / request routing を owner 候補ごとに棚卸しする
- [x] `startup/bootstrap`, `frame update cycle`, `shutdown/persist`, `tab routing`, `filelist/update dialog dispatch`, `trace helper` の 6 区分ごとに残置/移譲方針を決める
- [ ] owner module へ移せる責務を phase 単位で移す
- [ ] `tab switch/reorder/close`, `filelist confirmation/apply`, `update request/response handling`, `search/index refresh dispatch` から少なくとも 2 系統以上を owner module 側へ寄せる
- [ ] `update()` / `on_exit()` / `Drop` に残る orchestration を見直す
- [ ] Slice A で追加した seam と validation rule を docs へ反映する
- [ ] Slice A 完了時に roadmap へ結果と残課題を戻す

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - indexing path へ触れた場合:
    - `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`
    - `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
- Manual checks:
  - 検索入力、結果移動、tab 切替、FileList ダイアログ、update dialog の smoke を必要に応じて実施
- Performance or security checks:
  - stale response 吸収、UI freeze 退行、sync I/O の描画ループ混入有無
- Regression focus:
  - active/background tab state の混線
  - filelist / update dialog command dispatch
  - startup/shutdown 時の worker join / persistence 退行

## 9. Rollback Plan
- Phase 1 は docs と軽量 seam 整理に留め、単独 rollback 可能にする。
- Phase 2/3 は owner 境界ごとに commit を分け、問題が出た phase だけ戻せるようにする。
- docs 更新は code 変更と同一 phase に含めるが、rollback 時は code + docs をセットで戻す。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- `ideal-architecture` の対応では、実装前に roadmap と active slice plan を上から順に読むこと。
- roadmap の `Execution Mode: autonomous` と `Execution Mode Policy` に従うこと。
- active phase は本計画の順序に従って実行し、scope / order / risk を変える場合は先に計画書を更新すること。
- `subslice` は初期状態では使わず、slice review で必要と判定された場合のみ追加すること。
- この一時ルールは計画対応の完了後に削除すること。
```

## 11. Progress Log
- 2026-04-10 00:00 Planned Slice A around app coordinator reduction.
- 2026-04-10 00:00 Reviewed by sub-agents; feasibility confirmed and completion criteria tightened.
- 2026-04-11 00:00 Phase 1 の責務境界固定に着手し、`ARCHITECTURE.md` と `DESIGN.md` へ 6 区分の owner 棚卸しを反映する方針を確定した。
- 2026-04-11 00:00 Phase 2 の初手として、root browse/dropdown helper を `mod.rs` から `tabs.rs` へ移し、tab/root routing owner を明確化した。`cargo test` は green。
- 2026-04-11 00:00 Phase 3 の初手として、worker shutdown / viewport close helper を `worker_runtime.rs` へ、persist + shutdown seam を `session.rs` へ移し、`update()` / `on_exit()` / `Drop` の top-level orchestration を薄くした。`cargo test` は green。
- 2026-04-11 00:00 frame lifecycle の render 側 owner を強めるため、`run_ui_frame()` を `render.rs` へ移した。`cargo test` は green。
- 2026-04-11 00:00 kind resolution の queue / pump / poll を `index_coordinator.rs` へ寄せ、`mod.rs` から kind resolution owner を縮小した。`cargo test` は green。

## 12. Communication Plan
- Return to user when:
  - plan creation and review are complete
  - all phases are complete
  - a phase cannot continue without resolving a blocking problem
- If the project is under git control, commit at the end of each completed phase.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] If the project is under git control, each completed phase was committed separately
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- Slice A の成功条件は「行数削減」ではなく、「owner ごとの責務がコード上で追いやすくなること」とする。
- Phase 2 で active phase が大きすぎると判定された場合だけ、subslice の追加を検討する。
- Slice A 完了後は、親 roadmap の次 slice 判定に必要な差分を必ず戻し、autonomous mode の継続条件を満たしてから次へ進む。
