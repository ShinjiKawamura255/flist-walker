# Test Strategy and Levels

## Scope and priority
- Target: FileList 優先ロジック、walker、検索、実行/オープン分岐、CLI 契約、GUI 主要フロー、逐次表示/キャッシュ境界。
- Priority:
- P0: FR-001/002/003/004/005
- P1: FR-006/007, NFR-002/003/004
- P2: NFR-001（性能計測）

## Test levels
- Unit:
- FileList 検出/解析
- walker 走査
- fuzzy 検索順位
- action 分岐
- GUI ロジック（ui_model）
- Integration:
- 一時ディレクトリで index -> search -> action 連携を確認。
- CLI 実行で出力契約を確認。
- App test module policy:
- app-level regression は owner/command seam ごとに module を分けて保守する。update は `rust/src/app/tests/update_commands.rs`、session restore は `rust/src/app/tests/session_restore.rs`、tab/background routing は `rust/src/app/tests/tab_lifecycle.rs` / `rust/src/app/tests/tab_drag.rs` / `rust/src/app/tests/tab_background_responses.rs`、tab snapshot contract は `rust/src/app/tests/tab_contract.rs`、index/filelist lifecycle は `rust/src/app/tests/index_pipeline/*` を主対象にし、`app_core.rs` へ unrelated fixture regression を増やし続けない。
- `rust/src/app/tests/tab_contract.rs` の `tab_state_contract_round_trip_pins_field_layout` は `TabIndexState` / `TabQueryState` / `TabResultState` / `AppTabState` の field layout を固定する contract regression として扱う。
- routing / cleanup の確認は `rust/src/app/response_flow.rs`、`rust/src/app/result_reducer.rs`、`rust/src/app/index_coordinator.rs`、`rust/src/app/pipeline.rs`、`rust/src/app/tab_state.rs`、`rust/src/app/worker_bus.rs` を owner seam として扱い、background response の stale discard は `tab_background_responses.rs`、tab close cleanup は `tab_lifecycle.rs` / `tab_result_cache.rs`、index lifecycle cleanup は `index_pipeline/filelist_lifecycle.rs` へ寄せる。
- `FeatureStateBundle` / `TabSessionState` のような state bundle 導入後も、bundle 単位の ownership を直接確認したい回帰は既存 owner test module に寄せ、bundle 配置だけを検証するための横断 fixture を増やさない。
- stale response discard、cancel cleanup、pending/inflight 解放の契約は `update_commands.rs` と `index_pipeline/*` を優先対象にし、`app_core.rs` へ cross-cutting でない lifecycle regression を戻さない。
- filelist response の current/previous/stale-requested-root 分岐は `rust/src/app/tests/index_pipeline/filelist_lifecycle.rs` を owner test とし、request cleanup と post-settle routing を同じ module で固定する。
- runtime settings は Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` と関連する session file に集約し、`FLISTWALKER_*` は初回 seed としてのみ扱う。環境変数は validation 上 `dev/test override`、`build/release` に分けて扱う。`README.md` では config file の場所と seed-only 挙動を明記し、dev/test override は `TESTPLAN.md` と実装近傍 test に閉じる。
- GUI Manual:
- 起動、検索、選択、プレビュー、実行/オープン、再読込を `docs/GUI-TESTPLAN.md` の `GSM-*` 手順で検証する。
- GUI smoke fixture は `scripts/gui-smoke-fixture.sh` で作成し、証跡は `rust/target/gui-smoke/evidence/` に記録する。手動で報告を作る場合は `docs/GUI-TESTREPORT.template.md` を雛形にする。
- release candidate または VM-002 対象の GUI-adjacent 変更では、該当 `GSM-*` の PASS / FAIL / SKIPPED と証跡パスを必ず記録する。単なる「手元で見た」だけでは gate 完了扱いにしない。
- GUI Headful Smoke:
- release candidate / nightly では `scripts/gui-headful-smoke.sh` または `scripts/gui-headful-smoke.ps1` で native window 起動の早期クラッシュを検出し、`rust/target/gui-smoke/evidence/GUI-HEADFUL-SMOKE.local.md` に記録する。通常 PR の required gate にはしない。
- Perf/Sec:
- Perf: 10万件相当ダミー候補で検索時間計測。
- Perf: 軽量 PR gate は `perf_filelist_stream_is_faster_than_metadata_probe_baseline` とし、include_files/include_dirs 両有効の FileList stream で line-only fast path を metadata-probe baseline に対して維持する。hosted Linux runner の揺れを吸収するため、CI の下限は 1.20x とする。heavy suite は `perf_walker_classification_is_faster_than_eager_metadata_resolution` と `perf_adaptive_walker_reports_local_dataset_metrics` として分離し、walker 側の現行 control baseline は 1.25x を下限としつつ、adaptive の件数一致・実行時間・read_dir 制御指標も継続計測する。
- Coverage: CI の `lint-and-coverage` job は `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75` を実行し、line coverage 75% 未満への低下を失敗扱いにする。2026-05-14 の fresh baseline は 79.08%（LH=12604 / LF=15938）。中期目標は 80% とする。enforced threshold を上げる変更では、同一変更内で fresh baseline、失敗時の不足領域、追加した owner-seam test を記録する。
- Sec: コマンド引数を配列化しシェルインジェクションを回避。
- Sec: root 外パス実行拒否、履歴永続化無効化、CI の依存脆弱性検査を確認。
- Sec: Windows の一般 `.ps1` は既定で直接実行せず、既定アプリでオープンする。
- Sec: 自己更新は `SHA256SUMS.sig` の署名検証と checksum 検証を通過した asset のみを staged binary として採用する。
- Sec: `cargo audit` の accepted transitive warning は `docs/OSS_COMPLIANCE.md` に owner、review cadence、re-evaluation trigger を明記し、release candidate ごとに再確認する。
