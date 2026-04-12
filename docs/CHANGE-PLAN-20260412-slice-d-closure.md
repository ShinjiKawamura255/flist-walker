# CHANGE PLAN: Closure Slice

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
- Scope Label: closure
- Related Tickets/Issues: none
- Review Status: レビュー中
- Review Notes:
  - 初回レビューで実装 scope が残っていたため、closure 専用に修正した。
  - この slice は実装を増やす場ではなく、理想形への到達判定を閉じる場である。

## 1. Background
- 大きなアーキテクチャ移行は、実装の完了と目標達成の確認を分けないと、いつ終わったかが曖昧になる。
- closure slice は、その曖昧さを排除し、roadmap を閉じるために必要である。

## 2. Goal
- 理想形の達成判定を行い、必要なテスト・文書・観測結果が揃っているかを確認する。
- 目標が達成されていれば roadmap を閉じ、未達なら追加 slice の目的と境界を明文化して継続する。

## 3. Scope
### In Scope
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTPLAN.md`
- `docs/REQUIREMENTS.md`
- `docs/SPEC.md`

### Out of Scope
- 新しい機能追加
- 大きな追加リファクタリング
- 残存実装の継続作業

## 4. Constraints and Assumptions
- closure slice は goal validation を主目的とし、スコープ拡張はしない。
- 追加 slice が必要な場合でも、それは closure で明文化してからにする。

## 5. Current Risks
- Risk: closure で未達点を曖昧にしたまま roadmap を閉じる。
  - Impact: 理想形の完成度が不明瞭になる。
  - Mitigation: goal 未達項目を列挙し、継続 slice を作るか close するかを明記する。
- Risk: 検証不足のまま closure になる。
  - Impact: 後続の回帰に気づけない。
  - Mitigation: full validation matrix を必須にする。

## 6. Execution Strategy
1. Phase D1: goal validation and evidence collection
   - Files/modules/components: architecture/design/testplan docs, relevant app/search/indexer touch points
   - Expected result: 理想形の達成基準が満たされたかを証拠付きで確認できる。
   - Verification: full validation matrix, targeted perf/regression checks, doc consistency review.
2. Phase D2: close-or-continue decision
   - Files/modules/components: roadmap and related slice docs
   - Expected result: roadmap を完了として閉じるか、追加 slice を定義して継続するかが決まる。
   - Verification: recorded closure notes and explicit next-step decision.

## 7. Detailed Task Breakdown
- [ ] goal 達成の証拠を集める。
- [ ] 必要なテストと文書の整合を確認する。
- [ ] roadmap を閉じるか継続するかを記録する。

## 8. Validation Plan
- Automated tests:
  - full `cargo test`
  - targeted app/search/indexer regression tests
  - perf regressions where applicable
- Manual checks:
  - architecture docs read-through
  - end-to-end workflow sanity check
- Performance or security checks:
  - indexing and search latency budget
  - stale response and routing safety
- Regression focus:
  - goal completeness
  - docs/test traceability

## 9. Rollback Plan
- If closure shows that the roadmap is still missing a material target, do not weaken the target. Add a new slice with a precise boundary.
- If the closure doc itself becomes obsolete, update the roadmap first and then this slice.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-idealization`, read the roadmap and earlier slices, then use this closure slice to validate completion.
- Do not close the roadmap until goal validation is recorded.
- If the roadmap remains open, define the next slice explicitly before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12  Planned.

## 12. Communication Plan
- Return to user after goal validation and the close-or-continue decision are recorded.

## 13. Completion Checklist
- [ ] Plan created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Goal validation completed
- [ ] Close-or-continue decision recorded
- [ ] Temporary rule removed after completion

## 14. Final Notes
- This slice is the guardrail that prevents the roadmap from being declared “done” merely because the code stopped moving.
