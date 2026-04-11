# CHANGE PLAN: Slice D Release Platform Docs Consolidation

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice
- Parent Plan: `docs/CHANGE-PLAN-20260411-roadmap-regression-release-followup.md`
- Child Plan(s): none
- Scope Label: release-platform-docs-consolidation
- Related Tickets/Issues: none
- Inherited Execution Mode: standard
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-11 初版: release/platform/docs の恒久ルール整理を docs/workflow 差分中心に phase 分割した。
  - 2026-04-11 実現性レビュー: feasible。workflow を触る可能性があるなら YAML 妥当性と release 実行系確認の検証を別 phase に分ける。
  - 2026-04-11 粒度レビュー: docs-only phase と workflow-touching phase の境界を厳密化した方がよい。roadmap 側の順序自体は妥当。
  - 2026-04-11 収束レビュー: 着手可能。Slice D の phase 境界と検証は概ね整っており、重大な未解決事項はない。

## 1. Background
- release/tag 運用、macOS notarization、GitHub Release 本文、sidecar asset の扱いは `AGENTS.md`、`docs/RELEASE.md`、`.github/release-template.md`、`docs/SPEC.md`、`docs/TESTPLAN.md` に分散している。
- 現時点で、macOS notarization を publish 前提条件にするかどうか、draft release 後の確認順、manual test と workflow の役割分担に表現ゆれが残っている。
- これらはコード実装というより運用契約の不整合であり、release 判断の属人化を防ぐには恒久文書の同期が必要である。

## 2. Goal
- release/platform/docs の恒久ルールを同期し、notarization・draft release・asset 同梱・release note の扱いを一貫させる。
- `AGENTS.md`、`docs/RELEASE.md`、`.github/release-template.md`、必要な workflow / TESTPLAN / SPEC の整合を取る。
- user-facing 公開文書へ出してはいけない開発専用 override と、公開すべき release 運用情報の境界を明確に保つ。

## 3. Scope
### In Scope
- `AGENTS.md`
- `docs/{RELEASE,SPEC,TESTPLAN}.md`
- `.github/release-template.md`
- GitHub Release 本文の運用前提（template / docs で表現される範囲）
- 必要時 `.github/workflows/release-tagged.yml`

### Out of Scope
- notarization 基盤の新規構築
- release asset 名や updater 実装の仕様変更
- 個別 version の release note 本文更新

## 4. Constraints and Assumptions
- 開発専用 override (`FLISTWALKER_UPDATE_FEED_URL` など) を公開向け文書へ出さない方針は維持する。
- 既存 workflow の build/publish 実装は原則変えず、まず docs 契約を同期する。
- docs-only で閉じるなら validation は VM-001 を使う。workflow 変更が入る場合だけ該当 validation を追加する。

## 5. Execution Strategy
1. Phase 1: release/platform policy の不整合を棚卸しする
   - Files/modules/components:
     - `AGENTS.md`
     - `docs/{RELEASE,SPEC,TESTPLAN}.md`
     - `.github/release-template.md`
     - `.github/workflows/release-tagged.yml`
   - Expected result:
     - notarization、draft release、sidecar asset、manual test の表現差が列挙され、同期対象が確定する
   - Verification:
     - doc diff review
2. Phase 2: docs/template/AGENTS の恒久ルールを同期する
   - Files/modules/components:
     - `AGENTS.md`
     - `docs/{RELEASE,SPEC,TESTPLAN}.md`
     - `.github/release-template.md`
   - Expected result:
     - docs/template/AGENTS の運用契約が一貫し、publish 判断の前提が揃う
   - Verification:
     - doc diff review + `rg` 参照整合
3. Phase 3: workflow / validation 契約が必要なら追加同期する
   - Files/modules/components:
     - `.github/workflows/release-tagged.yml`
     - `docs/TESTPLAN.md`
     - 必要時 `docs/RELEASE.md`
   - Expected result:
     - workflow を変更する場合だけ、その validation と docs 契約が同時に更新される
   - Verification:
     - workflow を触らない場合は `not needed` を記録する
     - workflow を触る場合は YAML diff review と必要な validation を追加する
4. Phase 4: roadmap / cleanup gate を同期する
   - Files/modules/components:
     - `docs/CHANGE-PLAN-20260411-roadmap-regression-release-followup.md`
     - `AGENTS.md`
   - Expected result:
     - Slice D 完了結果を roadmap へ戻し、follow-up roadmap を閉じるか追加 slice を定義するか判断できる
   - Verification:
     - doc diff review

## 6. Phase Execution Policy
- active phase は上から順に 1 つずつ完了させる。順序を変える場合は先に本計画を更新する。
- 各 phase 完了時は、完了条件、検証結果、残課題を本計画へ記録し、git 管理下では phase 単位コミットを作成する。

## 7. Detailed Task Breakdown
- [ ] release/platform policy の不整合を棚卸しする
- [ ] 恒久ルールを docs/template/AGENTS へ同期する
- [ ] roadmap / cleanup gate を同期する

## 8. Validation Plan
- Automated tests:
  - docs-only なら不要
  - workflow 変更が入る場合は `cd rust && cargo test`
- Manual checks:
  - release 運用の公開文書に開発専用 override が混入していないことを確認する
- Additional checks:
  - workflow を触る場合は YAML diff review と tag/release 実行系への影響確認を記録する
- Regression focus:
  - notarization 運用の記述不整合
  - draft release と publish 判定の混線
  - sidecar asset / checksum / release note の欠落

## 9. Rollback Plan
- docs/template の同期は docs-only commit に閉じる。
- workflow 変更が必要でも docs と混在させず、別 phase / 別 commit に分ける。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- `regression-release-followup` の対応では、実装前に以下の計画書を上から順に読むこと。
- `docs/CHANGE-PLAN-20260411-roadmap-regression-release-followup.md`
- `docs/CHANGE-PLAN-20260411-slice-d-release-platform-docs-consolidation.md`
- roadmap の `Execution Mode: standard` と `Execution Mode Policy` に従うこと。
- phase 実行は原則として subagent へ委譲し、main agent は orchestrator / reviewer として計画更新、レビュー反映、完了判定、コミットを担当すること。
- 実装順と確認順は計画書に従い、scope / order / risk を変える場合は先に計画書を更新すること。
- この一時ルールは計画対応の完了後に削除すること。
```

## 11. Progress Log
- 2026-04-11 00:00 Slice D 初版を作成した。
- 2026-04-11 00:00 初回レビューを反映し、docs-only phase と workflow-touching phase を分離した。GitHub Release 本文の運用前提は template / docs 経由で scope に含めることへ補正した。
- 2026-04-11 00:00 Phase 1 として、release/platform policy の不整合を棚卸しした。主な差分は、`docs/SPEC.md` の SP-012 が notarization を publish 前提条件として読める一方、`AGENTS.md` / `docs/RELEASE.md` / `.github/release-template.md` は未 notarized publish を暫定許容している点、`.github/release-template.md` の Downloads が `README.txt` sidecar を列挙していない点、GitHub Release 本文の `Security` / `Known issues` 記載前提が docs 間で分散している点。
- 2026-04-11 00:00 Phase 2 として、`docs/SPEC.md`、`.github/release-template.md`、`docs/TESTPLAN.md` を同期し、notarization の暫定運用、README sidecar asset、GitHub Release 本文の `Security` / `Known issues` 記載前提を一貫させた。

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Review completed and reflected
- [x] Temporary `AGENTS.md` rule added
- [x] release/platform policy の不整合を棚卸しする
- [x] 恒久ルールを docs/template/AGENTS へ同期する
- [ ] Each completed phase committed separately
- [ ] Verification completed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion
