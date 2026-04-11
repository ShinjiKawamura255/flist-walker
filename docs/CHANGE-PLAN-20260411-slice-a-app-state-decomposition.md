# CHANGE PLAN: Slice A App State Decomposition

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: inherited from parent roadmap (`autonomous`)
- Execution Mode Policy: parent roadmap の `autonomous` policy に従う。main agent は active phase を確定し、phase 実行は原則 subagent に委譲する。
- Parent Plan: docs/CHANGE-PLAN-20260411-roadmap-app-structure-followup.md
- Child Plan(s): none
- Scope Label: slice-a-app-state-decomposition
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-11 initial review で、slice が広すぎることと phase 順序を tab/session boundary 先行へ入れ替えるべきという指摘を受けた。
  - 対応として、Phase 1 を inventory gate へ狭め、Phase 2 を tab/session boundary tightening、Phase 3 を app-global / feature bundle extraction の順へ修正した。
  - `subslice` は初期作成しない。Phase 2/3 のどちらかが実行前 review で大きすぎると判定された場合のみ追加する。
  - 2026-04-11 convergence review で execution-ready と確認されたため、本 slice を `レビュー済み` とする。

## 1. Background
- `FlistWalkerApp` は依然として app-wide state を多数の field で保持し、owner modules も `use super::*;` と `impl FlistWalkerApp` 経由で広いスコープに依存している。
- `ui_state.rs` / `query_state.rs` / `state.rs` / `tab_state.rs` は型定義の整理には効いているが、ownership 自体は `FlistWalkerApp` に残っている。
- 次の `search.rs` / `indexer.rs` 分割へ進む前に、app coordinator の state cohesion を改善しないと変更影響範囲が広すぎる。

## 2. Goal
- `FlistWalkerApp` 直下の field 群を、少なくとも `app shell`, `search/index state`, `tab/session state`, `feature dialog/update state` のような機能ドメイン単位へ再配置する。
- owner modules が必要な state bundle を明示的に受け取る構造へ寄せ、`mod.rs` の coordinator と feature policy の境界をもう一段狭める。
- 既存の tab/session/filelist/update regression を保ったまま、次 slice の module split の土台を作る。

## 3. Scope
### In Scope
- `FlistWalkerApp` field の grouping と sub-struct 化
- `rust/src/app/state.rs` への state bundle 追加または再編
- owner module の API 調整
- state decomposition に伴う app tests と docs 更新

### Out of Scope
- `search.rs` / `indexer.rs` 自体の module split
- 新しい設定ファイル導入
- workflow/CI 変更

## 4. Constraints and Assumptions
- tab/session restore、background tab 応答 apply、filelist/update dialog lifecycle を崩さない。
- UI 非ブロック原則のため、worker channel や request_id 管理は ownership を移しても挙動を変えない。
- `FlistWalkerApp` を一気に消すのではなく、owner を狭める段階改善として扱う。

## 5. Current Risks
- Risk:
  - field regrouping が広範囲の compile error と borrow conflict を誘発する。
  - Impact:
    - 実装速度低下、暫定 workaround 増加。
  - Mitigation:
    - phase ごとに state bundle を限定し、app tests を phase 単位で補強する。
- Risk:
  - tab/session state と active/shared state の境界が中途半端に残る。
  - Impact:
    - 後続 slice でも `&mut self` 依存が残存する。
  - Mitigation:
    - active tab/local state と app-global shared state を phase で明示分割し、docs に owner 境界を残す。

## 6. Execution Strategy
1. Phase 1: state inventory and target bundles
   - Files/modules/components:
     - `rust/src/app/mod.rs`, `rust/src/app/state.rs`, `docs/ARCHITECTURE.md`, `docs/DESIGN.md`
   - Expected result:
     - `FlistWalkerApp` field を app-global shared state / active-tab-local state / persisted-background state / feature dialog-update state に分類する inventory gate が固定される。
   - Verification:
     - compile/test impact review
2. Phase 2: tab/session boundary tightening
   - Files/modules/components:
     - `rust/src/app/tabs.rs`, `rust/src/app/session.rs`, `rust/src/app/tab_state.rs`, related tests
   - Expected result:
     - active tab と persisted/background tab の state 境界が明確になり、snapshot/apply cost と restore concern が局所化される。
   - Verification:
     - `cargo test`
3. Phase 3: app-global and feature state extraction
   - Files/modules/components:
     - `rust/src/app/mod.rs`, `rust/src/app/state.rs`, `rust/src/app/ui_state.rs`, `rust/src/app/query_state.rs`, `rust/src/app/cache.rs`, related tests
   - Expected result:
     - shared app shell / query / cache / dialog state の少なくとも一部が dedicated bundle を通して所有される。
   - Verification:
     - `cargo test`
4. Phase 4: owner API cleanup and docs sync
   - Files/modules/components:
     - owner modules, `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
   - Expected result:
     - owner modules が state bundle を前提にした API surface へ揃い、恒久 docs と validation が同期する。
   - Verification:
     - `cargo test`
     - doc diff review

## 7. Detailed Task Breakdown
- [ ] `FlistWalkerApp` field inventory を bundle 方針へ整理する
- [ ] tab/session 境界を tighten する
- [ ] app-global / feature state bundle を導入する
- [ ] owner API と docs/test を同期する

## 8. Validation Plan
- Automated tests:
  - `cargo test`
- Manual checks:
  - 必要に応じて Structural Refactoring GUI Smoke Test の Step 4/5/6 を実施
- Performance or security checks:
  - 本 slice では perf guard は不要。ただし indexing path へ触れた場合は VM-003 を追加適用する。
- Regression focus:
  - root change cleanup
  - tab switch/close/reorder
  - session restore
  - filelist/update dialog lifecycle

## 9. Rollback Plan
- Phase 2 と Phase 3 を別 commit に分け、state regrouping と tab/session boundary change を独立して戻せるようにする。
- inventory gate の結果で Phase 2/3 の境界が保てない場合は、実装開始前に `subslice` を追加してから進める。
- docs sync は最後の phase に寄せ、コード rollback 時に docs mismatch を残さない。

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
- 2026-04-11 12:20 Planned initial Slice A draft.
- 2026-04-11 12:55 Phase 1 completed. inventory gate として `FlistWalkerApp` field の 4-way classification を docs へ反映した。Phase 2 は tab/session boundary tightening へ進む。
- 2026-04-11 14:35 Phase 2 completed. `tabs` field は `TabSessionState` bundle へ置き換え、`active_tab` / `next_tab_id` / `pending_restore_refresh` / request routing を tab/session registry としてまとめた。`cargo test` green。

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
- If review concludes that Phase 2 and Phase 3 are still too large to execute safely, add `subslice` documents before implementation instead of stretching this slice ad hoc.
