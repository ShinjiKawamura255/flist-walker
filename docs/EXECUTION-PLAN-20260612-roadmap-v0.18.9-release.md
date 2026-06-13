# EXECUTION PLAN: v0.18.9 Release Roadmap

## Metadata
- Date: 2026-06-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Profile: safety-critical
- Planning Depth: roadmap+slice
- Review Pattern: specialist-subagents
- Review Requiredness: required-before-and-after-revision-and-final
- Execution Mode: autonomous
- Execution Mode Policy: Review済み計画を起点に、preflight、tag/draft/publish、closureを順に自律実行する。main agentが計画、外部公開、commit、rollback、closureを所有し、subagentはrelease integrity、testing、security、operabilityのレビューだけを担当する。
- Parent Plan: none
- Child Plan(s):
  - docs/EXECUTION-PLAN-20260612-slice-a-v0.18.9-preflight.md
  - docs/EXECUTION-PLAN-20260612-slice-b-v0.18.9-publish.md
  - docs/EXECUTION-PLAN-20260612-slice-c-v0.18.9-closure.md
- Work Item Manifest: docs/EXECUTION-WORK-ITEMS-20260612-v0.18.9-release.json
- AGENTS.md Initial State: existing
- Temporary AGENTS.md Ownership: section-only
- Docs Directory Initial State: existing
- Temporary Docs Directory Ownership: existing-directory
- Scope Label: v0.18.9 release
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Viewpoints: purpose-scope, ordering, validation, rollback, testing, security, operability
- Review Notes:
  - 初回reviewで署名検証、asset完全性、publish前review、GUI証跡、PowerShell実build、恒久記録が不足と判定された。
  - workflow hardening、独立gate、GUI恒久証跡、tag/asset不変性、incident runbookを追加した。
  - testing/release-integrity と security/operability の最終収束reviewで blocking findingなし、実装開始可能と判定された。

## 1. Background
- `v0.18.8` 以降に保存済みroot編集改善、Windows PowerShell GNU build復旧、クロスプラットフォームCI修正が入った。
- release/tag/publishは署名付き配布物と外部公開を伴うため、preflight完了前にtagを作らない。

## 2. Goal
- `v0.18.9` のversion、CHANGELOG、tag、release本文、配布assetを一致させる。
- release workflowで秘密鍵を署名stepだけに限定する。公開鍵secretの形式、秘密鍵から導出した公開鍵、配布クライアントが使用する公開鍵の一致と署名往復検証、期待24 assetの完全一致、22 checksum entryの自己検証、archive内容検証を行う。
- tagged workflowの全jobと全assetを確認し、macOS未notarizedを明記したreleaseを公開する。
- 最後にclosure sliceでgoal達成を確認し、一時計画を削除する。

## 3. Scope
### In Scope
- `rust/Cargo.toml` / `rust/Cargo.lock` の `0.18.9` 更新。
- `CHANGELOG.md` とGitHub Release本文の作成。
- `.github/workflows/release-tagged.yml` とrelease bundle validationのhardening。既存release/assetを検出した場合は停止し、`--clobber` による上書きを禁止する。
- 公開後incident用の恒久runbook作成。
- VM-002 / VM-005、release preflight、tagged workflow、asset、checksum/signature確認。
- tag `v0.18.9`、draft release、publish。

### Out of Scope
- 新機能追加、依存更新、asset命名変更、notarization導入。

## 4. Constraints and Assumptions
- 最新release tagは `v0.18.8`、対象rangeは `v0.18.8..HEAD`。
- GitHub secrets `FLISTWALKER_UPDATE_PUBLIC_KEY_HEX` と `FLISTWALKER_UPDATE_SIGNING_KEY_HEX` は存在する。
- macOS notarizationは暫定的にpublish前提条件ではないが、release本文への明記が必須。
- tag/publish前にpreflightを完了する。

## 5. Current Risks
- Version/tag/changelog不一致:
  - Impact: 誤った配布物と更新判定。
  - Mitigation: tag前に4点照合する。
- Tagged workflowまたは署名失敗:
  - Impact: draft/asset未完成。
  - Mitigation: publishせず、tagged runとdraftを修復する。
- 不完全なasset公開:
  - Impact: OS別配布欠落、自己更新失敗。
  - Mitigation: expected asset一覧とSHA256SUMS/SHA256SUMS.sigを照合する。
- 公開後のrollback:
  - Impact: 利用者が不完全版を取得可能。
  - Mitigation: draft段階で検証し、publish後は公開停止、本文警告、更新経路確認、影響調査、patch release、利用者告知の順でcontainmentする。既存tag/assetは書き換えない。

## 6. Execution Strategy
1. Slice A: release preparation and preflight
   - Workflow hardening、恒久incident runbook、Version、CHANGELOG、release note draft、GUI evidence、PowerShell native build、docs/asset/OSS/ID ordering、VM-002/005を検証する。
2. Slice B: tag, draft, asset validation, publish
   - 準備commitをpushし、tagをpush、workflowとdraft assetを再downloadして検証し、本文を設定し、pre-publish specialist review後にpublishする。
3. Slice C: closure
   - 公開状態、tag、assets、working tree、final reviewを確認し、`docs/releases/v0.18.9.md` に恒久証跡を残してから一時成果物を削除する。

## 7. Detailed Task Breakdown
- [ ] Specialist plan review and convergence review
- [ ] Add temporary AGENTS.md rule after convergence review
- [ ] Complete Slice A
- [ ] Complete Slice B
- [ ] Complete Slice C and final review

- Manifest path: `docs/EXECUTION-WORK-ITEMS-20260612-v0.18.9-release.json`
- Ready item selection rule: dependencyがcompleteでactive sliceに属するitemのみ処理する。
- Completion evidence rule: file diff、command result、GitHub run/release URLをresultへ記録する。
- Repair item rule: failureは最小のpreflight/workflow/asset itemへ限定して修復する。

## 8. Validation Plan
- Automated: `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, `cargo audit`, PowerShell parser/regression/native build/artifact checks, Windows headful smoke, tagged GitHub Actions、release bundle validation。
- Manual: Windows `GSM-006` / `GSM-008`、CHANGELOG commit coverage、asset名、sidecar、checksum/signature、release本文、macOS notarization記載。
- macOS GUI: interactive macOS環境がないためSKIPPED理由と、macOS native CI test/release buildを代替証跡として恒久記録へ残す。
- Regression focus: root list duplicate edit、Windows GNU resource/icon build、cross-platform tests。

## 9. Rollback Plan
- Tag前: release preparation commitをrevert可能。
- Tag後draft前: remote tagを削除せず、workflow失敗を修正してpatch commit/tag方針を再評価する。
- Draft段階: publishしない。asset/body/workflowを修復する。tagged commitに修正が必要なら既存tagを書き換えずpatch versionを選定する。
- Publish後: releaseをdraftへ戻す判断またはpatch releaseを作る。既存tagの書換えはしない。

## 10. Temporary `AGENTS.md` Rule Draft
- 本roadmapとslice plans、manifestを上位から読み、ready itemのみ処理する。
- preflight完了前にtagを作成しない。
- publish前にtagged workflow、全asset、checksum/signature、release本文を確認する。
- publish前specialist reviewを完了する。
- final specialist review完了前にcleanupしない。

## 11. Progress Log
- 2026-06-12: Planned.
- 2026-06-12: Specialist convergence reviews completed; implementation approved.

## 12. Communication Plan
- 計画レビュー完了、blocking issue、公開完了時に報告する。

## 13. Closure Gate
- [ ] Required subagent final review completed
- [ ] Findings addressed or accepted with reasons
- [ ] Revalidation completed
- [ ] Durable release records completed
- [ ] Temporary rule and plans can be deleted

## 14. Completion Checklist
- [ ] Plan reviewed
- [ ] Temporary rule added
- [ ] Version and changelog updated
- [ ] Preflight passed
- [ ] Tag and draft created
- [ ] Assets and release body verified
- [ ] Release published
- [ ] Final review completed
- [ ] Temporary files removed
