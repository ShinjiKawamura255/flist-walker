# 復旧性・仕様実装テスト整合レビュー 2026-09-21

## 対象と判断基準

- 基準: `b034ede1ad454e2f776dd66d9cc0ae4272912fed`。この記録は同時点の診断と、その後の局所修正を対象とする。
- 環境: Windows、Rust 1.97.1、`x86_64-pc-windows-gnu`。
- 適用手法: `operational-resilience-review` と `spec-code-test-gap-review` を別々に実施した。
- 選択した重要経路: UI-state/history/saved-roots の保存と復旧、worker 切断・終了、FileList の途中失敗、action の部分失敗、updater の曖昧な復旧状態。全機能・全故障条件の網羅監査ではない。
- 仕様の正本は `docs/INDEX.md` が指す SP、実装は実際の分岐、テストは到達経路と assertion を独立に照合した。レビュー中は実環境への障害注入、外部アプリ起動、利用者データの変更を行っていない。
- 全体の RTO/RPO は未定義。既存の bounded shutdown と crash-before-flush の履歴損失許容を確認したが、独自の数値目標は追加していない。

## Operational resilience review

### OR-01: 読込不能・不正な保存データを空 object として上書きする

- Severity: **High**。Confidence: **High**。Disposition: **Fix + Test + Document**。
- Scenario: UI-state が不完全な JSON、無効 UTF-8、non-object、または読み込み不能になった後に履歴保存・設定確定が発生する。
- 原因: 基準時点の `build_ui_state_document` は read/parse error を `.ok()` で捨て、空 object に置き換えていた。破損内容の保全や復旧を行う前に、通常の書き込みで元 bytes を失う可能性がある。
- Evidence: [worker](../../rust/src/persistence/worker.rs) の `build_ui_state_document`、`write_pending_ui_state`、`commit_settings_with_writer`。追加した3件の再現テストは修正前に失敗した。
- Detection: 確定操作は request identity 付き応答、公開 history API は `flush`/`shutdown` のエラーで検知する。GUI の通常 autosave に新しい常設エラー表示を追加したわけではない。
- 修正: `NotFound` のみ空 object を許可する。read/UTF-8/parse/non-object failure はエラーとし、関連ファイルへの最初の書き込みより前に停止する。通常の pending history は保持し、修復後に最新 object へ再適用する。型付き起動 reader の従来の default fallback は維持する。
- 検証: `tc_167_unreadable_existing_document_is_not_an_empty_merge_base`、`tc_167_invalid_document_blocks_settings_commit_before_any_write`、`tc_167_invalid_document_retry_preserves_history_until_external_repair`。元 bytes、writer呼出し0件、修復後の `old,B,A` 順序と未知フィールド保持を assertion した。
- Residual: process exit 前に flush できなかったメモリ上の履歴は保証しない。ファイルの自動修復は行わない。[手動復旧手順](../SUPPORT.md#ui-state-persistence-recovery)を記載した。

### Failure / recovery matrix

| Scenario | Expected behavior | Detection | Containment | Recovery | Evidence | Gap |
| --- | --- | --- | --- | --- | --- | --- |
| UI-state read/parse failure | 元 bytes と関連 roots を保持 | commit/flush error | write 前に拒否 | 修復後の retry | OR-01 の3テスト | 隔離 fixture で検証。実ユーザファイルの修復訓練は未実施 |
| lock contention / 通常 write failure | 順序付き pending を失わない | flush error / commit response | worker 内の待機 | lock 解放・保存先修復後 retry | `tc_167_persistence_retries_lock_timeout_without_losing_generations`、`tc_168_persistence_enqueue_does_not_wait_for_a_held_lock` | OS I/O 自体の応答時間・長時間資源枯渇は未計測 |
| 二つ目の settings write failure | 先行 roots を復元 | commit error | UI-state未変更、roots rollback | 明示再操作 | SG-01 の修正テスト | 実ディスク満杯ではなく writer fault injection |
| replace後syncと両rollbackの失敗 | 成功を返さず全エラーを残す | 元エラー + 各rollback error | 追加の成功通知をしない | bytes保全・手動照合 | SG-01 の追加テスト | 電源断・物理媒体故障は未実施 |
| worker切断・終了 | 待機を解放、無期限joinを避ける | unavailable表示 / shutdown diagnostics | stale/cancel、bounded join | 再起動 | [search_failure](../../rust/src/app/tests/search_failure.rs)、[shutdown](../../rust/src/app/tests/shutdown.rs)、TC-153 | OS停止・native操作を伴う訓練は未実施 |
| FileList途中失敗・rollback panic | 旧内容復元または部分失敗を明示 | report/exit code | replacement/rollback境界 | retryまたは手動確認 | [indexer tests](../../rust/src/indexer/tests/mod.rs) の `tc165_panic_replacing_later_target_rolls_back_earlier_commit`、`tc165_panic_during_rollback_is_reported_without_unwinding` | fixtureによる検証 |
| action直前の再認可失敗 | 残件停止、開始済み件数を明示 | partial completion | backend呼出し前の再検証 | 利用者が結果確認後に再操作 | [action_commands](../../rust/src/app/tests/action_commands.rs) の `tc_050_worker_reports_partial_completion_when_recheck_fails` | 実OS操作は未実施。開始済みactionのrollback保証なし |
| updater hash/marker が曖昧 | 自動改変せず証跡保持 | Ambiguous/startup diagnostics | marker/backup保持 | clean parallel installation | [transaction tests](../../rust/src/updater/transaction/tests.rs) の `tc159_ambiguous_hash_state_preserves_recovery_evidence`、[runbook](../UPDATER_RECOVERY.md) | algorithmはfixture、実配布物の復旧訓練はDocumented only |

## Spec-code-test gap review

### SG-01: rollback と命名されたテストがrollbackまで到達しない

- Primary category: **WEAKLY_VERIFIED**。Severity: **Medium**。Confidence: **High**。Disposition: **Test**。
- Specification: [SP-016](../spec/operations-release-config.md#sp-016-runtime-config-bootstrap) は複数ファイルの一方が失敗した際の復元、復元失敗時の明示、元エラーと復元エラーの保持を要求する。
- Implementation: `commit_settings_with_writer` は先行 roots write 後の UI-state write failure で roots を復元する。replace後sync failureでは両方の復元を試み、エラーを集約する。
- Tests: 基準時点の `tc_167_observed_settings_commit_rolls_back_saved_roots_when_ui_state_write_fails` は UI-state path に directory を置いていた。冒頭の `read_optional_file` で終了し、roots write/rollbackに到達しない。別のpost-replaceテストは正常な復元を検証するが、両復元の失敗によるエラー集約は直接検証していなかった。
- 現実的な変更リスク: pre-replace failure時のrollbackだけを削除、またはエラー集約で最後のエラーだけを返す変更が、該当テストでは検出されない。
- 修正: 有効な両ファイルを用意し、rootsが新内容になったことを確認してからUI-state writeを失敗させる。roots→UI-state→rootsの呼出しと最終bytesを確認する。追加テストはpost-replace sync failureと両restore failureを注入し、元エラー・両rollbackエラーと実際の未復元bytesを確認する。
- 検証: [storage tests](../../rust/src/persistence/worker/tests.rs) の修正済み同名テスト、および `tc_167_settings_rollback_failures_preserve_original_and_both_restore_errors`。
- 追加の仕様判断: rollback契約は既存のまま。OR-01の保存前読込拒否だけをSP-016/DES-017/TC-167へ明記した。起動時default fallbackと書込側のデータ保全を混同しない。

### Behavior matrix

| Behavior | Specification | Implementation | Tests | Gap / disposition |
| --- | --- | --- | --- | --- |
| 不正なmerge baseの扱い | 基準時点ではエラー時retry/未知項目保持はPresent、parse失敗時の扱いはAmbiguous | 空objectへfallback | 当該異常条件はMissing（storage tests全件を確認） | OR-01で保全契約を明確化し実装・回帰テストを追加 |
| 複数ファイルrollbackとエラー保持 | SP-016 Present | commit/restore/error集約 Present | pre-replace到達と複数復元失敗はPartial | SG-01で補強 |
| 履歴disabled・順序・100件・未知項目 | SP-010/SP-016 Present | persistence worker Present | TC-167のno-op/順序/cap/two-processテスト Present | 調査範囲でAligned |
| 設定保存失敗時のGUI草稿/live state | SP-010 Present | settings commit responseでsuccessのみ適用 | `root_list_manager::tc_167_saved_root_failure_keeps_live_and_draft_state_for_retry` Present | 自動テストでAligned、native再操作はUnknown |
| action認可・部分失敗 | SP-004 Present | [actions.rs](../../rust/src/actions.rs) のwhole-request認可と直前再認可 | TC-050/051、TC-164 Present | recording backendでAligned、実OS/UNCはUnknown |
| FileList transaction | SP-001 Present | filelist writerのreplacement/rollback | TC-165 Present | fixtureでAligned |
| updater ambiguous recovery | SP-014 Present | transaction recoveryがhash/phase検証 | TC-159 Present | fixtureでAligned、runbook実演はUnknown |

### Change probes

1. **「読込失敗を空objectとして保存」へ変更**: SP-016、merge-base判断、TC-167へ影響。新規3テストがbytes破壊または成功応答を検出する。修正前の実際の失敗結果が根拠。Result: Guarded。
2. **pre-replace rollbackの削除、または復元エラーの上書き**: SP-016の不変条件に反する。修正・追加テストが実際の途中writeと復元呼出し、全エラー文字列を確認する。Result: Guarded（静的追跡、実装mutationは未実施）。
3. **履歴上限・順序・disabled方針の片側変更**: SP-010/SP-016、共通history policy、writer/reader、TC-167へ影響。既存のcap100、A/B/A、disabled、複数processテストが主要契約を固定する。Result: Guarded within tested paths。仕様文言だけの変更を機械的に検知する保証はない。

## 検証と残余リスク

高確度の指摘2件は修正済み。既存rollbackの構造に変更はなく、検証経路を強化した。修正前のstorage suiteは29件成功・追加3件失敗（exit 101）、修正後は32件成功（exit 0）だった。

| Command / check | Result |
| --- | --- |
| `cargo test --locked --target x86_64-pc-windows-gnu --lib persistence::worker::tests` | PASS: 32件。lock contention、frame dispatch、未知フィールド、複数process、失敗・復旧を含む |
| `cargo test --locked --target x86_64-pc-windows-gnu` | PASS: lib 1395 + fw 8 + architecture 2 + CLI 47 + path-key 2 = 1454件。通常suiteのignored 15件は未実行。child writerの1件は内側の実行なので合計へ重複加算しない |
| `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings` / `cargo fmt --check` | PASS |
| `cargo audit --file Cargo.lock` | PASS: 452 dependencies、1251 advisories時点 |
| `python -m unittest discover -s scripts/tests` | PASS: 62件。tooling削除、audit safe-skip、native clippy、worktreeの既存negative guardsを含む |
| `python scripts/check_repo_contract.py` / `python scripts/check_ci_policy.py --guardian .` | PASS |
| workflow YAML parse | PASS: 8ファイル |

RustはWindows GNU helperで環境を初期化した。Pythonは環境に付属する3.12.14 runtime、YAML検証は既存のvalidation用PyYAMLを使用した。`validate_change.py --base b034ede1ad454e2f776dd66d9cc0ae4272912fed --plan` はVM-001/007/008/009を選択した。VM-009はTESTPLAN文書変更に伴うローカル検証であり、workflow/trusted checker/remote policy変更やproof PRは対象外。GUIの描画・入力・indexing algorithmを変更していないため、新たなnative smokeやindexing性能試験は追加していない。

恒久成果物はworkerの局所修正、回帰テスト、SP-016/DES-017/TC-167の補足、復旧手順、およびこの診断記録である。利用者データや外部状態の変更、push、PR、releaseは行っていない。

実行環境外のLinux/macOS、native GUI/IME/DPI/複数画面、実UNC・外部Open、電源断、実媒体のdisk-full、実インストールのrollback/restore、remote CIはNOT RUN。自動テストの成功をこれらの実証とみなさない。長期障害時のメモリ増大や全workerの全故障組合せも今回の網羅対象ではない。診断の高確度2件を修正対象とし、追加の外部操作を必要とする復旧訓練は実施していない。
