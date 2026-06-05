# Durable History

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
