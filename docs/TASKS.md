# TASKS

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
