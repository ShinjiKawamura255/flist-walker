# CHANGE PLAN: God Object Resolution (Command Pattern Introduction)

## Metadata
- Date: 2026-04-03
- Owner: Antigravity Agent
- Target Project: FlistWalker
- Scope Label: god-object-resolution
- Related Tickets/Issues: Refactoring Technical Debt

## 1. Background
- `FlistWalkerApp` が God Object 化している問題に対し、これまで様々な分割アプローチが取られ、現行では `IndexCoordinator`, `SearchCoordinator` などの境界が一定構築されています。
- しかし、一部の垂直機能（特に `FileList` 関連処理）において、状態遷移の中に UI の更新や他ワーカーへのディスパッチ処理が混在し、依然として `&mut FlistWalkerApp` に強く依存（密結合）しています。
- これにより単体テストの脆さや `request_id` などのライフサイクルの複雑化が顕在化しているため、対象を絞って純粋な状態遷移関数（pure reducer）＋ 副作用の分割への移行を行います。

## 2. Goal
- 「全面的な Command パターン導入（新しい巨大ルーターの構築）」ではなく、**`FileListWorkflowState` という具体的な 1 つの垂直スライスを対象に、コマンドパターンの有効性を検証・適用する。**
- `FlistWalkerApp` に依存していたメソッド群（`create_filelist` 等）を `FileListManager` にカプセル化し、所有する状態、発行する副作用、保持すべき `request_id` 契約を明確にする。
- プロジェクト要件である「古い応答による UI の巻き戻し禁止（`request_id` による管理）」を設計レベルで厳格に維持する。

## 3. Scope
### In Scope
- `FileListWorkflowState` を `FileListManager` に昇格させる（状態の所有）。
- `FileList` 関連メソッドを `FileListManager` 内に移動し、外部への副作用を `Vec<Action>` のような戻り値で返すようにリファクタリングする。
- 副作用コマンドは単一の巨大 Enum でなく、カテゴリ分け（`UiCommand`, `WorkerCommand` など）して責務を分けるか、`FileList` 固有の戻り値として型付けする。
- `FileListManager` 自身が FileList worker の request/response のライフサイクルに関する `request_id` を所有・管理し、stale 判定と cancel 責務を実行する。一方、Index refresh の要否判定や他タブ・ルートへの反映（オーケストレーション）は `FlistWalkerApp` 側の責務として残し、必要に応じてコマンドで委譲する境界とする。
- 必要に応じて `DESIGN.md` および `TESTPLAN.md` への設計・検証観点の事前/同時追従を行う。

### Out of Scope
- `TabState`, `UpdateState` など、`FileList` 以外の垂直スライスへの AppCommand 全面導入（今回は安全性のためのスコープ限定）。
- 既存の `SearchCoordinator` や `IndexCoordinator` の内部設計の変更。
- 非同期 Worker 自体の処理ロジックやスレッド間通信（mpsc）の変更。

## 4. Constraints and Assumptions
- この変更による UI のレスポンス低下は許容されない（UI をブロックしないこと）。
- 既存の「`request_id` を用いた非同期 UI 契約」を破壊しないこと。

## 5. Current Risks
- Risk: `FileList` 周りのコマンド化に伴い、`request_id` にひもづく Stale Response の破棄責務が漏れ、古いレスポンスで UI が巻き戻る。
  - Impact: 高（致命的な要件違反）
  - Mitigation: `FileListManager` 内での `request_id` 判定ロジックを必ず持たせ、Stale は Manager 内で `ignored` として吸収する。
- Risk: FileList/Walker 経路の変更による予期せぬパフォーマンス劣化。
  - Impact: 高
  - Mitigation: 変更前後で `TESTPLAN.md` に基づく perf ignored テストを確実に実行し、指標の変化を確認する。

## 6. Execution Strategy
1. Phase 1: 副作用コマンド型の定義と境界設計
   - Files: `src/app/filelist.rs`, `docs/DESIGN.md`, `docs/TESTPLAN.md`
   - Action: `FileList` に閉じた副作用 (`UiCommand`, `WorkerCommand` 相当) を定義する。事前に `DESIGN.md`, `TESTPLAN.md` に新しいディスパッチアーキテクチャの概要と検証観点を追記する。
   - Expected result: 型が定義され、ドキュメントとのトレースが取れる状態になる。
   - Verification: `cargo check` が通る。
2. Phase 2: `FileListManager` の構築と `request_id` 契約の委譲
   - Files: `src/app/filelist.rs`, `src/app/state.rs`
   - Action: `FileListWorkflowState` を包含する `FileListManager` を定義し、`create_filelist` 等のメソッドを移行する。自身で `request_id` を持ち、stale を破棄する責務をテストコードと共に明示する。
   - Expected result: FlistWalkerApp から FileList 固有のステート直接操作がなくなる。
   - Verification: `cargo test` で `index_pipeline` や `dialogs_and_inflight` が通ることを確認。
3. Phase 3: FlistWalkerApp との結合とパフォーマンステスト
   - Files: `src/app/mod.rs`
   - Action: `FlistWalkerApp` が `FileListManager` を呼び出し、返ってきたコマンドを UI や Worker にディスパッチするように結線する。
   - Expected result: 全機能が以前と同様に動作し、`&mut FlistWalkerApp` への不要な依存が FileList 処理から消滅する。
   - Verification: 
     - 全体の `cargo test` 実行
     - `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture` 実行
     - `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture` 実行

## 7. Detailed Task Breakdown
- [ ] Task 1: `DESIGN.md` および `TESTPLAN.md` の先行更新（アーキテクチャ定義・検証観点の追加）。
- [ ] Task 2: `src/app/filelist.rs` 等での副作用コマンド列挙（`FileListAction` など）の定義。
- [ ] Task 3: `FileListWorkflowState` を `FileListManager` にリファクタリング。
- [ ] Task 4: `FlistWalkerApp` のメソッドを `FileListManager` に委譲し、状態変更＋コマンド返却への修正。
- [ ] Task 5: 返却されたコマンドを `FlistWalkerApp` で受け取りディスパッチするルーターの構築。
- [ ] Task 6: Unit Test の修正と実行。
- [ ] Task 7: Perf Test の実行。

## 8. Minor Review Notes
- Phase 1 の完了条件は `cargo check` だけでなく、`DESIGN.md` / `TESTPLAN.md` の差分レビューと ID/参照整合確認も実施する。
- Progress Log は今回の再々計画まで追記し、実装開始後に「どのレビュー指摘を吸収した版か」を追跡できるようにする。

## 9. Implementation Checkpoints
- Checkpoint 1: `FileListManager` は FileList worker request/response の lifecycle だけを所有し、`pending_after_index` や tab/root orchestration を抱え込まない。
- Checkpoint 2: stale response は `FileListManager` 内で吸収し、`FlistWalkerApp` 側へ古い完了通知を漏らさない。
- Checkpoint 3: `create_filelist` 系の pure state transition と、notice 更新・worker dispatch・index refresh 要求を分離したまま実装する。
- Checkpoint 4: `request_index_refresh` / `request_create_filelist_walker_refresh` との境界を維持し、既存の `IndexCoordinator` / `SearchCoordinator` の内部契約へ波及させない。
- Checkpoint 5: Phase ごとに docs 更新、unit test、必要な perf test を完了させてから次へ進み、途中で scope が広がったら先に計画書を更新する。

## 10. Validation Plan
- Automated tests: `cargo test` が完全に通ること。
- Performance Check: `FileList` / `Walker` のプロファイルが変わらないことを ignored perf test で保証する。
- Manual Checks: 実機環境でのタブ切り替え、FileList の生成中ダイアログキャンセル、及び Stale リクエスト無視が想定通りか。

## 11. Rollback Plan
- リファクタリングは段階ごとの小刻みなコミットで行うか、機能ブランチ上で行う。テストや Perf Check が基準を満たさなかった場合は、変更対象のファイル単位での `git restore <file>`、または `git revert` による確実な戻しを行う。ワーキングツリーの他変更を巻き込む危険がある `git reset --hard` は使用しない。

## 12. Temporary `AGENTS.md` Rule Draft

```md
## Temporary Change Plan Rule
- For `god-object-resolution`, read `docs/CHANGE-PLAN-20260403-god-object-resolution.md` before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 13. Progress Log
- 2026-04-03 Planned. 初期計画策定。
- 2026-04-03 Plan Revised. レビューによる危惧（現状アーキテクチャの破壊リスク、request_id契約の未定義、巨大コマンド化リスク、テスト基準の甘さ）を受け、対象を FileList 系に絞り、SDD/Perf Test を Exit criteria とするアプローチに修正。
- 2026-04-03 Plan Revised Again. Manager の責務境界を FileList worker lifecycle と orchestration に分離し、rollback を `git restore` / `git revert` ベースへ修正。さらに軽微なレビュー指摘と実装チェックポイントを計画へ追加。

## 14. Completion Checklist
- [x] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `DESIGN.md` / `TESTPLAN.md`
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 15. Final Notes
- アーキテクチャの恒久変更となるため、Phase 1 の段階で必ず `DESIGN.md` および `TESTPLAN.md` の同時更新を行うこと。
