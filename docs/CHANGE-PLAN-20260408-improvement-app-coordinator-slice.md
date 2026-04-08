# CHANGE PLAN: App Coordinator Compression Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-improvement-roadmap.md`
- Child Plan(s): none
- Scope Label: improvement-app-coordinator
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: 2 段 roadmap の child slice として再構成し、phase を実行可能単位に合わせて整理した。

## 1. Background
- roadmap には複数の後続 workstream があり、最初の active slice は app coordinator の圧縮が妥当である。
- 以前の slice 定義は、棚卸し中心の phase を含んでいて、実装単位としては弱かった。
- この slice は、`app/mod.rs` の coordinator 責務を helper 抽出と boundary closure の 2 phase で整理し、後続の worker/domain hardening へ進める下地を作る。

## 2. Goal
- `app/mod.rs` を coordinator 中心に再編し、後続の worker/domain hardening を進めやすい app 境界を作る。
- 2 phase の完了条件を明確にする。
  - Phase 1 で root/action guard まわりの純粋ヘルパーを coordinator module に集約する。
  - Phase 2 で `app/mod.rs` を orchestration 中心に寄せ、docs と validation matrix を実装に一致させる。
- 最終的に `app/mod.rs` が frame lifecycle と feature owner の結線として読める状態に近づく。

## 3. Scope
### In Scope
- `rust/src/app/mod.rs` の coordinator 寄り責務の切り出し
- `rust/src/app/coordinator.rs` への純粋ヘルパー集約
- app architecture 変更に伴う targeted regression の更新
- `docs/ARCHITECTURE.md` / `docs/DESIGN.md` / `docs/TESTPLAN.md` / `docs/TASKS.md` の必要同期

### Out of Scope
- `workers.rs`、`indexer.rs`、`search.rs`、`updater.rs` の大規模分割
- OS integration hardening
- perf gate workflow の変更
- docs 情報設計の全面整理

## 4. Constraints and Assumptions
- UI 応答性ポリシーを最優先し、UI スレッドへの同期 I/O 持ち込みは禁止とする。
- request_id 契約と root 外 path ガードの挙動は維持する。
- 既存の tab/session/history/filelist/update のユーザ挙動を保ったまま構造整理する。
- app architecture 変更により `docs/TESTPLAN.md` の VM-001 / VM-002 / VM-003 適用対象が変わる場合は同一変更で更新する。
- phase はコード変更と検証が一度に完結する粒度で切り、棚卸しのみの phase を置かない。

## 5. Current Risks
- Risk:
  - 責務分割の途中で `app/mod.rs` から別 module へ move した処理が、active/background tab routing を壊す。
  - Impact:
    - 検索結果、preview、action 応答が誤 tab に適用される。
  - Mitigation:
    - request routing / active-background tab regression を slice 完了条件に含める。
- Risk:
  - kind resolution や status line を切り出す際に、既存の UI responsiveness guard が崩れる。
  - Impact:
    - frame loop が重くなる、または stale response handling が壊れる。
  - Mitigation:
    - kind resolution regression と incremental index/search regression を優先確認する。
- Risk:
  - owner 境界の整理だけで終わらず、domain/worker 分割へ作業が膨らむ。
  - Impact:
    - slice が肥大化し、roadmap の依存順が崩れる。
  - Mitigation:
    - `workers.rs` / `indexer.rs` / `search.rs` / `updater.rs` の本格分割は次 slice へ送る。

## 6. Execution Strategy
1. Phase 1: Extract Shared Coordinator Helpers
   - Files/modules/components:
     - `rust/src/app/mod.rs`
     - `rust/src/app/coordinator.rs`
   - Expected result:
     - status line、root guard、normalized compare key のような純粋ヘルパーが coordinator module に集約される。
   - Verification:
     - `cargo test`
     - coordinator module の unit test
2. Phase 2: Stabilize Coordinator Boundary and Sync Docs
   - Files/modules/components:
     - `rust/src/app/mod.rs`
     - `docs/ARCHITECTURE.md`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/TASKS.md`
   - Expected result:
     - `app/mod.rs` が orchestration 中心に読める状態になり、docs と validation matrix が実装に一致する。
   - Verification:
     - `cargo test`
     - docs diff review

## 7. Detailed Task Breakdown
- [x] coordinator helper を新 module に抽出する
- [x] `app/mod.rs` の呼び出し側を helper module に切り替える
- [x] architecture/design/validation docs を実装に同期する
- [x] targeted regression と unit test を通す

## 8. Validation Plan
- Automated tests:
  - `cargo test`
  - `rust/src/app/coordinator.rs` の unit test
- Manual checks:
  - `app/mod.rs` が coordinator として読めるかの code review
  - active/background tab routing の影響確認
- Performance or security checks:
  - `rust/src/app/workers.rs`、`rust/src/indexer.rs`、`rust/src/app/mod.rs` のインデクシング経路に波及した場合は ignored perf テストを追加実行する
- Regression focus:
  - request routing
  - stale response suppression
  - kind resolution freeze regression
  - root 外 path guard
  - status line / in-progress indicator の維持

## 9. Rollback Plan
- owner 切り出しは小さな単位で戻せるようにする。
- docs 更新は実装 rollback に合わせて同時に戻す。
- slice の途中で肥大化した場合は、変更を戻すのではなく slice plan を更新して次 slice へ分離する。

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `improvement-roadmap`, read `[docs/CHANGE-PLAN-20260408-improvement-roadmap.md]` and `[docs/CHANGE-PLAN-20260408-improvement-app-coordinator-slice.md]` before starting implementation.
- Execute the App Coordinator Compression work in the documented phase order unless the roadmap or slice plan is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Active slice created as child plan of the improvement roadmap.
- 2026-04-08 00:00 Coordinator helper extraction implemented and verified with `cargo test`.
- 2026-04-08 00:00 Slice phases reconstructed to use executable batches instead of inventory-only steps.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Work executed according to the plan or the plan updated first
- [x] Verification completed
- [x] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- この slice は coordinator 圧縮に限定し、worker/domain 本体分割へ膨らませない。
- slice で扱う phase は、実装と検証が完結するバッチだけにする。
- slice 完了時は roadmap 側へ、次 slice の前提条件と依存順の更新を必ず反映する。
