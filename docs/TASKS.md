# TASKS

## Active Scope
- Goal: レビュー指摘のうち、リリース安全性・Windows 検証・CLI 契約・保守性・性能回帰検知の弱点を、段階的に是正する。
- Docs: `docs/TASKS.md`, `docs/REQUIREMENTS.md`, `docs/SPEC.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/RELEASE.md`
- Updated: 2026-04-01

## Active Task List
| ID | Status | Area | Summary | Dependencies | DoD | Next action |
| --- | --- | --- | --- | --- | --- | --- |
| R-001 | DONE | CI | release/tag workflow と通常 CI の関係を整理し、tag 側でも test/audit の成功を前提条件にする | - | tag push で draft release が作成される前に必要な検証 job が必ず通る構成になり、`docs/RELEASE.md` と整合する | Phase 2 の CLI/perf 契約整理へ進む |
| R-002 | DONE | Windows QA | Windows native runner を導入し、Windows 専用分岐（action / path / update helper）を CI で継続検証する | R-001 | Windows runner で少なくとも `cargo test --locked` 相当が回り、Windows 固有分岐が PR / release 前に検知可能になる | Phase 2 の CLI/perf 契約整理へ進む |
| R-003 | DONE | CLI contract | CLI の `--limit` 契約を見直し、1000 件上限を撤廃するか、明示的な仕様として docs/help/test へ反映する | - | 実装・README・SPEC・CLI テストの解釈が一致し、利用者が `--limit` の実効値を誤解しない | Phase 3 の app architecture 整理へ進む |
| R-004 | DONE | Perf guard | 10万件 / 100ms 目標と ignored perf テストの運用を見直し、自動回帰検知の仕組みを追加する | R-001 | perf の定点観測または gate が CI/workflow に追加され、`docs/TESTPLAN.md` の VM-003 / TC-007 と実態が一致する | Phase 3 の app architecture 整理へ進む |
| R-005 | DONE | App architecture | `FlistWalkerApp` の責務を再分割し、state/coordinator/workflow 境界を明確化する | R-001 | `app.rs` の責務が縮小し、変更時の影響範囲とテスト対象が局所化される。設計 docs も更新される | R-006 の docs/process 同期を完了する |
| R-006 | DONE | Docs/process | 上記改善後の release / validation / review 観点を docs に反映し、運用依存の暗黙知を減らす | R-001, R-002, R-003, R-004 | `docs/RELEASE.md`, `docs/TESTPLAN.md`, 必要な `README.md` / `AGENTS.md` が新運用と一致する | review follow-ups をクローズする |

## Priority / Phase
- Phase 1: `R-001`, `R-002`
  release 事故防止と Windows 検証不足を先に塞ぐ。出荷導線に直接効くため最優先。
- Phase 2: `R-003`, `R-004`
  利用者契約の不一致と性能回帰検知の弱さを是正する。機能の信頼性を上げるフェーズ。
- Phase 3: `R-005`, `R-006`
  中長期の保守性改善と docs 同期を行う。Phase 1/2 の結果を反映して設計を安定化する。

## Validation Notes
- `R-001`, `R-002`: workflow 変更後は対象 workflow の dry-run 相当確認に加え、Linux/macOS/Windows での `cargo test --locked` 実行可否を確認する。
- `R-003`: CLI 契約変更は integration test を先に更新し、`README.md` と `docs/SPEC.md` の記述を同一変更でそろえる。
- `R-004`: 既存 ignored perf テストの位置づけを見直し、gate にしない場合も計測結果の保存先と失敗条件を明文化する。
- `R-005`: `rust/src/app.rs`, `rust/src/app/*.rs` の変更は `docs/TESTPLAN.md` の VM-002/VM-003 に従って検証する。

## Active Progress
- 2026-04-01: Phase 1 完了。tag release workflow に preflight test/audit gate を追加し、通常 CI に Windows native runner を追加。`ruby -e "require 'yaml'; ..."` で workflow YAML を読込確認し、`cd rust && cargo test --locked` を実行済み。
- 2026-04-01: Phase 2 完了。CLI `--limit` の 1000 件暗黙上限を撤廃し、integration test と docs を更新。`.github/workflows/perf-regression.yml` を追加し、`cargo test --locked` と ignored perf テスト 2 本を実行済み。
- 2026-04-01: Phase 3 完了。`app/session.rs` に UI state/saved roots/window geometry 永続化を寄せ、`app/state.rs` に GUI 横断 state 型を集約して `app.rs` の責務を縮小。`docs/DESIGN.md` を同期し、一時 change plan と AGENTS 一時ルールを撤去。

## Scope
- Goal: `rust/src/app.rs` の段階的分割を、機能互換と既存テスト資産を維持したまま進める。
- Docs: `docs/TASKS.md`, `docs/REQUIREMENTS.md`, `docs/SPEC.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
- Updated: 2026-03-06

## Task list
| ID | Status | Area | Summary | Dependencies | DoD | Next action |
| --- | --- | --- | --- | --- | --- | --- |
| T-001 | DONE | Planning | `app.rs` 分割方針と Phase 順を確定する | - | 分割方針を記録した計画 docs が存在する | AGENTS 一時方針へ反映 |
| T-002 | DONE | Process | `AGENTS.md` に一時的な分割実行ポリシーを追加する | T-001 | `AGENTS.md` から参照順と更新ルールが読める | Phase 1 に着手 |
| T-003 | DONE | Tests | `app.rs` inline test を `rust/src/app/tests/` へ分離する | T-001, T-002 | inline test が外出しされ `cargo test` が通る | Phase 2 の worker 境界を棚卸し |
| T-004 | DONE | Workers | worker request/response 型と `spawn_*_worker` 群を `workers.rs` へ移す | T-003 | worker 群が別モジュール化され `cargo test` が通る | Phase 3 の session/tab 永続化境界を棚卸し |
| T-005 | DONE | Session | session/tab restore/persistence を `session.rs` または `tabs.rs` に分離する | T-004 | session 系責務が分離され `cargo test` が通る | Phase 4 の input 依存を棚卸し |
| T-006 | DONE | Input | shortcut/query history/IME を `input.rs` に分離する | T-005 | input 系責務が分離され `cargo test` が通る | Phase 5 の render 境界を棚卸し |
| T-007 | DONE | Render | `update()` 内の panel/dialog 描画を整理する | T-006 | `update()` が orchestration 中心になる | Cleanup と AGENTS 一時項目削除へ進む |
| T-008 | DONE | Cleanup | 一時 AGENTS 項目を削除し、計画完了を文書へ反映する | T-003, T-004, T-005, T-006, T-007 | `AGENTS.md` から一時項目が削除されている | 分割計画完了 |

## Blocked
- なし

## Done
- T-001: `app.rs` 分割方針を記録した計画 docs を作成し、段階的分割方針を確定。
- T-002: `AGENTS.md` に分割計画 docs / `docs/TASKS.md` / `docs/WORKLOG.md` を参照する一時運用ルールを追加。
- T-003: `app.rs` の inline test を `rust/src/app/tests/` 配下へ分離し、`cargo fmt` / `cargo test --manifest-path rust/Cargo.toml --locked` の green を確認。
- T-004: worker runtime、request/response 型、`spawn_*_worker` 群と専用 helper を `rust/src/app/workers.rs` へ移し、`cargo fmt` / `cargo test --manifest-path rust/Cargo.toml --locked` の green を確認。
- T-005: session 永続化データ型と `load/save/sanitize` helper を `rust/src/app/session.rs` へ移し、`cargo fmt` / `cargo test --manifest-path rust/Cargo.toml --locked` の green を確認。
- T-006: shortcut/query history/IME/deferred shortcut と文字列編集 helper を `rust/src/app/input.rs` へ移し、`cargo fmt` / `cargo test --manifest-path rust/Cargo.toml --locked` の green を確認。
- T-007: `render_results_and_preview` / `render_results_list` / `render_tab_bar` と `update()` 内の top/status/dialog/central panel 構築を `rust/src/app/render.rs` へ移し、`cargo fmt` / `cargo test --manifest-path rust/Cargo.toml --locked` の green を確認。
- T-008: `AGENTS.md` の一時運用項目を削除し、分割計画完了を `docs/TASKS.md` / `docs/WORKLOG.md` へ反映。
