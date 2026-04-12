# CHANGE PLAN: Core Boundary and Contract Stabilization

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260412-roadmap-architecture-idealization.md](./CHANGE-PLAN-20260412-roadmap-architecture-idealization.md)
- Child Plan(s): none
- Scope Label: core-boundary
- Related Tickets/Issues: none
- Review Status: 実装済み
- Review Notes:
  - 初回レビューで A1 の完了条件が曖昧だと指摘された。
  - 追加レビューでは計画自体は feasible と判断された。
  - この slice は理想形の core 契約を確定する前提ゲートである。
  - shell 側の都合で core 契約を削らない。

## 1. Background
- `search` と `indexer` の核は比較的良いが、いくつかの表示用・協調用 helper が shell 側と密接に見える。
- 理想形では、core が shell に依存せず、UI から独立した pure contract として維持される必要がある。

## 2. Goal
- `entry` / `query` / `search` / `indexer` / `ui_model` / `path_utils` の契約を固定し、shell 依存を最小化する。
- worker protocol は shell と core の境界を跨ぐ transport として扱い、業務ロジックを含めない。
- 後続 slice で shell を分解しても、core 契約は揺れない状態にする。

## 3. Scope
### In Scope
- `rust/src/query.rs`
- `rust/src/search/*`
- `rust/src/indexer/*`
- `rust/src/entry.rs`
- `rust/src/ui_model.rs`
- `rust/src/path_utils.rs`
- `rust/src/app/worker_protocol.rs`

### Out of Scope
- tab/session orchestration の再構成
- render/input 層の分離
- UI state の projection 再設計

## 4. Constraints and Assumptions
- fzf 互換 query 契約は壊さない。
- FileList 優先と walker fallback の意味は維持する。
- Windows path 正規化と visible match の挙動は後方互換を維持する。

## 5. Current Risks
- Risk: core 境界を切るつもりが、search/indexer の実装 detail を shell へ移すだけで終わる。
  - Impact: 境界が見かけ倒しになる。
  - Mitigation: pure API を定義し、shell 側の直接アクセスを禁止する。
- Risk: 契約の過剰抽象化で performance を落とす。
  - Impact: 100k candidates の応答性が悪化する。
  - Mitigation: allocation と clone の増減を必ず確認する。

## 6. Execution Strategy
1. Phase A1: canonical contract inventory
   - Files/modules/components: `entry.rs`, `query.rs`, `search/mod.rs`, `indexer/mod.rs`, `ui_model.rs`, `path_utils.rs`
   - Expected result: core 境界の責務表が揃い、どの関数が pure であるべきかが明確になる。
   - Verification: contract-focused unit tests and doc assertions.
   - Exit condition: core API 表、pure/public 関数一覧、shell 依存禁止対象が文書化され、A2 着手判断に使える状態になる。
2. Phase A2: pure core extraction and dependency tightening
   - Files/modules/components: `search/*`, `indexer/*`, `query.rs`, `ui_model.rs`, `path_utils.rs`
   - Expected result: shell helper への依存を減らし、core だけで意味が完結する。
   - Verification: existing search/indexer tests, boundary regression checks.
   - Entry condition: A1 の inventory がレビュー済みで、移動/削除対象の関数が確定している。
   - Exit condition: `search` と `ui_model` の presentation/helper 依存が純粋 core 側へ移設または分離されている。
3. Phase A3: protocol and compatibility pinning
   - Files/modules/components: `app/worker_protocol.rs`, `search/mod.rs`, `indexer/mod.rs`
   - Expected result: worker requests/responses and core contracts are stable, explicit, and testable.
   - Verification: compile-time checks and targeted protocol tests.
   - Entry condition: A2 で core boundary が固まり、protocol に残すべき契約が明確になっている。
   - Exit condition: protocol と core 契約のテストが揃い、shell/core で型と責務がぶれない。

## 7. Detailed Task Breakdown
- [ ] core API と pure helper を inventory する。
- [ ] query/search/indexer の契約を shell 依存なしで成立させる。
- [ ] worker protocol の責務を transport に限定する。
- [ ] visible match / path normalization / ranking contract を固定する。

## 8. Validation Plan
- Automated tests:
  - `cargo test` for `search` and `indexer`
  - existing query/operator tests
- Manual checks:
  - query operator parity with current behavior
  - Windows path display normalization sanity check
- Performance or security checks:
  - 100k candidate search benchmark regression watch
- Regression focus:
  - exact / exclusion / anchor behavior
  - relative vs absolute visible match consistency
  - FileList/walker parity on current inputs

## 9. Rollback Plan
- Keep core API additions additive until shell adoption is complete.
- If a contract proves too ambitious, revert the shell adaptation first and retain the pure helper extraction that is already stable.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-idealization`, read the roadmap before starting implementation, then follow slice-a before later slices.
- Keep the core boundary stable and do not trim the target to match current shell limitations.
- If scope or ordering changes, update the plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12  Planned.
- 2026-04-12  Slice A implemented and validated with `cargo test`.

## 12. Communication Plan
- Return to user when this slice is reviewed and ready for implementation or when a blocking issue prevents continuation.

## 13. Completion Checklist
- [x] Plan created before implementation
- [x] Slice A boundary changes implemented
- [x] Validation passed
- [ ] Temporary `AGENTS.md` rule added
- [ ] Core contracts stabilized
- [ ] Verification completed
- [ ] Temporary rule removed after completion

## 14. Final Notes
- This slice is intentionally conservative about behavioral changes and aggressive about boundary clarity.
