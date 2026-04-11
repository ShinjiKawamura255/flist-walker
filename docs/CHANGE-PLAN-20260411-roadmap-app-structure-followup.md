# CHANGE PLAN: App Structure Follow-up Roadmap

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Mode: autonomous
- Execution Mode Policy: Review 済み roadmap を起点に、問題がない限り roadmap 完遂まで active slice の作成、review、phase 実行、phase ごとの commit、roadmap 更新を継続する。phase 実行は原則 subagent に委譲し、main agent は orchestrator と reviewer を担う。Slice 完了後は roadmap を更新し、goal 未達なら次 slice を同じ方針で継続する。
- Parent Plan: none
- Child Plan(s): docs/CHANGE-PLAN-20260411-slice-b-search-indexer-decomposition.md (active). Slice C/D plan docs are intentionally created when each slice becomes active.
- Scope Label: app-structure-followup
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-11 initial review で、未作成 slice を `Child Plan(s)` へ並べた点、review 状態表記、Temporary Rule の参照強度が曖昧という指摘を受けた。
  - 対応として、`Child Plan(s)` は active slice のみに絞り、後続 slice は roadmap 本文で queue として管理する形へ修正した。
  - Slice A の広さと phase 順序についても指摘を受け、tab/session boundary を app-global 抽出より先に置くように見直した。
  - 2026-04-11 convergence review で blocking finding は解消済みと確認されたため、本 roadmap を `レビュー済み` とする。
  - `Execution Mode: autonomous` is intentional because the roadmap should run through slice creation, review, implementation, and phase commits without returning to the user unless blocked.
  - Phase execution is expected to be delegated to subagents by default. The main agent remains responsible for plan maintenance, review, and commit boundaries.

## 1. Background
- `FlistWalkerApp` は `app/` 分割後も巨大 state holder として残っており、feature owner module が `&mut self` 越しに広い状態へ触れる構造が保守コストを上げている。
- `search.rs` と `indexer.rs` は query compilation / execution / IO / caching / ranking など複数責務を 1 ファイルへ抱えており、変更時の回帰面が広い。
- CI / config / dependency hygiene でも、`cargo clippy` 未常設、coverage 未可視化、不要依存候補、環境変数の役割混在といった follow-up が残っている。

## 2. Goal
- `FlistWalkerApp` の state ownership を機能ドメインごとの sub-struct へ段階移譲し、coordinator と state holder の境界を明確にする。
- `search.rs` / `indexer.rs` を複数 module に分割し、query / execution / IO / ranking / config を owner ごとに保守できる構造へ寄せる。
- CI / dependency / config hygiene を最低限の実運用水準へ揃え、不要依存と暫定設定の扱いを明文化する。
- 最後の slice では docs と validation を閉じ、roadmap を完了または継続判断できる状態にする。

## 3. Scope
### In Scope
- `rust/src/app/mod.rs` と `FlistWalkerApp` 周辺の state decomposition
- `rust/src/app/state.rs`、`ui_state.rs`、`query_state.rs`、`tab_state.rs` など state holder の再配置
- `rust/src/search.rs` と `rust/src/indexer.rs` の責務分割
- `Cargo.toml` dependency hygiene、CI lint/coverage 強化、設定経路の分類と必要な config surface 整理
- 上記変更に伴う docs / tests / validation matrix 更新

### Out of Scope
- 新規 UI 機能追加
- 自己更新仕様の拡張
- release asset 形式の再設計
- network drive 最適化や新しい検索演算子追加

## 4. Constraints and Assumptions
- UI 非ブロック原則を壊さない。index/search/preview/filelist/update の worker 契約と request_id stale discard は維持する。
- `search.rs` / `indexer.rs` の分割は挙動互換を前提に行い、仕様演算子と FileList 優先契約は変えない。
- 実装変更時は `cargo test` を最低限実行し、インデクシング経路へ触れる slice では VM-003 perf guard も適用する。
- config 一元化は「ユーザ設定」「開発/試験 override」「ビルド/配布 secret」を混同しない分類から始める。

## 5. Current Risks
- Risk:
  - `FlistWalkerApp` 分解の途中で state ownership と tab/session restore が崩れる。
  - Impact:
    - root/query/result の同期ずれ、background tab 応答の誤適用、persist/restore regression。
  - Mitigation:
    - Slice A を active tab / shared app / persistence seam ごとに phase 分割し、owner test を先行拡張する。
- Risk:
  - `search.rs` / `indexer.rs` 分割で perf regression や仕様差分が出る。
  - Impact:
    - 体感遅延増加、query/operator 契約逸脱、FileList/walker path の regression。
  - Mitigation:
    - Slice B では module split 前後で unit/perf guard を維持し、責務境界ごとに test owner を明示する。
- Risk:
  - config/CI hygiene を一気に進めると docs と実装の整合が崩れる。
  - Impact:
    - 公開向け設定説明の混乱、CI 運用のノイズ増大、不要依存の取りこぼし。
  - Mitigation:
    - Slice C を audit -> minimal fix -> docs sync の順で進める。

## 6. Execution Strategy
1. Slice A: App state decomposition
   - Files/modules/components:
     - `rust/src/app/mod.rs`, `rust/src/app/state.rs`, `rust/src/app/ui_state.rs`, `rust/src/app/query_state.rs`, `rust/src/app/tab_state.rs`, owner modules, app tests, docs
   - Expected result:
     - `FlistWalkerApp` がドメイン別 state holder を通して owner API を呼ぶ構造になり、cross-feature state の境界が以前より明確になる。
   - Verification:
     - `cargo test`
2. Slice B: Search and indexer decomposition
   - Files/modules/components:
     - `rust/src/search.rs`, `rust/src/indexer.rs`, related tests, docs
   - Expected result:
     - query compilation / execution / ranking / IO / filelist-write などが module 単位へ分離され、責務ごとの変更が局所化される。
   - Verification:
     - `cargo test`
     - VM-003 perf guard 2 本
3. Slice C: Tooling and config hygiene
   - Files/modules/components:
     - `rust/Cargo.toml`, `.github/workflows/*`, config-loading code paths, README/docs
   - Expected result:
     - `walkdir` の要否が確定し、必要なら削除される。CI に `cargo clippy` と coverage 計測が追加される。設定経路は user-facing / dev-only / build-secret に分類され、必要な config surface が導入される。
   - Verification:
     - `cargo test`
     - CI/workflow diff review
     - docs/config consistency review
4. Slice D: Roadmap closure and steady-state sync
   - Files/modules/components:
     - `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, `AGENTS.md`
   - Expected result:
     - 恒久 docs と validation が最終構造へ同期し、roadmap を完了または継続判断できる。
   - Verification:
     - doc diff review
     - `rg` 参照整合確認

## 6.1 Planned Slice Queue
- Slice A: App state decomposition
  - Purpose:
    - `FlistWalkerApp` の state ownership を domain bundle へ段階移譲し、次 slice の module split 前提を作る。
  - Dependency / entry condition:
    - roadmap review 完了後すぐ着手。
  - Exit condition:
    - state bundle の owner 境界と app docs/test が同期している。
- Slice B: Search and indexer decomposition
  - Purpose:
    - `search.rs` / `indexer.rs` を owner 別 module へ分離し、責務ごとの test/perf 境界を明示する。
  - Dependency / entry condition:
    - Slice A で app-side state ownership が十分局所化され、search/index coordinator 側の依存境界が固定している。
  - Exit condition:
    - module split 完了後も search operator 契約と VM-003 perf guard が維持される。
- Slice C: Tooling and config hygiene
  - Purpose:
    - CI lint/coverage、dependency hygiene、config 経路分類を steady-state へ揃える。
  - Dependency / entry condition:
    - Slice B で search/index boundary と docs が落ち着いている。
  - Exit condition:
    - CI / config / dependency の扱いが code/docs/workflow で整合している。
- Slice D: Roadmap closure and steady-state sync
  - Purpose:
    - roadmap goal 達成確認、恒久 docs 反映、closure か追加 slice かの判断を行う。
  - Dependency / entry condition:
    - Slice A-C の結果が roadmap へ反映済み。
  - Exit condition:
    - roadmap を閉じるか、追加 slice を定義して継続するかを判断できる。

## 7. Detailed Task Breakdown
- [ ] roadmap と active slice の計画作成・review・Temporary Rule 追加
- [x] Slice A で `FlistWalkerApp` state decomposition を進める
- [ ] Slice B で `search.rs` / `indexer.rs` を責務分割する
- [ ] Slice C で CI / config / dependency hygiene を整える
- [ ] Slice D で roadmap closure と恒久 docs 反映を行う

## 8. Validation Plan
- Automated tests:
  - `cargo test`
  - Slice B では VM-003 perf guard 2 本
- Manual checks:
  - 必要に応じて structural GUI smoke の該当手順を実施
- Performance or security checks:
  - search/indexer split 後の perf guard
  - dependency hygiene 時の OSS/CI 影響確認
- Regression focus:
  - tab/session restore、background response routing、FileList/walker indexing、query operator 契約

## 9. Rollback Plan
- 各 phase は個別 commit に閉じる。state decomposition と module split を跨いで混在させない。
- Slice A/B で regression が出た場合は該当 phase commit 単位で revert できるようにする。
- Slice C の CI/config 変更は docs-only / workflow-only / code-path change を分けて戻せるようにする。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-structure-followup`, read the relevant change plan document(s) before starting implementation.
- Read them from upper to lower order:
  - `[docs/CHANGE-PLAN-20260411-roadmap-app-structure-followup.md]`
  - `[docs/CHANGE-PLAN-20260411-slice-a-app-state-decomposition.md]`
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Because the roadmap says `Execution Mode: autonomous`, continue autonomously through slice creation, review, phase execution, roadmap updates, and phase commits until the roadmap is complete unless a blocking problem occurs.
- Delegate phase execution to subagents by default; the main agent acts as orchestrator and reviewer.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-11 12:20 Planned initial roadmap draft.
- 2026-04-11 12:55 Phase 1 inventory gate completed. `ARCHITECTURE.md` / `DESIGN.md` に 4-way state inventory (`app-global shared`, `active-tab-local`, `persisted/background tab`, `feature dialog/update`) を固定した。
- 2026-04-11 14:35 Slice A Phase 2 completed. `tabs` は `TabSessionState` bundle を介して live tab/session registry を保持する構造へ移行し、`cargo test` green を確認した。
- 2026-04-11 19:05 Slice A Phase 3 completed. `root_browser` / `filelist_state` / `update_state` を `FeatureStateBundle` へ抽出し、feature dialog/update owner を `state.rs` へ寄せた。`cargo test` green。
- 2026-04-11 19:20 Slice A completed. 恒久 docs を state bundle ownership に同期し、roadmap は Slice B planning へ進める状態になった。
- 2026-04-11 19:40 Slice B activated. `search` / `indexer` module split を 4 phase で進める slice plan を作成し、active child plan を切り替えた。

## 12. Communication Plan
- Return to user when:
  - plan creation and review are complete
  - all phases are complete
  - a phase cannot continue without resolving a blocking problem
- If the project is under git control, commit at the end of each completed phase.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] If the project is under git control, each completed phase was committed separately
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- Slice D is reserved for roadmap goal validation and closure judgment. If Slice C shows that config migration needs additional dedicated work, update this roadmap before implementation continues.
