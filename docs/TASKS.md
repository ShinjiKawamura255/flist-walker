# TASKS

## Status Snapshot
- Updated: 2026-04-08
- Current active engineering roadmap: `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md`
- App architecture change-plan program: IN PROGRESS
- Notes:
  - app architecture の multi-slice refactor は closure まで完了し、恒久 docs だけを残す状態へ移行した。
  - 2026-04-08 に `plan-driven-changes` 用の上位 roadmap と child slice を追加し、同日中に close した。
  - 2026-04-08 に architecture debt closure の計画を再導入し、feature freeze 前提で debt を重要度順に解消する方針へ切り替えた。
  - 2026-04-08 の architecture debt closure program は closure まで完了し、temporary rule と change-plan 文書を撤去した。
  - 2026-04-08 に single-plan の architecture refactor program を再導入し、Phase 1 の pipeline state-transition 整理、Phase 2 の `IndexCoordinator` owner API 化、Phase 3 の worker modularization を完了した。
  - 2026-04-08 に次の app architecture 改善を roadmap 化し、Slice A として pipeline owner extraction を active slice に設定した。
  - 2026-04-08 に Slice A Phase 1 の seam 抽出を完了し、active request cleanup を `IndexCoordinator` 経由へ寄せたうえで search refresh request/response routing を `SearchCoordinator` lifecycle helper と pipeline-local handler に整理した。
  - 2026-04-08 に Slice A Phase 2 を完了し、`pipeline_owner.rs` へ search/result refresh と entry-filter application の owner surface を移して `pipeline.rs` を thin wrapper 化した。
  - 2026-04-08 に Slice A Phase 3 を完了し、`ARCHITECTURE.md` / `DESIGN.md` を `pipeline_owner.rs` ベースの owner boundary に同期した。
  - 2026-04-08 に Slice B 向けの active child plan `docs/CHANGE-PLAN-20260408-background-tab-result-flow-slice.md` を追加し、background tab result-flow separation へ進む状態に切り替えた。
  - 2026-04-08 に Slice B を完了し、background search/index apply helper を `tabs.rs` 側へ揃えて active/background result-flow の境界を明示した。
  - 2026-04-08 に Slice C 向けの active child plan `docs/CHANGE-PLAN-20260408-worker-protocol-separation-slice.md` を追加し、worker protocol separation へ進む状態に切り替えた。

## Completed Programs

### Program E: Architecture Refactor Follow-up
- Status: DONE on 2026-04-08
- Goal: pipeline 重複遷移を整理し、index request lifecycle の owner 境界を明示し、`workers.rs` を concern ごとに分割して app architecture の保守コストを下げる。
- Outcome:
  - `pipeline.rs` は active/background refresh と terminal cleanup の重複 state transition を helper 化した。
  - `index_coordinator.rs` は request id 採番、refresh 開始、terminal cleanup を持つ owner API を担当する構造になった。
  - `worker_runtime.rs` と `index_worker.rs` を追加し、`workers.rs` から runtime orchestration と index streaming/classification を切り離した。
  - `ARCHITECTURE.md` と `DESIGN.md` は新しい owner/module 境界へ同期し、temporary rule と change plan は closure 時に撤去した。

| Phase | Status | Completed |
| --- | --- | --- |
| Phase 1: Pipeline State-Transition Consolidation | DONE | 2026-04-08 |
| Phase 2: Pipeline Ownership Extraction | DONE | 2026-04-08 |
| Phase 3: Worker Modularization and Docs Closure | DONE | 2026-04-08 |

### Program D: Architecture Debt Closure
- Status: DONE on 2026-04-08
- Goal: updater, perf, diagnostics, docs/closure の順で visible architecture debt を解消し、新規機能再開前の steady-state docs と validation rule を確定する。
- Outcome:
  - updater contract は candidate selection / support classification / app command boundary に分割された。
  - lightweight perf gate は PR CI に載り、heavy suite は分離維持された。
  - diagnostics は request_id-correlated trace と supportability notes で追跡できるようになった。
  - closure record は `TASKS.md` に集約し、temporary rule と debt-program change plans は撤去した。

| Slice | Status | Completed |
| --- | --- | --- |
| Slice A: Updater Hardening | DONE | 2026-04-08 |
| Slice B: Perf Gate Strengthening | DONE | 2026-04-08 |
| Slice C: Diagnostics and Supportability | DONE | 2026-04-08 |
| Slice D: Docs and Closure Restructuring | DONE | 2026-04-08 |

### Program A: `app.rs` Split Follow-up
- Status: DONE on 2026-04-01
- Goal: `rust/src/app.rs` に残る tab lifecycle、index/search orchestration、preview/highlight/cache の責務を追加分割し、coordinator 境界を明確化する。
- Outcome:
  - `tabs.rs`, `pipeline.rs`, `cache.rs` などへ責務を分割した。
  - 当時の temporary plan と `AGENTS.md` temporary rule は撤去済み。

| ID | Status | Area | Summary |
| --- | --- | --- | --- |
| P-001 | DONE | Tabs | tab lifecycle の責務を `app.rs` から分離し、初期化/保存/切替/移動の境界を module 化した |
| P-002 | DONE | Pipeline | index/search queue と incremental refresh を `app.rs` から分離した |
| P-003 | DONE | Cache | preview/highlight/cache helper を整理し、cache state と invalidation policy を局所化した |
| P-004 | DONE | Cleanup | docs 同期と一時 plan の撤去を完了した |

### Program B: Review Follow-ups
- Status: DONE on 2026-04-01
- Goal: リリース安全性・Windows 検証・CLI 契約・性能回帰検知・app architecture の弱点を段階的に是正する。
- Outcome:
  - Windows native CI、CLI `--limit` 契約、perf guard、docs/process 同期を完了した。

| ID | Status | Area | Summary |
| --- | --- | --- | --- |
| R-001 | DONE | CI | release/tag workflow と通常 CI の関係を整理し、tag 側でも test/audit 成功を前提化した |
| R-002 | DONE | Windows QA | Windows native runner を導入し、Windows 専用分岐を継続検証対象にした |
| R-003 | DONE | CLI contract | CLI `--limit` の 1000 件暗黙上限を撤廃し、契約を docs/help/test と一致させた |
| R-004 | DONE | Perf guard | ignored perf テストと workflow を整理し、自動回帰検知を追加した |
| R-005 | DONE | App architecture | `FlistWalkerApp` の責務を再分割し、state/coordinator/workflow 境界を明確化した |
| R-006 | DONE | Docs/process | release / validation / review 観点を docs に反映し、運用依存の暗黙知を減らした |

### Program C: App Architecture Roadmap
- Status: DONE on 2026-04-04
- Goal: request routing owner localization、render orchestration cleanup、final coordinator cleanup、docs/validation closure を二段 change plan で完了する。
- Outcome:
  - `cache.rs` が preview request routing owner API を担当する構造になった。
  - `tabs.rs` が action/sort request routing と active/background tab 向け response consume helper を担当する構造になった。
  - `render.rs` は `RenderCommand` queue を通じて dialog / tab bar / top action の interaction を owner helper へ橋渡しする構造になった。
  - `mod.rs` は `run_update_cycle()` などの helper seam を通す最終 coordinator に縮小した。
  - 2026-04-04 の docs/validation closure で roadmap / active slice plan / temporary rule を撤去し、恒久 docs のみを残した。

| Workstream | Status | Completed |
| --- | --- | --- |
| Request Routing Ownership | DONE | 2026-04-04 |
| Render/UI Orchestration | DONE | 2026-04-04 |
| Final Coordinator Cleanup | DONE | 2026-04-04 |
| Docs and Validation Closure | DONE | 2026-04-04 |

## Durable History
- 2026-04-08: 次の app architecture 改善を 2 段 change plan に再編し、`docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md` と `docs/CHANGE-PLAN-20260408-pipeline-owner-slice.md` を追加した。
- 2026-04-08: Slice A 完了後、次の child slice として `docs/CHANGE-PLAN-20260408-background-tab-result-flow-slice.md` を追加し、background tab result-flow separation に進める状態へ更新した。
- 2026-04-08: Slice B 完了後、次の child slice として `docs/CHANGE-PLAN-20260408-worker-protocol-separation-slice.md` を追加し、worker protocol separation に進める状態へ更新した。
- 2026-04-08: single-plan の architecture refactor program として `docs/CHANGE-PLAN-20260408-architecture-refactor.md` を追加し、`AGENTS.md` に temporary rule を追記した。
- 2026-04-08: architecture refactor follow-up program を close し、恒久内容を `ARCHITECTURE.md` / `DESIGN.md` / `TASKS.md` へ移したうえで temporary rule と `docs/CHANGE-PLAN-20260408-architecture-refactor.md` を削除した。
- 2026-04-04: app architecture roadmap closure のため、roadmap と active slice plan を削除する前に本ファイルへ完了理由と実施日を転記した。
- 2026-04-04: closure 完了後、app architecture 用 temporary rule を `AGENTS.md` から削除し、validation は `docs/TESTPLAN.md` の Validation Matrix を直接適用する運用へ戻した。
- 2026-04-08: 2026-04-08 のレビュー結果を踏まえ、`plan-driven-changes` 用の上位 roadmap として `docs/CHANGE-PLAN-20260408-improvement-roadmap.md` を追加した。
- 2026-04-08: 初回レビューの指摘を受け、child slice として `docs/CHANGE-PLAN-20260408-improvement-app-coordinator-slice.md` を追加した。roadmap 側の slice-level detail は上位計画向けに圧縮した。
- 2026-04-08: Slice A 完了後、次の child slice として `docs/CHANGE-PLAN-20260408-worker-domain-modularization-slice.md` を追加し、worker/domain modularization に進める状態へ更新した。
- 2026-04-08: Slice B 完了後、次の child slice として `docs/CHANGE-PLAN-20260408-os-integration-hardening-slice.md` を追加し、OS integration hardening に進める状態へ更新した。
- 2026-04-08: `plan-driven-changes` の roadmap と child slice を close し、関連 plan 文書を削除した。
- 2026-04-08: architecture debt closure 用の roadmap / slice / subslice を再追加し、`AGENTS.md` に一時ルールを復活させた。
- 2026-04-08: updater slice を close し、perf gate strengthening slice に切り替えた。
- 2026-04-08: architecture debt closure の Slice A Phase 1 を完了し、updater candidate resolution を staged apply から分離した。
- 2026-04-08: perf gate strengthening slice を close し、diagnostics and supportability slice に切り替えた。
- 2026-04-08: diagnostics and supportability slice の Phase 1 を完了し、update の supportability traces を request_id 対応にした。
- 2026-04-08: diagnostics and supportability slice の Phase 2 を完了し、supportability notes を docs 側へ同期した。
- 2026-04-08: diagnostics and supportability slice を close し、docs and closure restructuring slice に切り替えた。
- 2026-04-08: architecture debt closure program を close し、temporary rule と debt-program change-plan 文書を削除した。
