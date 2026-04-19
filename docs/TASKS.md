# TASKS

## Status Snapshot
- Updated: 2026-04-19
- Current active engineering roadmap: `docs/EXECUTION-PLAN-20260419-roadmap-quality-maturity-uplift.md`
- Current active engineering change plan: `docs/EXECUTION-PLAN-20260419-slice-a-quality-baseline-gates.md`
- App architecture change-plan program: DONE on 2026-04-09
- Notes:
  - 2026-04-19 に外部の多角的評価（総合 72/100、低評価: 運用成熟度 40、保守性・拡張性 62、コード品質 68、GUI テスト不足）を受け、`quality-maturity-uplift` roadmap を作成した。初手は `coverage gate` / GUI render validation strategy / supportability without telemetry を扱う Slice A とし、大規模な `render.rs` 分割や app boundary tightening は測定可能な gate を入れてから進める方針にした。
  - 2026-04-19 に Slice A の coverage baseline を測定し、line coverage 70.29%（LH=9870 / LF=14042）を確認した。CI の coverage command は `--fail-under-lines 70` を付け、現状を通しつつ 70% 未満への退行を落とす gate とした。
  - 2026-04-17 に Slice G の boundary tightening を完了し、`FileListManager` / `UpdateManager` の透過露出をやめて `workflow` / `state` 明示境界へ統一した。`cargo test --quiet` は無警告で green、closure scoring は 92/100 となり roadmap を close した。
  - 2026-04-17 に architecture score 78 評価を受け、90 点到達に向けた不完全な ad-hoc roadmap を `plan-driven-execution` 準拠の execution plan 群へ再構成した。上位 plan は `docs/EXECUTION-PLAN-20260417-roadmap-architecture-score-90.md`、active slice は `docs/EXECUTION-PLAN-20260417-slice-a-coordinator-surface-reduction.md`、終端は `docs/EXECUTION-PLAN-20260417-slice-f-architecture-score-closure.md` とし、roadmap は `Execution Mode: autonomous`、closure は別 subagent の 100 点満点評価で 90/100 以上を条件に close する形へ更新し、`AGENTS.md` に temporary rule を追記した。
  - 2026-04-17 に Slice A の実装を開始し、`FlistWalkerApp` の定数群を `rust/src/app/config.rs` へ外出しして `mod.rs` を coordinator entrypoint 寄りに縮小した。`cargo test` は green、`cargo test --quiet` は成功したが、既存の unused import 警告は baseline として残っている。
  - 2026-04-17 に Slice A の warning cleanup を完了し、`cargo test --quiet` を無警告で通したうえで `cargo test` も再確認した。Slice B を active change plan に切り替えた。
  - 2026-04-17 に Slice B の owner relocation を進め、`FileListManager` と `UpdateManager` の command/state lifecycle を `filelist.rs` / `update.rs` へ戻して `state.rs` を shared boundary 寄りに薄くした。`cargo test --quiet` を再確認し、次の active change plan を Slice C に切り替えた。
  - 2026-04-17 に Slice C の presentation boundary cleanup を開始し、`ui_model.rs` から `choose_action` 参照を外して preview text の生成を display concern に寄せた。`cargo test --quiet` は引き続き green。
  - 2026-04-17 に Slice D の worker concern separation を完了し、worker spawn/use-case 実装を `worker_tasks.rs` に退避して `workers.rs` を registry shim にした。`cargo test --quiet` を再確認し、次の active change plan を Slice E に切り替えた。
  - 2026-04-17 に Slice E の enforcement through tests を完了し、`ui_model` preview text の display-only guard を追加して `ARCHITECTURE.md` / `TESTPLAN.md` を同期した。`cargo test --quiet` を再確認し、次の active change plan を Slice F に切り替えた。
  - 2026-04-17 に Slice F の closure scoring が 86/100 となり continue 判定を受けたため、`Deref` boundary と worker_tasks docs sync を締める Slice G を追加し、次の active change plan を Slice G に切り替えた。
  - 2026-04-17 に `DerefMut` 除去と tab-state contract test を完了し、`cargo test` を通した。Slice C の closure validation では `Closed: Slice A の ownership boundary 完了, Slice B の import hygiene 完了, cargo test green` / `Deferred: none` / `Blocked: none` と判定し、follow-up roadmap を閉じた。
  - 2026-04-14 に architecture score 80 follow-up roadmap を再編し、Tab-Shell 二重所有 / `DerefMut` / `use super::*;` 汚染 / local clone hot path / closure validation を別 slice で機械的に閉じる方針へ切り替えた。
  - 2026-04-14 に Slice A の review を受け、`DerefMut` 除去と tab-state contract test の方向で GO を得た。
  - 2026-04-14 に Slice B の review を受け、import hygiene と局所 cleanup の方向で GO を得た。
  - 2026-04-14 に Slice C の review を受け、close/continue の fixed rubric と `Closed` / `Deferred` / `Blocked` 記録フォーマットの方向で GO を得た。
  - 2026-04-12 に旧 state ownership consolidation program を起点に、runtime/tab snapshot の二重管理と event-routing の direct mutation を整理する方針へ切り替えた。
  - 2026-04-12 に旧 state ownership consolidation program の closure validation を行い、残課題がまだ material だと判断したため後続の state sync finalization program へ再計画した。
  - 2026-04-12 に state sync finalization program を新規作成し、残る live/snapshot ownership overlap をさらに削る次の pass へ切り替えた。
  - 2026-04-12 に state sync finalization の実装 slice を完了し、`query_history_dirty_since` を runtime-only に寄せ、`pending_restore_refresh_tabs` で tab restore pending を管理するようにしたうえで `cargo test` を通した。
  - 2026-04-12 に state sync finalization program を closure し、残る overlap は設計上の live/snapshot split として許容可能と判断した。
  - 2026-04-12 に shell boundary closure roadmap を完了し、`FlistWalkerApp` の透明な shell 露出を除去したうえで `status_line` を render-time derived data として扱うように切り替え、`cd rust && cargo test` を通した。
  - 2026-04-12 に architecture score uplift roadmap と slice A/B/C の change-plan 文書を作成し、`AGENTS.md` に temporary rule を追記した。
  - 2026-04-13 に architecture score uplift roadmap を closure し、Slice A/B で shell boundary と routing/lifecycle ownership を締めたうえで `cargo test` を通し、temporary rule と change-plan 文書を撤去した。
  - 2026-04-12 の外部レビューで、現在の architecture は 60〜65 点相当と評価され、`Deref` 連鎖、direct mutation、state duplication、imperative UI refresh が主な未解消点として指摘された。
  - 2026-04-12 に app-shell/use-case decoupling roadmap と initial slice A を作成し、`AGENTS.md` に temporary rule を追記した。
  - 2026-04-12 に Slice A を実装し、`AppShellState` で app shell を包み、`cd rust && cargo test` を通した。次の slice を起こす前に roadmap 側へ結果を反映した。
  - 2026-04-12 に Slice B を実装し、result/preview/sort response handling を reducer boundary へ移したうえで `cd rust && cargo test` を再実行した。
  - 2026-04-12 に Slice B を完了し、result/preview/sort response handling を reducer boundary へ移した。
  - 2026-04-12 に前段の closure validation を実施し、`cargo test` を再実行したうえで roadmap は未閉鎖と判断した。残課題は `FlistWalkerApp` の透明な shell 露出、`status_line` の命令的更新、そして direct mutation / derived UI state の整理である。
  - 2026-04-12 に shell boundary closure roadmap を新規作成し、残課題を `Deref` 廃止と `status_line` の派生化に絞った。
  - 2026-04-12 に app-shell/use-case decoupling roadmap を close し、temporary rule を撤去して change-plan 文書群を削除した。
  - 2026-04-12 に app-shell/use-case decoupling roadmap を復元し、継続判断用 Slice C を追加して plan-driven-changes の再開点へ戻した。
  - 2026-04-12 に Slice C で continue を判断し、残る shell-policy/helper extraction を Slice D として追加した。
  - 2026-04-12 に Slice D を完了し、`shell_support.rs` へ shell/runtime helper policy を移して `mod.rs` を薄くしたうえで `cd rust && cargo test` を通した。
  - 2026-04-12 に Slice E で closure judgment を完了し、roadmap を閉じて temporary plan machinery を撤去した。
  - 2026-04-12 に architecture-idealization roadmap を closure し、core boundary / shell decomposition / routing cleanup を恒久 docs へ移したうえで temporary rule と change-plan 文書を撤去した。
  - 2026-04-11 に app architecture boundary cleanup の single plan を追加し、`AGENTS.md` に temporary rule を追記した。
  - 2026-04-11 に plan review を完了し、`path_key` を `path_utils.rs` へ移し、`result_flow.rs` へ result orchestration を分離したうえで `cargo test` を通した。
  - 2026-04-11 に app architecture boundary cleanup を完了し、temporary rule と change plan を撤去した。
  - app architecture の multi-slice refactor は closure まで完了し、恒久 docs だけを残す状態へ移行した。
  - 2026-04-11 に app state cohesion / ownership transfer の next-step roadmap を再導入し、`FlistWalkerApp` の field cohesion と ownership transfer を別 slice で進める方針へ切り替えた。
  - 2026-04-11 に app state cohesion / ownership transfer roadmap を autonomous で完遂し、`AppRuntimeState` を導入して coordinator の direct field surface を runtime bundle へ寄せ、preview/highlight ownership を `preview_flow.rs` に分離したうえで roadmap / slice / temporary rule を撤去した。
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
  - 2026-04-09 に Slice C を完了し、`worker_protocol.rs` へ worker request/response 型を集約して `workers.rs` / `index_worker.rs` / `worker_bus.rs` / `mod.rs` の protocol 参照境界を同期した。
  - 2026-04-09 に Slice D 向けの active child plan `docs/CHANGE-PLAN-20260409-command-oriented-app-tests-slice.md` を追加し、command-oriented app tests へ進む状態に切り替えた。
  - 2026-04-09 に Slice D Phase 1/2 を進め、update command/manager 系テストを `app_core.rs` から `rust/src/app/tests/update_commands.rs` へ分離した。
  - 2026-04-09 に Slice D Phase 2 を完了し、session restore/startup-root 系テストを `session_tabs.rs` から `rust/src/app/tests/session_restore.rs` へ分離して `cargo test` で回帰がないことを確認した。
  - 2026-04-09 に Slice D Phase 3 を完了し、`ARCHITECTURE.md` / `DESIGN.md` / `TESTPLAN.md` / roadmap を command-oriented な test boundary に同期した。
  - 2026-04-09 に Slice E 向けの active child plan `docs/CHANGE-PLAN-20260409-structured-tracing-supportability-slice.md` を追加し、structured tracing/supportability と roadmap closure review へ進む状態に切り替えた。
  - 2026-04-09 に Slice E を完了し、worker-side trace の canonical `flow` / `event` / `request_id` 契約、index trace smoke、supportability docs を同期した。
  - 2026-04-09 に `cargo test`、VM-003 perf guard 2 本、`RUST_LOG=flist_walker::app::index_worker=info cargo test index_worker_trace_smoke_emits_canonical_fields --lib -- --nocapture` を実施し、roadmap closure 条件を満たすことを確認した。
  - 2026-04-09 に app architecture improvement roadmap を close し、`AGENTS.md` の temporary rule と関連 change-plan 文書群を撤去する状態へ移行した。

## Completed Programs

### Program G: Architecture Idealization Roadmap
- Status: DONE on 2026-04-12
- Goal: core boundary stabilization、shell decomposition、routing/lifecycle cleanup、closure validation を段階的に実施し、理想形を削らずに閉じる。
- Outcome:
  - `search` / `indexer` / `query` / `ui_model` / `path_utils` の core boundary を固定し、presentation/helper 依存を整理した。
  - `app/` shell は bootstrap、tab/session state、response routing、root browser lifecycle の owner 群へ分解した。
  - `cargo test` を通し、last-mile の routing/lifecycle cleanup まで既存回帰を発生させずに収束した。
  - `ARCHITECTURE.md` / `DESIGN.md` / `TASKS.md` を恒久化し、change-plan 文書群と temporary rule を撤去した。

| Slice | Status | Completed |
| --- | --- | --- |
| Slice A: Core Boundary and Contract Stabilization | DONE | 2026-04-12 |
| Slice B: Shell Decomposition and State Ownership | DONE | 2026-04-12 |
| Slice C: Routing and Lifecycle Cleanup | DONE | 2026-04-12 |
| Slice D: Closure Validation and Decision | DONE | 2026-04-12 |

### Program F: App Architecture Improvement Roadmap
- Status: DONE on 2026-04-09
- Goal: pipeline owner、background result flow、worker protocol、app test boundary、structured tracing/supportability を slice 単位で段階改善し、最終的に roadmap を閉じる。
- Outcome:
  - `pipeline_owner.rs` と `tabs.rs` を軸に active/background の owner 境界を明確化した。
  - `worker_protocol.rs` と owner-aligned app test modules により protocol/test seams を薄く保守できる構造へ寄せた。
  - worker-side diagnostics は canonical `flow` / `event` / `request_id` field を中心に揃え、index trace smoke と docs で supportability contract を固定した。
  - `AGENTS.md` temporary rule と roadmap/slice change-plan 文書を closure 時に撤去し、恒久情報は `ARCHITECTURE.md` / `DESIGN.md` / `TESTPLAN.md` / `TASKS.md` へ移した。

| Slice | Status | Completed |
| --- | --- | --- |
| Slice A: Pipeline Owner Extraction | DONE | 2026-04-08 |
| Slice B: Background Tab Result-Flow Separation | DONE | 2026-04-08 |
| Slice C: Worker Protocol Separation | DONE | 2026-04-09 |
| Slice D: Command-Oriented App Tests | DONE | 2026-04-09 |
| Slice E: Structured Tracing and Supportability | DONE | 2026-04-09 |

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
- 2026-04-09: Slice C 完了後、次の child slice として `docs/CHANGE-PLAN-20260409-command-oriented-app-tests-slice.md` を追加し、command-oriented app tests に進める状態へ更新した。
- 2026-04-09: Slice D 完了後、次の child slice として `docs/CHANGE-PLAN-20260409-structured-tracing-supportability-slice.md` を追加し、structured tracing/supportability と roadmap closure review に進める状態へ更新した。
- 2026-04-09: Slice E を完了し、worker-side trace contract と diagnostics docs を同期したうえで `cargo test`、VM-003 perf guard、index trace smoke を通した。
- 2026-04-09: app architecture improvement roadmap を close し、`AGENTS.md` の temporary rule と `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md`、`docs/CHANGE-PLAN-20260408-pipeline-owner-slice.md`、`docs/CHANGE-PLAN-20260408-background-tab-result-flow-slice.md`、`docs/CHANGE-PLAN-20260408-worker-protocol-separation-slice.md`、`docs/CHANGE-PLAN-20260409-command-oriented-app-tests-slice.md`、`docs/CHANGE-PLAN-20260409-structured-tracing-supportability-slice.md` を削除した。
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
