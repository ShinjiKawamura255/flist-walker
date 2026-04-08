# CHANGE PLAN: FlistWalker Improvement Roadmap

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Parent Plan: none
- Child Plan(s): `docs/CHANGE-PLAN-20260408-improvement-app-coordinator-slice.md`
- Scope Label: improvement-roadmap
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: 初回レビューで child slice 不在と roadmap/slice 混在を指摘されたため active slice を追加。収束レビューの stale path 指摘も反映済み。

## 1. Background
- 2026-04-08 のレビューで、FlistWalker は機能品質とテスト投資の水準は高い一方、今後の継続開発に対して構造的な負債が残っていることを確認した。
- とくに `app/mod.rs`、`app/workers.rs`、`indexer.rs`、`search.rs`、`updater.rs` の肥大化、OS 統合層の実実装に対する検証不足、性能回帰ガードの通常 CI からの分離、既定運用での観測性不足、docs の責務境界の曖昧化が次の大きな改善対象である。
- 本計画は、機能追加に先行して「保守しやすい構造」と「壊れにくい品質ゲート」を整えるための上位 roadmap を定義する。

## 2. Goal
- 以後の大きな改善作業を、場当たり的な修正ではなく、依存順と完了条件が明確な workstream と slice に分解して進められる状態にする。
- 理想的な到達形を先に固定し、後続の slice plan が同じ north star を参照できるようにする。
- 次の観測可能な成果を目標にする。
  - coordinator / worker / domain の責務境界が明確である。
  - OS 依存挙動と性能回帰が CI で継続検証される。
  - 調査用の観測性が改善される。
  - docs が規範・運用・履歴で整理される。

## 3. Scope
### In Scope
- 次期改善 workstream の定義
- workstream 間の依存関係、推奨順序、着手条件、完了条件の定義
- 各 workstream で後続 slice plan に落とし込むべき論点の整理
- roadmap と active slice を分離するための運用方針の明文化

### Out of Scope
- 実装着手
- 新機能追加、UI デザイン刷新、配布インストーラ整備

## 4. Constraints and Assumptions
- 既存の SDD/TDD 文書体系 (`REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, `TESTPLAN.md`) は維持する。
- UI 応答性ポリシーと既存の検索契約 (`'`, `!`, `^`, `$`) は後方互換前提で扱う。
- 本計画は 2 段構えとし、active slice として `docs/CHANGE-PLAN-20260408-improvement-app-coordinator-slice.md` を持つ。
- active slice の実装順と詳細検証は child slice plan 側で管理し、本書は slice 間の依存順と着手条件に集中する。
- `AGENTS.md` の Temporary Change Plan Rule は roadmap と active slice の両方を参照する前提で追加する。

## 5. Current Risks
- Risk:
  - workstream の粒度が粗すぎて、後続の slice 分解で再設計が必要になる。
  - Impact:
    - 計画の再編集コストが増える。
  - Mitigation:
    - 本書では理想形と依存順に集中し、実装手順の詳細は slice 側へ持たせる。
- Risk:
  - 既存 docs と roadmap の責務が再び重複する。
  - Impact:
    - 将来的に更新漏れや矛盾が生じる。
  - Mitigation:
    - 本書は上位計画に限定し、恒久仕様や詳細設計は既存 docs に寄せる。

## 6. Execution Strategy
1. Slice A: App Coordinator Compression
   - Purpose:
     - `FlistWalkerApp` の coordinator 圧縮を最初の active slice とし、以後の worker/domain hardening を進めやすい構造へ整える。
   - Boundary:
     - app coordinator と feature owner の責務整理に限定する。
   - Dependency / Ordering:
     - 最優先で着手する。後続 slice の前提になる。
   - Entry condition:
     - child slice plan がレビュー済みで、Temporary Change Plan Rule が roadmap と slice を参照していること。
   - Exit condition:
     - `app/mod.rs` の残責務整理方針がコード・docs・tests で確定すること。
2. Slice B: Worker and Domain Modularization
   - Purpose:
     - coordinator 圧縮後に、`workers.rs`、`indexer.rs`、`search.rs`、`updater.rs` の責務分離を進める。
   - Boundary:
     - domain / worker の境界整理に集中し、OS hardening は主目的にしない。
   - Dependency / Ordering:
     - Slice A の後。A の owner 境界が固まってから着手する。
   - Entry condition:
     - Slice A 完了で coordinator と domain の依存面が安定していること。
   - Exit condition:
     - 巨大ファイルの責務境界が module 単位で説明できること。
3. Slice C: OS Integration Hardening
   - Purpose:
     - open/execute/update helper の abstraction と contract test を整える。
   - Boundary:
     - OS 依存挙動と失敗系の検証強化を主対象とする。
   - Dependency / Ordering:
     - Slice B と並行候補はあるが、基本は B 後を推奨する。
   - Entry condition:
     - action / updater 周辺の責務境界が slice 単位で切れること。
   - Exit condition:
     - platform-specific contract test の導線が成立すること。
4. Slice D: Perf Gate Strengthening
   - Purpose:
     - PR CI で機能する軽量 perf gate を導入し、重い perf suite の役割を再整理する。
   - Boundary:
     - perf gate、budget、workflow 分担、validation matrix 更新を扱う。
   - Dependency / Ordering:
     - B または C で perf 対象境界が安定した後を推奨する。
   - Entry condition:
     - どの perf guard を PR へ入れるか判断できる粒度の責務分離が済んでいること。
   - Exit condition:
     - PR CI と別 workflow の役割が明文化されていること。
5. Slice E: Diagnostics and Supportability
   - Purpose:
     - tracing / debug hook / support 向け調査導線を整理する。
   - Boundary:
     - request_id、source、latency、candidate count などの観測性改善に集中する。
   - Dependency / Ordering:
     - A/B/C の後を推奨する。責務境界が固まってからイベント定義を固定する。
   - Entry condition:
     - 主要 request flow の owner が整理済みであること。
   - Exit condition:
     - 障害解析のためのログ採取方針を docs 化できること。
6. Slice F: Docs Restructuring
   - Purpose:
     - 規範文書、運用文書、履歴文書の責務を整理する。
   - Boundary:
     - docs の情報設計と参照導線整理を扱う。
   - Dependency / Ordering:
     - 基本は最後。A-E の結果を反映する closure slice として扱う。
   - Entry condition:
     - 構造と検証方針の変更点が出揃っていること。
   - Exit condition:
     - docs の責務分離が完了し、更新先が迷わないこと。

## 7. Detailed Task Breakdown
- [x] active slice を 1 つ選び、child slice plan を追加する
- [x] Slice A の phase 定義を executable batch に再構成し、roadmap へ結果を反映する
- [ ] Slice B の前提条件を確定する
- [ ] Slice B-C の並行可否を roadmap 上で再判定する
- [ ] Slice D の PR perf gate 候補と budget を roadmap 上で固定する
- [ ] Slice E-F の着手順を、A-D の結果を踏まえて再評価する

## 8. Validation Plan
- Automated tests:
  - roadmap 自体は docs-only のため、文書追加時点ではコードテストは必須ではない。
  - 実装着手時は各 slice plan の validation に従う。
- Manual checks:
  - roadmap と child slice plan の親子関係整合
  - roadmap と `TASKS.md` の参照整合
  - roadmap が slice-level detail を持ち込みすぎていないことの確認
- Performance or security checks:
  - 本 roadmap では定義のみ。実際の perf / security 確認は各 slice plan へ委譲する。
- Regression focus:
  - roadmap が単なる理想論ではなく、既存の UI 応答性・root guard・perf guard・release hygiene を維持する方向になっていること

## 9. Rollback Plan
- この roadmap 文書は単独で削除・置換できる。
- 実装を伴わないため、ロールバック対象は docs と参照更新だけである。
- 後続でより適切な上位計画へ置換する場合は、本書を更新または削除し、`docs/TASKS.md` の参照先だけ同期する。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `improvement-roadmap`, read `[docs/CHANGE-PLAN-20260408-improvement-roadmap.md]` and `[docs/CHANGE-PLAN-20260408-improvement-app-coordinator-slice.md]` before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the roadmap or slice plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Planned roadmap document created from review findings.
- 2026-04-08 00:00 Existing ad-hoc `docs/ROADMAP.md` replaced with skill-aligned change plan roadmap.
- 2026-04-08 00:00 Initial review requested child slice creation; roadmap revised to reference Slice A.
- 2026-04-08 00:00 Convergence review completed after child slice addition and validation path correction.
- 2026-04-08 00:00 Slice A implementation started with coordinator helper extraction and docs sync.
- 2026-04-08 00:00 Slice A phase structure reconstructed to use two executable phases instead of inventory-only steps.
- 2026-04-08 00:00 Slice A completed and is ready to serve as the dependency base for Slice B.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- 本書は上位 roadmap であり、active slice は `docs/CHANGE-PLAN-20260408-improvement-app-coordinator-slice.md` とする。
- 後続 slice の着手前には、本書を更新して依存順と前提条件を最新化する。
- 恒久仕様へ移すべき内容が発生した場合だけ、`REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, `TESTPLAN.md` へ反映する。
