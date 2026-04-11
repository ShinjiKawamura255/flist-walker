# CHANGE PLAN: Slice B Search Indexer Decomposition

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: inherited from parent roadmap (`autonomous`)
- Execution Mode Policy: parent roadmap の `autonomous` policy に従う。main agent は active phase を確定し、phase 実行を進め、phase ごとに検証と commit を閉じる。
- Parent Plan: docs/CHANGE-PLAN-20260411-roadmap-app-structure-followup.md
- Child Plan(s): none
- Scope Label: slice-b-search-indexer-decomposition
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-11 feasibility review で、`search.rs` と `indexer.rs` はどちらも 1,600 行超だが、public API は限定されており、まず module 境界だけを抽出して互換 surface を維持する方針が成立すると確認した。
  - 2026-04-11 slice review で、`search` と `indexer` を別々の phase で扱い、最後に import/perf/docs sync を閉じる 4 phase 構成なら 2 段のまま安全に実行できると判断した。
  - 外部要因で subagent review は利用できなかったため、main agent が fallback review を実施した。この例外は usage limit による一時的制約に限定する。

## 1. Background
- `rust/src/search.rs` は query compile、candidate match、score/ranking、parallel execution、prefix cache、materialization を 1 ファイルへ抱えている。
- `rust/src/indexer.rs` は FileList parse、nested FileList override、walker、index build、filelist write/propagation を 1 ファイルへ抱えている。
- Slice A で `FlistWalkerApp` 側の ownership を bundle 化したため、Slice B では app coordinator から独立した search/index domain の責務分割へ進める。

## 2. Goal
- `search` domain を query compilation / match-score / execution-config / public API の owner module へ分離する。
- `indexer` domain を FileList read / nested override / walker / index build / filelist write の owner module へ分離する。
- 既存の public API、query operator 契約、FileList 優先契約、VM-003 perf guard を維持したまま、以後の変更を局所化できる形へ寄せる。

## 3. Scope
### In Scope
- `rust/src/search.rs` の module directory 化と責務分割
- `rust/src/indexer.rs` の module directory 化と責務分割
- 既存 unit/app tests の import 維持または最小更新
- `ARCHITECTURE.md` / `DESIGN.md` / `TESTPLAN.md` / roadmap 進捗同期

### Out of Scope
- query syntax の仕様変更
- FileList 探索契約の変更
- 新しい perf framework や benchmark harness 導入
- Slice C で扱う CI / coverage / config / dependency hygiene

## 4. Constraints and Assumptions
- `pub fn search_entries_with_scope`, `try_search_entries_with_scope`, `try_search_entries_indexed_with_scope`, `rank_search_results` など外部 surface は互換維持する。
- `indexer` 側の `build_index`, `build_index_with_metadata`, `find_filelist_in_first_level`, `write_filelist_cancellable` など既存 surface は維持する。
- `rust/src/indexer.rs` を変更するため、最終 phase では `cargo test` に加えて VM-003 perf guard 2 本を実行する。
- 初手ではアルゴリズム変更を避け、module boundary 抽出と import 整理を主目的にする。

## 5. Current Risks
- Risk:
  - search module split で private helper の visibility と test scope が壊れる。
  - Impact:
    - query/operator regression、ranking 差分、prefix cache 利用崩れ。
  - Mitigation:
    - public API は `search/mod.rs` に残し、internal module は `pub(crate)` helper に限定する。
- Risk:
  - indexer split で nested FileList override と writer 側 helper の依存が循環する。
  - Impact:
    - FileList 優先契約の regression、write path の破綻。
  - Mitigation:
    - `filelist_read`, `walker`, `build`, `filelist_write` の 4 owner を先に固定し、共通 helper は `shared` 相当 module へ寄せる。
- Risk:
  - module move 後に perf regression を見落とす。
  - Impact:
    - FileList / Walker 初期 index 速度の後退。
  - Mitigation:
    - Slice B closure で VM-003 perf guard 2 本を必ず回す。

## 6. Execution Strategy
1. Phase 1: search module map and extraction
   - Files/modules/components:
     - `rust/src/search.rs` -> `rust/src/search/`
   - Expected result:
     - search が `compile`, `matcher`, `execute`, `cache`, `api` 相当の責務へ分かれ、既存 public API は `search/mod.rs` から再公開される。
   - Verification:
     - `cargo test`
2. Phase 2: indexer module map and extraction
   - Files/modules/components:
     - `rust/src/indexer.rs` -> `rust/src/indexer/`
   - Expected result:
     - indexer が `filelist_read`, `walker`, `build`, `filelist_write` 相当の責務へ分かれ、既存 public API は `indexer/mod.rs` から再公開される。
   - Verification:
     - `cargo test`
3. Phase 3: import cleanup and compatibility sync
   - Files/modules/components:
     - `rust/src/lib.rs`, app/worker imports, tests, docs
   - Expected result:
     - callsite import が新 module layout に整列し、public/private visibility が安定する。
   - Verification:
     - `cargo test`
4. Phase 4: perf/docs closure
   - Files/modules/components:
     - `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, roadmap/slice logs
   - Expected result:
     - module ownership と validation が恒久 docs に同期し、VM-003 perf guard を通した状態で Slice B を閉じられる。
   - Verification:
     - `cargo test`
     - `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`
     - `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
     - doc diff review

## 7. Detailed Task Breakdown
- [x] `search` を module directory へ分割する
- [x] `indexer` を module directory へ分割する
- [x] import / visibility / test scope を同期する
- [x] perf guard と docs を同期する

## 8. Validation Plan
- Automated tests:
  - `cargo test`
  - VM-003 perf guard 2 本
- Manual checks:
  - 追加の GUI manual は不要。必要なら CLI search smoke のみ確認する。
- Performance or security checks:
  - query operator 契約と FileList / Walker budget regression を重視する。
- Regression focus:
  - query compile/match/rank
  - prefix cache
  - FileList detection / nested override / write propagation
  - app worker import compatibility

## 9. Rollback Plan
- search extraction、indexer extraction、docs/perf closure を別 commit に分ける。
- `indexer` split で perf guard が悪化した場合は、indexer commit を独立して戻せる状態にする。
- public API surface を変えないため、rollback は module layout のみを巻き戻すことを基本とする。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-structure-followup`, read the relevant change plan document(s) before starting implementation.
- Read them from upper to lower order:
  - `[docs/CHANGE-PLAN-20260411-roadmap-app-structure-followup.md]`
  - `[docs/CHANGE-PLAN-20260411-slice-b-search-indexer-decomposition.md]`
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Because the roadmap says `Execution Mode: autonomous`, continue autonomously through slice creation, review, phase execution, roadmap updates, and phase commits until the roadmap is complete unless a blocking problem occurs.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-11 19:35 Planned Slice B draft.
- 2026-04-11 19:40 Feasibility review completed. 2 段のまま `search` / `indexer` / closure の phase 分割で進められると判断した。
- 2026-04-11 20:15 Phase 1 completed. `search.rs` を `search/mod.rs` + `cache/config/execute/rank` へ分割し、既存 public search API を維持したまま `cargo test` green を確認した。
- 2026-04-11 20:45 Phase 2-3 completed. `indexer.rs` を `indexer/mod.rs` + `filelist_reader/walker/filelist_writer` へ分割し、test/import compatibility を同期したうえで `cargo test` green を確認した。
- 2026-04-11 20:55 Phase 4 completed. VM-003 perf guard 2 本と docs 同期を通し、Slice B を closure-ready にした。

## 12. Communication Plan
- Return to user when:
  - all roadmap phases are complete
  - a phase cannot continue without resolving a blocking problem
- If the project is under git control, commit at the end of each completed phase.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Work executed according to the plan or the plan updated first
- [x] If the project is under git control, each completed phase was committed separately
- [x] Verification completed
- [x] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- indexer split は perf budget に触れるため、closure 前に VM-003 を省略しない。
