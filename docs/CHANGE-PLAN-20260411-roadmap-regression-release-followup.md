# CHANGE PLAN: Regression and Release Follow-up Roadmap

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Parent Plan: none
- Child Plan(s): `docs/CHANGE-PLAN-20260411-slice-c-gui-regression-automation-expansion.md`, `docs/CHANGE-PLAN-20260411-slice-d-release-platform-docs-consolidation.md`
- Scope Label: regression-release-followup
- Related Tickets/Issues: none
- Execution Mode: standard
- Execution Mode Policy:
  - review 済み roadmap を基準に active slice を 1 本ずつ進め、slice 完了ごとに次 slice を明示的に見直す。
  - phase 実行は原則として subagent へ委譲し、main agent は計画更新、レビュー反映、完了判定、コミット、slice 間の引き継ぎを担当する。
  - active slice の scope / order / risk を変える場合は、実装前に roadmap と対象 slice plan を先に更新する。
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-11 初版: follow-up backlog を plan-driven-changes 用の roadmap へ昇格した。Slice C を active slice 候補として詳細化する。
  - 2026-04-11 実現性レビュー: feasible。致命的 blocker はないが、Slice C 単体で manual smoke 依存を大幅削減するのではなく、自動化の足場づくりとして扱う方が現実的。
  - 2026-04-11 粒度レビュー: 2 段構成は許容。roadmap は Slice C を先に閉じるための standard-mode roadmap として使い、Slice D の詳細化は Slice C 完了後に再レビューする。
  - 2026-04-11 収束レビュー: 着手可能。重大な未解決事項なし。期待値は「自動化の足場づくり」に合わせる。
  - 2026-04-11 Slice D 初回レビュー: release/platform/docs consolidation は feasible。workflow-touching phase の検証を docs-only phase と分離し、active slice は Slice D review 完了後に着手する。

## 1. Background
- `ideal-architecture` roadmap では app coordinator 縮小と lifecycle contract hardening を完了した。
- 残件として、GUI regression automation 拡張と release/platform/docs consolidation が残っている。
- これらは architecture/lifecycle hardening と独立に review / rollback / verification 境界を持つため、follow-up roadmap として切り出す。

## 2. Goal
- GUI の主要回帰を event-driven test へ追加し、手動 smoke 依存を減らす。
- release/platform/docs の暫定運用を恒久ルールへ整理し、release 判断の属人性を減らす。

## 3. Scope
### In Scope
- GUI regression automation expansion
- release/platform/docs consolidation

### Out of Scope
- 新機能追加
- search / index / updater の契約変更そのもの
- notarization 基盤の新規構築

## 4. Execution Strategy
1. Slice C: GUI regression automation expansion
   - Purpose:
     - `Structural Refactoring GUI Smoke Test` のうち event-driven test へ落としやすい操作を unit/app test へ移し、自動化の足場を増やす
   - Entry condition:
     - follow-up roadmap と active slice plan が review 済みである
   - Exit condition:
     - manual smoke の代表操作が owner test module に固定され、manual-only 項目を縮減できる
   - Verification:
     - `cd rust && cargo test`
2. Slice D: release/platform/docs consolidation
   - Purpose:
     - release/platform/docs の暫定運用を恒久ルールとして整理する
   - Entry condition:
     - Slice C の結果を反映し、残る manual-heavy 領域と release policy の整理対象が確定している
   - Exit condition:
     - docs / workflow / AGENTS の整合がとれ、release 判断の属人性が下がる
   - Verification:
     - docs diff review
     - 必要時 `cd rust && cargo test`

## 5. Slice Ordering and Gates
- Active slice は `Slice D` とする。着手条件は Slice D plan がレビュー済みであること。
- `Slice C` 完了結果として、GUI smoke の自動化対象と manual-heavy 領域は整理できたため、次は release/platform/docs の恒久ルール整理へ進む。
- `Execution Mode: standard` のため、`Slice D` は詳細 plan の review 完了後に着手する。

## 6. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- `regression-release-followup` の対応では、実装前に以下の計画書を上から順に読むこと。
- `docs/CHANGE-PLAN-20260411-roadmap-regression-release-followup.md`
- active slice plan
- roadmap の `Execution Mode: standard` と `Execution Mode Policy` に従うこと。
- phase 実行は原則として subagent へ委譲し、main agent は orchestrator / reviewer として計画更新、レビュー反映、完了判定、コミットを担当すること。
- 実装順と確認順は計画書に従い、scope / order / risk を変える場合は先に計画書を更新すること。
- この一時ルールは計画対応の完了後に削除すること。
```

## 7. Progress Log
- 2026-04-11 00:00 follow-up backlog を 2 段の roadmap へ昇格し、Slice C を active slice 候補として選定した。
- 2026-04-11 00:00 初回レビューを反映し、Slice C の goal を「manual smoke の全面置換」ではなく「自動化の足場づくり」に補正した。Slice D の詳細化は Slice C 完了後の standard-mode gate で見直す。
- 2026-04-11 00:00 Slice C Phase 1 として、`TESTPLAN.md` に structural GUI smoke と owner test module の対応表を追加し、automation target と manual-only 領域を固定した。
- 2026-04-11 00:00 Slice C Phase 2 として、`render_tests.rs` と `session_tabs.rs` に root/tab/render interaction の regression test を追加し、`cargo test` green を確認した。
- 2026-04-11 00:00 Slice C Phase 3 として、`search_filelist.rs` に filelist dialog / background flow の regression test を追加し、`cargo test` green を確認した。
- 2026-04-11 00:00 Slice C 完了判定: `TESTPLAN.md` に新規 automated coverage を反映し、current HEAD で `cargo test` green を再確認した。次は standard-mode gate として Slice D の詳細 plan を作成する。
- 2026-04-11 00:00 Slice D 初版を作成し、release/platform/docs の不整合整理を次の active slice として詳細化した。
- 2026-04-11 00:00 Slice D Phase 1 として、notarization 運用、README sidecar 一覧、GitHub Release 本文の `Security` / `Known issues` 前提の不整合を棚卸しし、同期対象を確定した。

## 8. Completion Checklist
- [x] Planned document created before implementation
- [x] Review completed and reflected
- [x] Temporary `AGENTS.md` rule added
- [x] Active slice completed and committed phase by phase
- [x] Roadmap updated after slice completion
- [ ] Temporary `AGENTS.md` rule removed after completion
