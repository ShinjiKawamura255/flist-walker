# EXECUTION PLAN: v0.18.9 Candidate Rejection / v0.18.10 Publish

## Metadata
- Date: 2026-06-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Profile: safety-critical
- Planning Depth: roadmap+slice
- Review Pattern: specialist-subagents
- Review Requiredness: required-before-and-after-revision-and-final
- Execution Mode: none
- Execution Mode Policy: inherit roadmap
- Parent Plan: docs/EXECUTION-PLAN-20260612-roadmap-v0.18.9-release.md
- Child Plan(s): none
- Work Item Manifest: docs/EXECUTION-WORK-ITEMS-20260612-v0.18.9-release.json
- AGENTS.md Initial State: existing
- Temporary AGENTS.md Ownership: section-only
- Docs Directory Initial State: existing
- Temporary Docs Directory Ownership: existing-directory
- Scope Label: v0.18.9 candidate rejection and v0.18.10 publish
- Review Status: レビュー済み
- Review Viewpoints: security, rollback, operability, release-integrity

## Goal
- Immutable tag `v0.18.9` のcandidateをwarning gate不合格として公開しない。
- warningを修正したimmutable tag `v0.18.10` から全assetを生成し、検証済みreleaseを公開する。

## Execution Strategy
1. `v0.18.9` pre-publish review findingを記録し、draftを公開しない。tagとassetは更新・上書きしない。
2. macOS条件コンパイル警告とfuture-incompatibility依存を修正し、OSS noticeを同期する。
3. 修正準備commitをpushし、macOSを含む通常CIの全jobとwarningゼロを確認する。
4. Remote `refs/tags/v0.18.10` が存在しないことを確認する。既存なら更新・削除せず停止する。Version/tag/changelog/HEADを再照合し、annotated tagを作成・pushする。準備commit SHAとlocal/remote peeled SHAの一致を確認し、tag object SHAとpeeled commit SHAを記録する。
5. `Release Tagged Build` を完了まで監視する。
6. Draft releaseから全assetを再downloadし、期待24 asset完全一致、22 checksum entry、`sha256sum -c`、署名公開鍵検証、全archive内の `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` 双方、standalone sidecarの双方、version、`.app` bundle非混入を確認する。
7. 固定formatでrelease本文を設定し、macOS未notarizedを明記する。
8. workflow log warningと本文policyを確認する。warningは原則publish blockerとし、例外時は本文、影響、承認者、承認理由をpre-publish reviewと恒久記録へ残す。
9. pre-publish specialist reviewで `docs/RELEASE_INCIDENT_RUNBOOK.md` の具体的な判断基準、GitHub Release/update feed操作、担当、証跡保存先も確認する。
10. Draftをpublishし、公開状態を確認する。

## Stop Conditions
- preflightまたは通常CIがgreenでない。
- tagged workflowにfailureまたは未承認warningがある。
- expected asset、checksum、signature、本文のいずれかが欠ける。
- 同一tagのdraft/public releaseまたは既存assetが存在する。
- pre-publish specialist reviewが未完了。

## Rollback
- Draftではpublishしない。
- Published後に重大問題が見つかった場合、`docs/RELEASE_INCIDENT_RUNBOOK.md` に従う。tag/assetを書き換えず、新規取得停止の判断と操作、release本文警告、update feed停止と影響確認、asset SHAと発生時刻の保存、patch release判断、利用者通知、終結基準の順でcontainmentを実行する。draftへ戻しても既取得物は回収できないため、被害抑制として扱う。

## Completion
- [x] v0.18.9 candidate rejected without publication or tag rewrite
- [ ] macOS release build warning-free
- [ ] v0.18.10 tagged workflow green
- [ ] Expected assets verified
- [ ] Checksum and signature verified
- [ ] Archive and sidecar contents verified
- [ ] Release body verified
- [ ] Tag object and peeled commit SHAs recorded
- [ ] Warning exceptions, if any, approved and recorded
- [ ] Post-publish containment runbook approved
- [ ] Pre-publish specialist review completed
- [ ] Release published
