# CHANGE PLAN: Regression and Release Follow-up Roadmap

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 1
- Plan Role: roadmap
- Parent Plan: none
- Child Plan(s): none
- Scope Label: regression-release-followup
- Related Tickets/Issues: none
- Execution Mode: standard
- Review Status: draft

## 1. Background
- `ideal-architecture` roadmap では app coordinator 縮小と lifecycle contract hardening を完了した。
- 残件として、GUI regression automation 拡張と release/platform/docs consolidation が残っている。
- これらは architecture/lifecycle hardening と独立に review / rollback / verification 境界を持つため、follow-up roadmap として切り出す。

## 2. Goal
- GUI の主要回帰を event-driven test へ追加し、手動 smoke 依存を減らす。
- release/platform/docs の暫定運用を恒久ルールへ整理し、release 判断の属人性を減らす。

## 3. Candidate Slices
1. Slice C: GUI regression automation expansion
2. Slice D: release/platform/docs consolidation

## 4. Notes
- 本 roadmap は follow-up backlog として作成し、active plan 化は別途 review 後に行う。
- `ideal-architecture` 完了後は Temporary Change Plan Rule をいったん撤去し、この roadmap を自動着手の対象にはしない。
