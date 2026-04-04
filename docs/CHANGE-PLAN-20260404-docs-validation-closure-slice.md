# CHANGE PLAN: Docs and Validation Closure Slice

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: docs-validation-closure
- Parent Roadmap: `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md`
- Related Workstream: Docs and Validation Closure

## 1. Background
- request routing ownership、render orchestration、final coordinator cleanup まで完了し、Rust 側の app architecture 改善は一区切り付いた。
- 一方で roadmap / active slice / `AGENTS.md` の temporary rule はまだ途中状態を指しており、`DESIGN.md` / `TESTPLAN.md` / `TASKS.md` にも phase-by-phase の補足が残っている。
- roadmap の最後の workstream は、この一時運用を撤去し、恒久 docs と validation guidance を最終構造にそろえる closure である。

## 2. Goal
- app architecture 改修の完了状態を roadmap / 恒久 docs / `AGENTS.md` に反映し、一時 plan 運用を安全に撤去する。
- `DESIGN.md` / `TESTPLAN.md` / `TASKS.md` を、現在の module 境界・validation 実態・完了済み workstream と一致させる。
- lower-level slice plan を完了後に削除できるよう、closure 専用の手順と phase gate を先に固定する。

## 3. Scope
### In Scope
- roadmap の完了反映と active slice の `Docs and Validation Closure` への切り替え
- `DESIGN.md` / `TESTPLAN.md` / `TASKS.md` の app architecture 最終同期
- `AGENTS.md` temporary change plan rule の撤去
- roadmap と active slice plan の削除、および docs-only validation の記録

### Out of Scope
- Rust 実装や test code の変更
- app architecture 以外の release / updater / OSS compliance / CLI 契約の再整理
- 新しい workstream や follow-up refactor の追加

## 4. Cleanup Targets
- roadmap:
  - 4 workstream をすべて DONE とし、closure 完了後は plan 自体を削除する。
- `DESIGN.md`:
  - request routing / render / final coordinator cleanup の補足を、恒久的な module boundary 説明へ畳む。
- `TESTPLAN.md`:
  - active slice 固有の phase rule 群は履歴として必要な最小限に絞り、final validation の記録を current guidance と一致させる。
- `TASKS.md`:
  - app architecture 改修については「完了済み workstream の記録」と読める形に整理し、複数の旧 active scope が並存する状態を解消する。
  - roadmap / slice plan を削除する前に、この closure の完了理由と実施日の記録を永続項目として残す。
- `AGENTS.md`:
  - temporary rule を撤去し、恒久ルールのみ残す。

## 5. Risks
- Risk: docs cleanup のつもりで roadmap / `TASKS.md` から必要な履歴まで消し、後から経緯を追えなくなる。
  - Impact: 中
  - Mitigation: 履歴をゼロにせず、完了済み workstream と closure 実施日の記録は残す。
- Risk: temporary rule や plan を早く消しすぎて、closure 実施中に参照先が失われる。
  - Impact: 中
  - Mitigation: plan 削除は最終 phase に限定し、それまでは roadmap と active slice plan の両方を維持する。
- Risk: docs-only slice なのに validation rule の更新漏れで、closure 後の運用が曖昧になる。
  - Impact: 中
  - Mitigation: `TESTPLAN.md` に docs-only closure の検証方針を明記し、doc diff review と `rg` 整合確認を phase gate に含める。

## 6. Execution Strategy
1. Phase 1: closure plan scaffolding
   - Files: `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md`, `docs/CHANGE-PLAN-20260404-docs-validation-closure-slice.md`, `docs/TESTPLAN.md`, `AGENTS.md`
   - Action: roadmap を closure workstream へ進め、active slice plan と validation ルールを作成する。
   - Verification: doc diff review; `rg` 参照整合確認
2. Phase 2: permanent docs sync
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`
   - Action: app architecture の final state に合わせて恒久 docs を整理し、active/in-progress 表現を解消する。`docs/TASKS.md` には closure 完了理由と実施日の永続記録を追加する。
   - Verification: doc diff review; `rg` 参照整合確認
3. Phase 3: temporary rule and plan removal
   - Files: `AGENTS.md`, `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md`, `docs/CHANGE-PLAN-20260404-docs-validation-closure-slice.md`
   - Action: Phase 2 の永続記録追加を確認したうえで、closure 完了後に temporary rule と plan docs を撤去する。
   - Verification: doc diff review; `rg` 参照整合確認
4. Phase 4: final closure verification
   - Files: `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, `AGENTS.md`
   - Action: app architecture 用の一時運用が repo から消えていること、恒久 docs が現状と一致することを確認する。
   - Verification: doc diff review; `rg -n "CHANGE-PLAN-20260404-(app-architecture-roadmap|docs-validation-closure-slice)|Temporary Change Plan Rule|next active slice|Active Scope" AGENTS.md docs`; `git diff --stat`

## 7. Exit Criteria
- app architecture roadmap の 4 workstream が完了済みとして整理されている。
- `AGENTS.md` に app architecture 用 temporary rule が残っていない。
- `DESIGN.md` / `TESTPLAN.md` / `TASKS.md` が current architecture を説明しており、旧 active scope や obsolete phase note が整理されている。
- `TASKS.md` に app architecture roadmap 完了と closure 実施日の永続記録が残っている。
- roadmap と active lower-level plan が repo から削除されている。

## 8. Review Notes
- 2026-04-04 initial review:
  - closure workstream は docs-only cleanup に限定し、Rust 実装変更や follow-up refactor を混ぜない。
  - `TASKS.md` は削除ではなく「完了済み workstream の記録」と読める形へ整理する前提を固定した。
- 2026-04-04 subagent review:
  - plan 削除前に `docs/TASKS.md` へ closure 完了理由と実施日の永続記録を残す step を mandatory gate として追加した。
- 2026-04-04 convergence review:
  - durable history → temporary rule / plan removal の順序が成立しており、docs-only validation も `TESTPLAN.md` と整合していることを確認した。
  - `docs/TASKS.md` に旧 `Active Scope` が残っている点は、この slice 自身の cleanup 対象であり blocking issue ではない。

## 9. Temporary Rule Draft
- For `docs-validation-closure`, read `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md` first, then `docs/CHANGE-PLAN-20260404-docs-validation-closure-slice.md`.
- Follow the roadmap for scope/order and this slice plan for closure steps.
- Remove the temporary rule and delete both plans after the covered work is complete.
