# TASKS

## Status Snapshot
- Updated: 2026-04-04
- Current active engineering roadmap: なし
- App architecture change-plan program: DONE
- Notes:
  - app architecture の multi-slice refactor は closure まで完了し、一時 plan 運用の撤去段階へ入った。
  - 新しい大規模 workstream を開始する場合は、必要に応じて別の change plan / roadmap を起こす。

## Completed Programs

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
- 2026-04-04: app architecture roadmap closure のため、roadmap と active slice plan を削除する前に本ファイルへ完了理由と実施日を転記した。
- 2026-04-04: closure 完了後は app architecture 用 temporary rule を `AGENTS.md` から削除し、validation は `docs/TESTPLAN.md` の Validation Matrix を直接適用する運用へ戻す。
