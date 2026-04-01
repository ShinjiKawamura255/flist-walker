# CHANGE PLAN: Review Follow-up Hardening

## Metadata
- Date: 2026-04-01
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: review-followups
- Related Tickets/Issues: review feedback on release safety, Windows validation, CLI contract, perf guard, maintainability

## 1. Background
- レビューで、即時の機能破綻は少ない一方、出荷前の検証保証、Windows 固有分岐の継続検証、CLI 契約の明確性、性能回帰の自動検知、GUI コアの保守性に弱点があることを確認した。
- 現状の `docs/TASKS.md` には改善項目の一覧はあるが、`plan-driven-changes` が要求する「変更順序、リスク、ロールバック、一時運用ルール」を持つ専用 change plan にはなっていない。
- このまま個別に着手すると、release workflow、CI、docs、実装の同期順が崩れやすく、途中で判断が変わった場合の履歴も残りにくい。

## 2. Goal
- レビュー指摘を、実施順と検証方針が固定された大きな改善計画として管理できる状態にする。
- 以後の対応で、release/tag 導線、Windows CI、CLI 契約、perf 回帰検知、`FlistWalkerApp` 再分割を、依存順に沿って進められるようにする。
- 完了時には、恒久ルールは `REQUIREMENTS.md` / `SPEC.md` / `DESIGN.md` / `TESTPLAN.md` / `README.md` / `docs/RELEASE.md` へ反映し、この change plan と一時 AGENTS ルールは削除する。

## 3. Scope
### In Scope
- release/tag workflow と通常 CI の依存関係見直し
- Windows native CI 導入と Windows 固有経路の継続検証
- CLI `--limit` 契約の実装 / docs / テスト整合
- perf regression の自動検知方針整理
- `FlistWalkerApp` の責務再分割計画と段階実装
- 上記に伴う docs / validation / release 運用文書の同期

### Out of Scope
- 新機能追加
- 旧 prototype への機能移植
- ネットワークドライブ最適化
- 配布インストーラ作成
- リリースノート本文の更新そのもの

## 4. Constraints and Assumptions
- 既存の AGENTS 指示に従い、仕様変更時は docs を同一変更で更新する必要がある。
- `rust/src/indexer.rs`、`rust/src/app/workers.rs`、`rust/src/app.rs` の indexing 経路変更時は ignored perf テスト実行が必要になる。
- Windows を重視するプロジェクトだが、現在は WSL/Linux 中心の開発環境であるため、Windows native CI 追加は runner 時間と flaky 管理の現実性を考慮して進める。
- tag release workflow は既に利用中のため、破壊的な切り替えではなく、段階的に gate を強める方針を前提にする。
- `docs/TASKS.md` は進捗一覧として維持してよいが、今回の作業順は本 change plan を正とする。

## 5. Current Risks
- Risk:
  - release workflow が通常 CI 成功を必須条件としていない。
  - Impact:
    - green 未確認の tag から draft release が作成されうる。
  - Mitigation:
    - Phase 1 で workflow 関係を再設計し、release の前提条件を明文化する。
- Risk:
  - Windows 固有分岐が非 Windows CI では検知されない。
  - Impact:
    - `.ps1`、path 正規化、update helper の回帰が release 直前まで見えない。
  - Mitigation:
    - Windows native runner を導入し、最低限の `cargo test --locked` を継続実行する。
- Risk:
  - CLI `--limit` の実効値と docs がずれる。
  - Impact:
    - 自動化利用者が結果件数を誤認する。
  - Mitigation:
    - failing test 先行で期待契約を固定し、その後に docs と実装を一致させる。
- Risk:
  - perf 目標が docs にありながら gate が弱い。
  - Impact:
    - 速度劣化が蓄積しても通常 CI では止まらない。
  - Mitigation:
    - gate か定点観測かを決め、`TESTPLAN.md` と CI の実態を一致させる。
- Risk:
  - `FlistWalkerApp` の責務集中が続く。
  - Impact:
    - 変更時の影響範囲が広く、バグ修正や機能追加のコストが高止まりする。
  - Mitigation:
    - 先に出荷導線を安定化させ、その後に state/coordinator/workflow 境界で再分割する。

## 6. Execution Strategy
1. Phase 1: Release and CI hardening
   - Files/modules/components:
     - `.github/workflows/ci-cross-platform.yml`
     - `.github/workflows/release-tagged.yml`
     - `docs/RELEASE.md`
     - `docs/TESTPLAN.md`
     - 必要に応じて `AGENTS.md`
   - Expected result:
     - release/tag 導線が必要な test/audit 成功を前提条件として持ち、Windows native CI の導入方針も固まる。
   - Verification:
     - workflow lint / dry-run 相当確認
     - 変更後の `cargo test --locked`
     - docs 整合確認
2. Phase 2: Contract and perf guard alignment
   - Files/modules/components:
     - `rust/src/main.rs`
     - `rust/tests/cli_contract.rs`
     - `docs/SPEC.md`
     - `README.md`
     - `docs/TESTPLAN.md`
   - Expected result:
     - CLI `--limit` 契約が明確化され、perf guard の実行位置づけと失敗条件が明文化される。
   - Verification:
     - CLI integration test
     - `cargo test --locked`
     - perf 関連ルールの docs 差分確認
3. Phase 3: App maintainability follow-up
   - Files/modules/components:
     - `rust/src/app.rs`
     - `rust/src/app/*.rs`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/TASKS.md`
   - Expected result:
     - `FlistWalkerApp` の責務がさらに局所化され、以後の改修単位が小さくなる。
   - Verification:
     - `cargo test --locked`
     - 必要に応じて ignored perf テスト
     - GUI 手動確認項目の更新

## 7. Detailed Task Breakdown
- [x] R-001 release/tag workflow の gate 設計を確定する
- [x] R-002 Windows native CI の対象テストと runner 構成を確定する
- [x] R-003 CLI `--limit` 契約を failing test で固定する
- [x] R-004 perf regression の gate/観測方針を確定する
- [ ] R-005 `FlistWalkerApp` の追加分割単位を棚卸しする
- [ ] R-006 恒久 docs を新運用へ同期する

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test --locked`
  - indexing 経路変更時は AGENTS 指示の ignored perf テストを追加実行
  - workflow 変更時は YAML 差分レビューと job graph の確認
- Manual checks:
  - release/tag 導線に関する docs review
  - CLI help / README / SPEC の記述一致確認
  - App 分割時は `docs/TESTPLAN.md` VM-002 / VM-003 に沿った GUI 確認
- Performance or security checks:
  - perf guard の gate 条件または定点観測先を明文化
  - Windows `.ps1` / path / self-update helper の検証導線が CI で見えること
- Regression focus:
  - tag push だけで release が先行しないこと
  - Windows 固有分岐の未検知が残らないこと
  - CLI `--limit` の実効値が docs とずれないこと

## 9. Rollback Plan
- workflow 変更は `.github/workflows/*` と関連 docs をまとめて revert できるよう、小さな単位で適用する。
- CLI 契約変更は test/docs/実装を同一コミットで扱い、必要ならその単位で戻す。
- `FlistWalkerApp` 再分割は phase ごとに独立 revert 可能な単位で進める。
- release/tag 名や asset 命名を変える場合は `docs/RELEASE.md` と `.github/release-template.md` を同時に戻す必要がある。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `review-followups`, read [docs/CHANGE-PLAN-20260401-review-followups.md] before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-01 20:39 Planned from review findings and converted to plan-driven workflow.
- 2026-04-01 20:41 Temporary `AGENTS.md` rule added. Future implementation for this scope must follow this plan.
- 2026-04-01 20:52 Phase 1 completed. Added release preflight gates, added Windows native CI coverage, validated workflow YAML parsing, and ran `cargo test --locked`.
- 2026-04-01 21:03 Phase 2 completed. Removed the hidden CLI 1000-item cap, added a perf regression workflow, ran `cargo test --locked`, and executed both ignored perf regression tests.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- `docs/TASKS.md` の `R-001` から `R-006` はこの change plan の作業項目として扱う。
- 本件の実装途中で順番を変える場合は、先にこの文書の Execution Strategy / Detailed Task Breakdown / Progress Log を更新する。
