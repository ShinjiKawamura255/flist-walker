# CHANGE PLAN: Slice C Tooling Config Hygiene

## Metadata
- Date: 2026-04-11
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: inherited from parent roadmap (`autonomous`)
- Execution Mode Policy: parent roadmap の `autonomous` policy に従う。main agent は audit 結果を基準に minimal fix を進め、phase ごとに検証と commit を閉じる。
- Parent Plan: docs/CHANGE-PLAN-20260411-roadmap-app-structure-followup.md
- Child Plan(s): none
- Scope Label: slice-c-tooling-config-hygiene
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-11 feasibility review で、`walkdir` は現在未使用で削除可能、CI は `cargo clippy` と coverage 可視化が未常設、環境変数群は user-facing / dev-only / build-secret の 3 分類へ整理可能と確認した。
  - 2026-04-11 slice review で、config file 導入までは広げず、まず dependency cleanup、CI 強化、環境変数分類の明文化を最小 steady-state とする方針に絞った。
  - 外部要因で subagent review は利用できなかったため、main agent が fallback review を実施した。この例外は usage limit による一時的制約に限定する。

## 1. Background
- `walkdir` は `Cargo.toml` に残っているが、現行実装は `jwalk` を使っている。
- CI workflow は `cargo test --locked` と `cargo audit` はあるが、`cargo clippy` と coverage 可視化が未常設である。
- 環境変数は user-facing 設定、dev/test override、build/release secret が混在しており、どこまで公開文書へ載せるかの境界が曖昧である。

## 2. Goal
- 未使用依存 `walkdir` を除去し、lockfile と docs を同期する。
- CI に `cargo clippy --all-targets -- -D warnings` と coverage 可視化を追加する。
- 環境変数を `user-facing`, `dev/test override`, `build/release secret` に分類し、公開文書へ出す範囲を明文化する。

## 3. Scope
### In Scope
- `rust/Cargo.toml` / `rust/Cargo.lock` の dependency hygiene
- `.github/workflows/*.yml` の CI lint/coverage 追加
- `README.md`, `docs/RELEASE.md`, `docs/OSS_COMPLIANCE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md` などの config classification 更新

### Out of Scope
- 新しい config file format の導入
- 自己更新仕様の変更
- release asset 形式の変更

## 4. Constraints and Assumptions
- public user-facing docs へは dev/test override を混ぜない。
- coverage はまず CI artifact/report の可視化を導入し、coverage 閾値 enforcement までは要求しない。
- dependency 変更を含むため、必要な OSS/compliance docs は同一変更で更新する。

## 5. Current Risks
- Risk:
  - coverage job の追加が CI 実行時間や依存取得で不安定になる。
  - Impact:
    - workflow ノイズ、保守コスト増加。
  - Mitigation:
    - Linux job のみに限定し、coverage は artifact/report 生成に留める。
- Risk:
  - env var 分類が曖昧なままだと公開 docs に dev-only override が漏れる。
  - Impact:
    - ユーザ向け導線の混乱、サポート負荷。
  - Mitigation:
    - 3 分類を docs に明記し、dev-only override は README から排除または限定説明に留める。

## 6. Execution Strategy
1. Phase 1: audit and classification baseline
   - Files/modules/components:
     - `rust/Cargo.toml`, workflows, README/docs
   - Expected result:
     - `walkdir` 実使用の有無、CI 差分、env var の 3 分類が固定される。
   - Verification:
     - diff review
2. Phase 2: dependency and config hygiene
   - Files/modules/components:
     - `rust/Cargo.toml`, `rust/Cargo.lock`, docs
   - Expected result:
     - 未使用依存が除去され、env var 分類と公開可否が docs に反映される。
   - Verification:
     - `cargo test`
     - OSS/compliance diff review
3. Phase 3: CI lint and coverage
   - Files/modules/components:
     - `.github/workflows/*.yml`
   - Expected result:
     - `cargo clippy` と coverage job が CI へ追加される。
   - Verification:
     - workflow diff review
4. Phase 4: closure docs sync
   - Files/modules/components:
     - roadmap/slice logs, `ARCHITECTURE.md`, `DESIGN.md`, `TESTPLAN.md`
   - Expected result:
     - Slice C の steady-state と validation が恒久 docs に同期する。
   - Verification:
     - doc diff review

## 7. Detailed Task Breakdown
- [x] audit 結果を固定する
- [x] dependency と env var 分類を同期する
- [x] CI に clippy / coverage を追加する
- [x] docs と plan を closure する

## 8. Validation Plan
- Automated tests:
  - `cargo test`
- Manual checks:
  - workflow diff review
- Performance or security checks:
  - `docs/OSS_COMPLIANCE.md` の relevant items を確認する。
- Regression focus:
  - dependency lockfile consistency
  - public docs への dev-only override 混入防止
  - CI job 追加による既存 workflow 役割の崩れ防止

## 9. Rollback Plan
- dependency cleanup、CI enhancement、docs closure を別 commit に分ける。
- coverage job が不安定なら workflow commit だけを独立して戻せるようにする。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-structure-followup`, read the relevant change plan document(s) before starting implementation.
- Read them from upper to lower order:
  - `[docs/CHANGE-PLAN-20260411-roadmap-app-structure-followup.md]`
  - `[docs/CHANGE-PLAN-20260411-slice-c-tooling-config-hygiene.md]`
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Because the roadmap says `Execution Mode: autonomous`, continue autonomously through slice creation, review, phase execution, roadmap updates, and phase commits until the roadmap is complete unless a blocking problem occurs.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-11 21:05 Planned Slice C draft.
- 2026-04-11 21:10 Feasibility review completed. config file 導入までは広げず、dependency cleanup / CI enhancement / env var classification の最小 steady-state で進めると判断した。
- 2026-04-11 21:40 Phase 1 completed. `walkdir` 未使用、CI の lint/coverage 不足、env var の 3 分類を audit で固定した。
- 2026-04-11 22:05 Phase 2 completed. `walkdir` を削除し、公開 docs では user-facing env var だけを前面に出す分類へ整理した。`cargo test` green。
- 2026-04-11 22:25 Phase 3 completed. CI に `cargo clippy --all-targets -- -D warnings` と coverage artifact job を追加し、ローカル `cargo clippy` / `cargo test` を green で通した。
- 2026-04-11 22:30 Phase 4 completed. Slice C の steady-state を docs/roadmap に反映し、closure-ready にした。

## 12. Communication Plan
- Return to user when:
  - all roadmap phases are complete
  - a phase cannot continue without resolving a blocking problem
- If the project is under git control, commit at the end of each completed phase.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Work executed according to the plan or the plan updated first
- [x] If the project is under git control, each completed phase was committed separately
- [x] Verification completed
- [x] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion
