# CHANGE PLAN: Slice B Lifecycle Contract Hardening

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice
- Parent Plan: `docs/CHANGE-PLAN-20260410-roadmap-ideal-architecture.md`
- Child Plan(s): none
- Scope Label: lifecycle-contract-hardening
- Related Tickets/Issues: none
- Inherited Execution Mode: autonomous
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-11 実現性レビュー: feasible。致命的な blocker はなし。
  - 2026-04-11 粒度レビュー: 4 phase の境界は妥当で、現時点では `subslice` は不要。
  - 2026-04-11 反映: 親 roadmap と実行責務を揃え、phase 実行は subagent 委譲前提へ統一した。
  - 2026-04-11 反映: Slice C/D を follow-up roadmap へ分離する判断基準を Phase 4 と Final Notes へ追記した。

## 1. Background
- Slice A により `mod.rs` の coordinator 責務は大きく縮小し、owner module の境界が追いやすくなった。
- 一方で、index/search/update/filelist の lifecycle 契約は code 上で分散しており、`request_id` による stale 応答吸収、cancel、terminal cleanup、notice 更新の責務が module ごとに読み解きづらい箇所が残っている。
- 次の段階では、機能追加ではなく既存非同期フローの契約を明文化し、コードと docs と tests の 3 面を揃える必要がある。

## 2. Goal
- index/search/update/filelist の lifecycle 契約を、owner module と tests と docs の両方で追える状態にする。
- stale response 吸収、cancel、terminal cleanup、request routing の責務を module 単位で固定する。
- 挙動は変えず、既存 test と必要な perf guard を green のまま維持する。

## 3. Scope
### In Scope
- `worker_protocol.rs` / `pipeline.rs` / `pipeline_owner.rs` / `search_coordinator.rs` / `index_coordinator.rs` / `update.rs` / `filelist.rs` の lifecycle 契約整理
- lifecycle 契約に対応する app tests の補強
- `SPEC.md` / `DESIGN.md` / `TESTPLAN.md` への契約反映

### Out of Scope
- GUI デザイン変更
- 新機能追加
- release/platform policy の見直し
- GUI smoke の自動化拡張そのもの

## 4. Constraints and Assumptions
- UI 応答性ポリシーと `request_id` stale 応答破棄契約は不変とする。
- index/search/update/filelist の worker 実装を全面改造せず、owner module ごとの責務明文化と cleanup 経路の整理を優先する。
- `rust/src/app/workers.rs` / `rust/src/app/index_worker.rs` / `rust/src/app/pipeline.rs` / `rust/src/app/mod.rs` の indexing path に触れた場合は VM-003 perf guard を実行する。
- Slice B 完了時は、Slice C/D をこの roadmap で継続するか follow-up roadmap へ分離するかの判断材料を roadmap 側へ戻す。

## 5. Current Risks
- Risk:
  - lifecycle 契約の整理中に stale response 吸収や cancel cleanup の順序を崩す
  - Impact:
    - 古い応答の誤採用、進行中フローの stuck、notice/status の巻き戻り
  - Mitigation:
    - phase ごとに対象 flow を限定し、tests と docs を同じ phase で更新する
- Risk:
  - contract 明文化より先に大規模な worker 再編へ踏み込んで差分が肥大化する
  - Impact:
    - regression 切り分けが難しくなる
  - Mitigation:
    - worker protocol surface は維持し、cleanup / ownership / tests に絞る
- Risk:
  - docs が code 変更に追随せず、Slice C/D の前提が曖昧なまま残る
  - Impact:
    - 後続 slice の粒度判断を誤る
  - Mitigation:
    - Phase 4 を docs 同期専用で確保し、roadmap へ carry-over を明示する

## 6. Execution Strategy
1. Phase 1: lifecycle 契約の owner matrix を固定する
   - Files/modules/components:
     - `rust/src/app/{worker_protocol,pipeline,pipeline_owner,search_coordinator,index_coordinator,update,filelist}.rs`
     - `docs/SPEC.md`
     - `docs/DESIGN.md`
   - Expected result:
     - index/search/update/filelist ごとに request enqueue、inflight tracking、stale response discard、terminal cleanup の owner が文章で追える
     - phase 2/3 で触る cleanup seam が明文化される
   - Verification:
     - doc diff review
     - 対象 helper 一覧を plan または実装メモで追跡可能にする
2. Phase 2: index/search lifecycle cleanup を harden する
   - Files/modules/components:
     - `rust/src/app/{pipeline,pipeline_owner,search_coordinator,index_coordinator,worker_protocol,workers,index_worker}.rs`
     - `rust/src/app/tests/index_pipeline/*`
   - Expected result:
     - index/search の request lifecycle と stale discard / cleanup が owner API 経由で追いやすくなる
     - cancellation / terminal response / rerun 契約に対応する regression test が補強される
   - Verification:
     - `cd rust && cargo test`
     - VM-003 perf guard 2 本
3. Phase 3: update/filelist lifecycle cleanup を harden する
   - Files/modules/components:
     - `rust/src/app/{update,filelist,worker_protocol,workers}.rs`
     - `rust/src/app/tests/{update_commands.rs,index_pipeline/*}`
   - Expected result:
     - update/filelist の pending / inflight / cancel / completion notice 契約が owner module と tests で追える
     - flow ごとの failure / cancel / supersede ケースが regression test で固定される
   - Verification:
     - `cd rust && cargo test`
4. Phase 4: docs と roadmap gate を同期する
   - Files/modules/components:
     - `docs/SPEC.md`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/CHANGE-PLAN-20260410-roadmap-ideal-architecture.md`
   - Expected result:
     - lifecycle 契約と検証基準が docs に反映され、Slice C/D の継続可否判断に必要な材料が roadmap へ戻る
     - Slice B 完了時に、GUI smoke の自動化不足または release/platform docs の整理量が lifecycle 契約改善と独立に大きい場合は、Slice C/D を follow-up roadmap へ分離する判断材料が揃う
   - Verification:
     - doc diff review
     - validation matrix / file reference 整合確認

## 6.1 Phase Execution Policy
- active phase は上から順に 1 つずつ完了させる。phase の順序変更や統合が必要なら、先に本計画を更新する。
- 各 phase 完了時は、完了条件、検証結果、残課題を記録し、git 管理下では phase 単位コミットを作成する。
- worker / lifecycle 契約の扱いが想定より広く、phase 列のままでは検証境界が曖昧になる場合のみ `subslice` 追加を再評価する。
- Slice B 完了後は、本計画だけで閉じず、親 roadmap へ carry-over と Slice C/D gate 判断材料を反映する。

## 7. Detailed Task Breakdown
- [ ] lifecycle 契約の owner matrix を index/search/update/filelist ごとに文書化する
- [ ] index/search の stale discard / cancel / terminal cleanup を owner API 経由で追いやすくする
- [ ] update/filelist の pending / inflight / cancel / completion notice 契約を整理する
- [ ] lifecycle 契約に対応する regression test を補強する
- [ ] SPEC / DESIGN / TESTPLAN を code に同期する
- [ ] Slice B 完了時に roadmap へ結果と Slice C/D gate 判断材料を戻す

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - indexing path へ触れた場合:
    - `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`
    - `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
- Manual checks:
  - lifecycle 契約変更が UI observable output に影響する場合のみ relevant GUI smoke を選択実施する
- Regression focus:
  - stale response discard
  - cancellation / terminal cleanup
  - inflight request tracking
  - notice/status の巻き戻り

## 9. Rollback Plan
- Phase 1 は docs / plan 更新に留め、単独 rollback 可能にする。
- Phase 2 と Phase 3 は flow ごとに commit を分け、index/search と update/filelist を独立に戻せるようにする。
- docs 更新は code 変更と同一 phase に含めるが、rollback 時は code + docs をセットで戻す。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- `ideal-architecture` の対応では、実装前に roadmap と active slice plan を上から順に読むこと。
- roadmap の `Execution Mode: autonomous` と `Execution Mode Policy` に従うこと。
- active phase は本計画の順序に従って実行し、scope / order / risk を変える場合は先に計画書を更新すること。
- phase 実行は原則として subagent へ委譲し、main agent は orchestrator / reviewer として成果確認、計画更新、コミットを担当すること。
- `subslice` は初期状態では使わず、slice review で必要と判定された場合のみ追加すること。
- この一時ルールは計画対応の完了後に削除すること。
```

## 11. Progress Log
- 2026-04-11 00:00 Initial Slice B plan created after Slice A completion.
- 2026-04-11 00:00 Slice B review を反映し、実行責務を親 roadmap と統一した。`subslice` は不要と判断した。

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
- Slice B の成功条件は「コード移動」ではなく、「lifecycle 契約を code / tests / docs のどこから見ても同じ意味で追えること」とする。
- Slice B 完了時は、Slice C/D をこの roadmap に残すか follow-up roadmap へ分離するかの判断を roadmap 側へ必ず戻す。
- Slice C/D を follow-up roadmap へ分離するのは、Slice B 完了時点で GUI smoke 自動化または release/platform docs 整理の残量が大きく、lifecycle 契約 hardening と独立に閉じた slice として扱った方が review / rollback / verification 境界が明確になる場合に限る。
