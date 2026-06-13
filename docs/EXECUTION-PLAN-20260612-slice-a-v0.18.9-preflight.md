# EXECUTION PLAN: v0.18.9 Preflight

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
- Scope Label: v0.18.9 preflight
- Review Status: レビュー済み
- Review Viewpoints: ordering, validation, testing, security, operability

## Goal
- `v0.18.9` release metadataを作成し、tag作成可能なpreflight green状態にする。

## Scope
- Version、CHANGELOG、release本文draft。
- Tagged workflow hardening。
- 公開後incident用の恒久runbook。
- VM-002 / VM-005、ID ordering、禁止env、asset/OSS整合、GUI evidence、CI warning確認。

## Execution Strategy
1. Tagged workflowへsecret scope、公開鍵secret形式、秘密鍵導出公開鍵・配布クライアント公開鍵の一致、署名往復検証、asset完全一致、checksum/archive検証を追加する。既存release/asset検出時は停止し、`--clobber` を使用しない。
2. `docs/RELEASE_INCIDENT_RUNBOOK.md` に重大度判断、新規取得停止、GitHub Release警告、update feed停止・影響確認、担当、証跡保存先、patch判断、利用者通知、終結基準を実行可能な手順として記載する。
3. `v0.18.8..HEAD` の全commitをAdded/Changed/Fixed/Internalへ分類する。
4. Cargo versionとCHANGELOGを更新する。
5. Windows `GSM-006` / `GSM-008` evidenceを作成し、macOS GUI SKIPPED理由と代替CI証跡を記録する。
6. project-local preflight必須grepとvalidationを実行する。
7. release preparation commitを作成し、通常CIをgreenにする。

## Validation
- `cargo test --locked`
- `cargo clippy --all-targets -- -D warnings`
- `cargo audit`
- changed PowerShell scripts parser check
- `scripts/test-build-rust-win.ps1`
- `scripts/build-rust-win.ps1 -CheckOnly -NoInstall`
- `scripts/build-rust-win.ps1 -NoInstall`
- `scripts/test-windows-build-artifact.ps1`
- 両EXE hash、`.rsrc`、manifest、GUI subsystem、DLL依存
- required grep、ID ordering、asset/sidecar、OSS review
- Windows `GSM-006` / `GSM-008` の一覧選択、編集時focus/select-all、削除Cancel/Confirm、Apply、OKを確認し、各項目のPASS/FAIL/SKIPPEDと証跡パスを含むdated report
- 公開判断に必要なGUI report、screenshot、logを `docs/releases/evidence/v0.18.9/` へコピー
- macOS GUI SKIPPED理由、macOS CI native test/build代替証跡
- 公開鍵secretが非空の64桁hexであり、秘密鍵から導出した公開鍵および配布クライアントが使用する公開鍵と一致し、生成した `SHA256SUMS.sig` を検証できること
- 期待24 assetと22 checksum entryの完全一致、全archive内の `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` 双方、standalone sidecarの双方
- tag用releaseが既に存在する場合はworkflowが停止し、既存assetを上書きせず、release uploadに `--clobber` を使用しないこと
- `docs/RELEASE_INCIDENT_RUNBOOK.md` の手順が具体的な判断基準、操作、担当、証跡保存先を持つこと

## Rollback
- tag前なのでrelease preparation commitをrevertできる。

## Completion
- [ ] Metadata一致
- [ ] Workflow hardening validation
- [ ] Durable incident runbook reviewed
- [ ] GUI evidence recorded
- [ ] PowerShell native build and artifact validation
- [ ] Required validations green
- [ ] Release preparation commit pushed
- [ ] CI green
