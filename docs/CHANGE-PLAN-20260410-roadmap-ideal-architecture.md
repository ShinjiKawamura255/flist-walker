# CHANGE PLAN: FlistWalker Ideal Architecture Roadmap

## Metadata
- Date: 2026-04-10
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Parent Plan: none
- Child Plan(s): `docs/CHANGE-PLAN-20260410-slice-a-app-coordinator-reduction.md`, `docs/CHANGE-PLAN-20260410-slice-b-lifecycle-contract-hardening.md`
- Scope Label: ideal-architecture
- Related Tickets/Issues: none
- Execution Mode: autonomous
- Execution Mode Policy:
  - review 済み roadmap を基準に、blocking issue がない限り active slice の phase 実行、slice 完了反映、次 slice への遷移まで継続する。
  - roadmap goal 未達の場合は、slice 完了結果を roadmap へ反映したうえで追加 slice の要否を判断し、継続可能なら同じ roadmap 配下で次の active slice へ進む。
  - phase 実行は原則として subagent へ委譲し、main agent は計画更新、レビュー反映、完了判定、コミット、slice 間の引き継ぎを担当する。phase が短すぎて委譲コストの方が高い場合のみ main agent が直接実装する。
  - main agent は roadmap / slice 更新、レビュー反映、subagent への phase 指示、成果レビュー、phase 完了記録、slice 間の引き継ぎを担当する。
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-10 実現性レビュー: feasible。致命的な blocker はなし。
  - 2026-04-10 粒度レビュー: 2 段構成は妥当。3 段目は現時点では不要。
  - 2026-04-10 反映: `Review Status` 表記を日本語へ統一し、Slice A の完了条件を定量化した。
  - 2026-04-10 反映: roadmap の後半 slice は Slice B 完了時の gate で継続/分離を再判定する方針を追記した。
  - 2026-04-11 plan-driven-changes 更新追従: `Execution Mode` / `Execution Mode Policy` を追加し、Temporary Change Plan Rule との対応を明文化した。
  - 2026-04-11 実行方針更新: ユーザ指示に合わせて `Execution Mode` を `autonomous` へ変更し、blocking issue がない限り roadmap を継続実行する運用へ切り替えた。
  - 2026-04-11 Slice B review 反映: phase 実行の責務を subagent 委譲前提へ統一した。

## 1. Background
- 現状の FlistWalker は、検索契約、非同期処理、self-update、release hygiene、クロスプラットフォーム CI まで含めて品質は高い。
- 一方で、GUI coordinator 層の複雑さが高く、`rust/src/app/mod.rs` の `FlistWalkerApp` が state holder、request routing、worker orchestration、tab lifecycle、update/filelist policy を同時に抱えている。
- この構造は現状のテスト密度で支えられているが、今後の機能追加やバグ修正で変更コストを押し上げる。
- 理想形を目指すには、単発のリファクタではなく、段階的に責務を分解し、各段階で挙動と性能を保持しながら前進する必要がある。

## 2. Goal
- FlistWalker の GUI/worker 中心部を、機能追加より先に「理解しやすく壊しにくい形」へ寄せる。
- `FlistWalkerApp` は eframe/egui との入出力結線とトップレベル orchestration に寄せ、永続化ポリシー、owner state transition、request lifecycle は専用モジュールへ閉じる。
- 変更後も既存の検索契約、UI 応答性、request_id による stale 応答吸収、release/update/security hygiene を維持する。
- 最終 slice では、roadmap の goal 達成可否を評価し、追加 slice が必要かを判断する。

## 3. Scope
### In Scope
- app coordinator の責務削減と state ownership の再整理
- index/search/update/filelist の lifecycle 契約の明示化
- GUI 回帰の自動化強化と手動確認依存の削減
- release/platform/docs の恒久ルール整理
- 関連する `docs/REQUIREMENTS.md` / `SPEC.md` / `DESIGN.md` / `TESTPLAN.md` / `ARCHITECTURE.md` / `AGENTS.md` の更新

### Out of Scope
- 検索仕様の新機能追加
- 旧 prototype の整理
- ネットワークドライブ向け最適化
- 配布インストーラの新規整備
- macOS notarization 基盤の新規構築自体

## 4. Constraints and Assumptions
- UI 応答性ポリシーが最優先であり、重い I/O や計算を UI スレッドへ戻してはならない。
- `request_id` による stale response の破棄契約は後方互換で維持する。
- `rust/src/indexer.rs`、`rust/src/app/workers.rs`、`rust/src/app/mod.rs` を含む indexing path 変更では VM-003 の perf guard 実行が必須。
- リファクタは段階的に行い、各 phase 完了時にテストが green で、必要なら個別 commit 単位へ分けられる粒度を維持する。
- roadmap は 2 段で管理し、現時点では active slice を 1 本だけ詳細化する。3 段目は active phase が大きすぎると判定された場合のみ追加する。
- Slice C/D は roadmap 上の候補として保持するが、Slice B 完了時に「この roadmap のまま継続するか」「後続 roadmap へ分離するか」を再判定する。

## 5. Current Risks
- Risk:
  - `FlistWalkerApp` の縮小中に、tab/index/update/filelist の state transition が分散して振る舞いが崩れる
  - Impact:
    - GUI 回帰、stale 応答の誤採用、進行中処理の取り違え
  - Mitigation:
    - active slice は coordinator 縮小に限定し、既存 owner 境界を拡張する方向で進める
- Risk:
  - 計画を広げすぎて、1 回の slice で構造改善と周辺 cleanup を同時に抱える
  - Impact:
    - 完了条件が曖昧になり、途中で検証不能になる
  - Mitigation:
    - roadmap で slice ごとの完了条件を固定し、slice ごとにゴールを閉じる
- Risk:
  - GUI 手動確認項目が多く、構造改善の速度より検証負荷が先に増える
  - Impact:
    - リファクタの継続性低下
  - Mitigation:
    - 後続 slice で GUI 回帰の自動化拡張を明示的に扱う
- Risk:
  - macOS 配布運用の暫定状態を据え置いたまま内部改善だけ進み、品質方針が曖昧なまま残る
  - Impact:
    - release 判断が属人的になる
  - Mitigation:
    - roadmap 終盤で platform/release/docs 方針を整理し、恒久ルール化する

## 6. Execution Strategy
1. Slice A: app coordinator reduction
   - Files/modules/components:
     - `rust/src/app/mod.rs`
     - `rust/src/app/{tabs,pipeline,pipeline_owner,index_coordinator,search_coordinator,update,filelist,session,state}.rs`
     - `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
   - Expected result:
     - `FlistWalkerApp` が coordinator としての責務へ近づき、state transition の owner がさらに明確化される
   - Verification:
     - VM-002 / VM-003 相当の `cargo test` と ignored perf guard
2. Slice B: lifecycle contract hardening
   - Files/modules/components:
     - `rust/src/app/{worker_protocol,workers,index_worker,pipeline,update,filelist}.rs`
     - 関連 app tests
     - `docs/SPEC.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
   - Expected result:
     - index/search/update/filelist の lifecycle と cancellation / stale 吸収の契約がコードと docs の両方で明確になる
   - Verification:
     - VM-003, VM-005 相当の自動検証
3. Slice C: GUI regression automation expansion
   - Files/modules/components:
     - `rust/src/app/tests/**/*`
     - `docs/TESTPLAN.md`
   - Expected result:
     - 手動確認に依存している GUI 回帰の一部を event-driven test に寄せる
   - Verification:
     - 追加 test green、manual 確認項目の削減が説明可能
   - Gate:
     - Slice B 完了時点で構造改善の残件が大きい場合は、本 slice を follow-up roadmap へ分離してよい
4. Slice D: release/platform/docs consolidation
   - Files/modules/components:
     - `docs/{REQUIREMENTS,SPEC,DESIGN,TESTPLAN,ARCHITECTURE,RELEASE}.md`
     - `AGENTS.md`
     - 必要時 `.github/workflows/*`, `README.md`
   - Expected result:
     - macOS/release/docs の暫定ルールと恒久ルールを整理し、roadmap 完了判定を出せる
   - Verification:
     - docs diff review、必要時 `cargo test`、release policy の整合確認
   - Gate:
     - Slice C と同様に、Slice B 完了時点で architecture 改善の完了確認を優先すべき場合は follow-up roadmap へ分離してよい
5. Final validation slice
   - Files/modules/components:
     - roadmap 自体
     - 影響した docs 一式
   - Expected result:
     - roadmap goal を達成したか判定し、継続が必要なら追加 slice を定義する
   - Verification:
     - 各 slice の完了条件の確認、残課題の明示

## 6.1 Slice Ordering and Gates
- Active slice は `Slice B` とする。着手前に active slice plan を更新し、review 済み状態を維持する。
- `Slice A` は完了済み。roadmap へ結果差分、残課題、`Slice B` へ引き継ぐ制約を反映した。
- `Slice B` 完了時は `Slice C` / `Slice D` をこの roadmap で継続するか、follow-up roadmap へ分離するかを再判定する。
- `Final validation slice` は終端 slice として扱い、この roadmap を完了として閉じるか、追加 slice を定義して継続するかを判断する。
- `Execution Mode: autonomous` のため、各 slice 完了後に明示的な停止理由がなければ、roadmap 更新後に次の active slice へ継続する。

## 7. Detailed Task Breakdown
- [x] roadmap の slice 境界、着手順、完了条件を固定する
- [x] active slice として Slice A の詳細計画を作る
- [x] Slice A の実現性レビューを通し、必要なら phase 粒度を修正する
- [ ] Slice B 完了時に、Slice C/D をこの roadmap に残すか follow-up roadmap へ分離するかを判断する
- [x] Slice A 完了後に roadmap を更新し、Slice B 以降の前提差分を反映する
- [ ] 最終 slice で roadmap goal の達成可否を明示する

## 8. Validation Plan
- Automated tests:
  - Slice A/B では `cd rust && cargo test`
  - indexing path へ触れる場合は VM-003 の ignored perf guard 2 本
- Manual checks:
  - GUI 操作感に触れる slice では `docs/TESTPLAN.md` の relevant GUI smoke
- Performance or security checks:
  - FileList/Waker perf regression guard
  - self-update / release policy 変更時は security hygiene と release doc review
- Regression focus:
  - stale response 吸収
  - root/tab 切替時の state 混線
  - GUI 応答性低下
  - release/update 方針の docs 実装不整合

## 9. Rollback Plan
- 各 slice は独立に rollback 可能な単位で進める。
- Slice A では owner 境界の拡張と `mod.rs` 縮小を phase 単位で分け、途中段階でも green を維持する。
- docs 整理はコード変更と分離し、必要なら docs だけ個別に戻せるようにする。
- release/platform 方針変更は docs と workflow を一体で戻す。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- `ideal-architecture` の対応では、実装前に以下の計画書を上から順に読むこと。
- `docs/CHANGE-PLAN-20260410-roadmap-ideal-architecture.md`
- `docs/CHANGE-PLAN-20260410-slice-b-lifecycle-contract-hardening.md`
- roadmap の `Execution Mode: autonomous` と `Execution Mode Policy` に従い、blocking issue がない限り roadmap 更新、phase 実行、slice 完了反映、次 slice 着手まで継続すること。
- 実装順と確認順は計画書に従い、scope / order / risk を変える場合は先に計画書を更新すること。
- phase 実行は原則として subagent へ委譲し、main agent は orchestrator / reviewer として成果確認、計画更新、コミットを担当すること。
- この一時ルールは計画対応の完了後に削除すること。
```

## 11. Progress Log
- 2026-04-10 00:00 Planned roadmap and active slice structure.
- 2026-04-10 00:00 Reviewed by sub-agents; feasibility confirmed and roadmap gates clarified.
- 2026-04-11 00:00 `Execution Mode` を `autonomous` へ更新し、Slice A Phase 1 から継続実装する前提へ切り替えた。
- 2026-04-11 00:00 Slice A Phase 2 の初手として、root browse/dropdown routing を `tabs.rs` owner へ寄せ、`cargo test` green を確認した。
- 2026-04-11 00:00 Slice A Phase 3 の初手として、worker shutdown / viewport close / persist shutdown seam を `worker_runtime.rs` と `session.rs` へ寄せ、`cargo test` green を確認した。
- 2026-04-11 00:00 Slice A の frame lifecycle 整理として、`run_ui_frame()` を `render.rs` owner へ移し、`mod.rs` の update loop をさらに薄くした。`cargo test` green。
- 2026-04-11 00:00 Slice A 継続として、kind resolution queue / pump / response handling を `index_coordinator.rs` owner へ寄せ、`cargo test` green を確認した。
- 2026-04-11 00:00 Slice A 継続として、sort metadata / result ordering helper を `cache.rs` owner へ寄せ、`cargo test` green を確認した。
- 2026-04-11 00:00 Slice A 継続として、action response polling を `tabs.rs` owner へ、sort response polling を `cache.rs` owner へ、row/query command を `input.rs` owner へ寄せ、`cargo test` green を確認した。
- 2026-04-11 00:00 Slice A 継続として、result row navigation と pin toggle を `input.rs` owner へ寄せ、`mod.rs` に残る user command mutation をさらに削減した。`cargo test` green。
- 2026-04-11 00:00 Slice A 継続として、status/notice helper と `run_update_cycle()` / `update()` / `on_exit()` / `Drop` の glue を `coordinator.rs` owner へ寄せ、`mod.rs` をさらに薄くした。`cargo test` green を確認した。
- 2026-04-11 00:00 Slice A 完了判定: `mod.rs` の coordinator glue を `coordinator.rs` へ寄せ、owner 境界の可読性を改善した。`cargo test` green を確認し、Slice B へ進む準備を整えた。

## 12. Communication Plan
- Return to user when:
  - plan creation and review are complete
  - all phases are complete
  - a phase cannot continue without resolving a blocking problem
- If the project is under git control, commit at the end of each completed phase.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Work executed according to the plan or the plan updated first
- [x] If the project is under git control, each completed phase was committed separately
- [x] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- Slice A 完了時に、roadmap 側へ少なくとも「何を `mod.rs` から追い出せたか」「何がなお central coordinator に残るべきか」を記録する。
- Final validation slice は roadmap を閉じるための gate とし、未達項目があれば追加 slice を定義して継続判断を残す。
- `Execution Mode: autonomous` のため、blocking issue がない限り roadmap は slice 完了ごとに更新し、そのまま次 slice へ継続する。
