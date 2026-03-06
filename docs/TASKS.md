# TASKS

## Scope
- Goal: `rust/src/app.rs` の段階的分割を、機能互換と既存テスト資産を維持したまま進める。
- Docs: `docs/APP_SPLIT_PLAN.md`, `docs/REQUIREMENTS.md`, `docs/SPEC.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
- Updated: 2026-03-06

## Task list
| ID | Status | Area | Summary | Dependencies | DoD | Next action |
| --- | --- | --- | --- | --- | --- | --- |
| T-001 | DONE | Planning | `app.rs` 分割方針と Phase 順を確定する | - | `docs/APP_SPLIT_PLAN.md` が作成されている | AGENTS 一時方針へ反映 |
| T-002 | DONE | Process | `AGENTS.md` に一時的な分割実行ポリシーを追加する | T-001 | `AGENTS.md` から参照順と更新ルールが読める | Phase 1 に着手 |
| T-003 | DONE | Tests | `app.rs` inline test を `rust/src/app/tests/` へ分離する | T-001, T-002 | inline test が外出しされ `cargo test` が通る | Phase 2 の worker 境界を棚卸し |
| T-004 | DONE | Workers | worker request/response 型と `spawn_*_worker` 群を `workers.rs` へ移す | T-003 | worker 群が別モジュール化され `cargo test` が通る | Phase 3 の session/tab 永続化境界を棚卸し |
| T-005 | DONE | Session | session/tab restore/persistence を `session.rs` または `tabs.rs` に分離する | T-004 | session 系責務が分離され `cargo test` が通る | Phase 4 の input 依存を棚卸し |
| T-006 | TODO | Input | shortcut/query history/IME を `input.rs` に分離する | T-005 | input 系責務が分離され `cargo test` が通る | shortcut 群の依存を整理 |
| T-007 | TODO | Render | `update()` 内の panel/dialog 描画を整理する | T-006 | `update()` が orchestration 中心になる | top/status/dialog/central の単位を確定 |
| T-008 | TODO | Cleanup | 一時 AGENTS 項目を削除し、計画完了を文書へ反映する | T-003, T-004, T-005, T-006, T-007 | `AGENTS.md` から一時項目が削除されている | 完了時に cleanup |

## Blocked
- なし

## Done
- T-001: `docs/APP_SPLIT_PLAN.md` を追加し、段階的分割方針を確定。
- T-002: `AGENTS.md` に `docs/APP_SPLIT_PLAN.md` / `docs/TASKS.md` / `docs/WORKLOG.md` を参照する一時運用ルールを追加。
- T-003: `app.rs` の inline test を `rust/src/app/tests/` 配下へ分離し、`cargo fmt` / `cargo test --manifest-path rust/Cargo.toml --locked` の green を確認。
- T-004: worker runtime、request/response 型、`spawn_*_worker` 群と専用 helper を `rust/src/app/workers.rs` へ移し、`cargo fmt` / `cargo test --manifest-path rust/Cargo.toml --locked` の green を確認。
- T-005: session 永続化データ型と `load/save/sanitize` helper を `rust/src/app/session.rs` へ移し、`cargo fmt` / `cargo test --manifest-path rust/Cargo.toml --locked` の green を確認。
