# Validation Matrix and Runner Commands

## Regression Guard
- 発生条件: 検索結果の更新時に 100 行目へカーソルがある状態で結果数が 100 未満へ減る、または current row が未選択のまま再検索が走る。
- 期待動作: current row はユーザ操作なしで別の行へ移動せず、保持できる場合は同じ行番号を維持し、縮小した場合のみ末尾へ丸める。未選択状態は自動選択に変換しない。
- 非対象範囲: 手動の Arrow キー移動、Sort 切替、Root 変更による既存 selection 破棄。
- 関連テストID: TC-068.
- 発生条件: `copy_selected_paths` の Windows-only テストで、`FlistWalkerApp` の旧 `notice` 直参照が残る。
- 期待動作: notice は live runtime の `app.shell.runtime.notice` を参照し、`\\?\` 付きの extended prefix を正規化した結果だけを検証する。
- 非対象範囲: copy パス実装そのものの出力形式変更、Windows 以外の OS の path normalization。
- 関連テストID: TC-121.
- 発生条件: `egui-winit` が `Ctrl+Shift+C` / `Cmd+Shift+C` を `Event::Copy` に変換し、`Key::C` の shortcut test だけでは path copy 経路が検知できない。
- 期待動作: Shift 付き primary copy event は選択中または PIN 済み path をコピーし、Shift なしの通常 copy event は path copy shortcut として扱わない。
- 非対象範囲: TextEdit 内の通常 query text copy、Copy Path(s) ボタン経由の直接実行。
- 関連テストID: TC-018.
- 発生条件: Walker 完了後に visible な結果が少数しかないのに、全件 kind 解決が走って巨大な on-demand root を走査し続ける。
- 期待動作: kind 解決は visible results に限定し、検索/index が停止済みの idle 状態では全件 metadata 解決を継続しない。
- 非対象範囲: Files / Folders の単一フィルタ時に必要な kind 解決、preview 要求に伴う単発の kind 解決。
- 関連テストID: TC-122.

## Change-Type Checklist
Use this checklist before selecting runner commands. The VM table below remains the normative validation matrix; this section is an operator-friendly entrypoint for common change intents.

### Docs-only or SDD/TDD Document Updates
- Apply: VM-001.
- Check that the touched docs keep `FR/NFR/CON -> SP -> DES -> TC` references intact when IDs are mentioned.
- Review the affected diff for obsolete assumptions, duplicated instructions, and local Markdown links.
- Run focused `rg` checks for renamed headings, IDs, and file references.
- Do not run `cargo test` when the diff is limited to docs and `AGENTS.md`; confirm that with `git diff --stat`.

### GUI Orchestration, Rendering, Input, Tabs, or Session Changes
- Apply: VM-002.
- Keep heavy I/O and long computation out of the egui frame path.
- Preserve request routing, stale response handling, tab/background response ownership, and selection behavior.
- Add focused tests under `rust/src/app/tests/` that match the owner module touched.
- Run GUI smoke evidence when rendering, focus, tabs, dialogs, result drawing, or responsiveness changes.
- For tab ownership transfer, run TC-154 plus `tab_contract`, `tab_lifecycle`, `tab_result_cache`, `tab_background_responses`, `query_history`, `session_restore`, and `filelist_lifecycle`; keep allocation-identity fixtures non-compacting/non-sparse, include tab-owned entry-kind cache identity and active-scratch stale-routing guards, and verify result compaction separately.

### Bounded Worker Scheduling or Shutdown Changes
- Apply VM-002 to action/kind dispatch, worker bus, load accounting, runtime handle ownership, and shutdown changes; run focused TC-150, TC-151, and TC-153 tests in addition to the full Rust suite.
- Apply VM-003 as well when index dispatch, index coordinator, index worker, or stale-before-canonicalize behavior changes; run focused TC-152 and the VM-003 ignored FileList/Walker performance tests.
- Verify the fixed limits exactly: action 2 + 8 = 10, kind 1 + 256 = 257, index 2 + 2 = 4 including stale, coordinator tracking <= 2, app pending <= 4 and latest one per tab.
- Exercise `Accepted`, `Full`, `Disconnected`, stale/cancel, error, panic unwind, and shutdown-timeout paths. Assert terminal settlement, zero leaked load, no filesystem I/O before stale rejection, and UI dispatch latency independent of queue availability.

### Indexing, FileList, Walker, or Kind Resolution Changes
- Apply: VM-003.
- Update indexer tests before changing FileList detection, precedence, root lookup, nested FileList handling, or walker classification.
- FileList byte decoding、BOM、line bound、ancestor read/append を変更する場合は TC-161 を先に red/green 実行し、stable invalid root の callback 0 件、invalid child subtree 不変、invalid ancestor no-rewrite、64 KiB 以下の cancel cadence を確認する。
- Preserve incremental ingestion, keep regular FILE/DIR classification on the `file_type` fast path, and avoid full-list synchronous metadata resolution in idle UI paths. Confirm LINK identity is not used as a fallback for special files and terminal OTHER results are not requeued.
- Run the VM-003 ignored perf tests when index/filelist/walker paths are touched. FileList encoding preflight を変更する場合、metadata-probe/allocating-lines controls は production と同じ preflight を通し既存 threshold を維持し、validation-only と total parse elapsed を記録する。
- Add large-root manual GUI checks when the change can affect responsiveness or throughput.

### Search or Query Contract Changes
- Apply: VM-004.
- Update SPEC/DESIGN/TESTPLAN together when operator behavior, ranking, matching, highlight, case sensitivity, or compatibility changes.
- Add or update failing tests first for query operators such as `'`, `!`, `^`, `$`, and `|`.
- Verify CLI and GUI-facing behavior stay aligned.
- Add focused GUI checks when highlight, visible result filtering, or user-facing result ordering changes.
- Shared evaluator/cache changes run focused TC-155 public-adapter/score/span/compile-count/cache-context coverage before the full suite.
- 100k search performance changes run TC-156 explicitly with `cargo test --release --locked perf_search_100k_cold_warm_query_shapes --lib -- --ignored --nocapture`; record median/maximum/evaluated candidates and validate `.github/workflows/perf-regression.yml`.

### CLI, Build, Release, Updater, or OSS Packaging Changes
- Apply: VM-005.
- Run the project-local release preflight skill before tag/release/publish work.
- Update `docs/RELEASE.md`, `.github/release-template.md`, OSS notices, and asset sidecar handling together when packaging changes.
- Check release asset names, target OS coverage, update manifest/security behavior, and workflow warning gates.
- Updater staging 変更では TC-157 を failing-first で追加し、trust-first request order、strict manifest、byte/time/redirect bounds、streaming hash、partial cleanup を focused 実行する。
- Updater activation/recovery 変更では TC-158 と TC-159 を failing-first で追加し、Windows/Linux 両方の TC-160 inert dummy filesystem 証跡を必須とする。実行中 binary の置換または外部 application 起動を行わない。
- Keep macOS notarization status wording in release notes while the temporary non-notarized publish posture remains active.

### CI Coverage, GUI Validation Docs, or Smoke Script Changes
- Apply: VM-006.
- Validate shell/PowerShell scripts with the parser checks listed in VM-006.
- Keep GUI test plan IDs, report template fields, smoke script names, and workflow references synchronized.
- Treat coverage threshold changes as quality-policy changes that require fresh baseline measurement and docs updates.

### Supportability Docs, Templates, or Diagnostics Wording
- Apply: VM-007.
- Check redaction wording, telemetry/support language, issue template links, and forbidden internal env names.
- Keep diagnostics instructions aligned with the worker tracing and window trace contracts.
- Do not require Rust validation if only support docs/templates changed.

### Runtime Config, Settings, or Startup Bootstrap Changes
- Apply: VM-008.
- Keep runtime config seed-only behavior and migration rules aligned across code and public docs.
- Do not mention development-only update override environment variables in public-facing docs or help.
- Verify first-run config creation, existing-config precedence, and startup/session path behavior.
- Update release/config docs when user-facing settings locations or defaults change.

## Runner and commands
- Runner: `cargo test`
- Runner: `cargo test`, `cargo audit`
- Validation Matrix:
| Change Type | Typical Targets | Required Validation | Optional / Follow-up |
| --- | --- | --- | --- |
| VM-001 Docs only | `docs/*.md`, `AGENTS.md`, release note text only | affected doc diff review, `rg` で ID/参照整合を確認 | Rust 実装に触れない限り `cargo test` は不要 |
| VM-002 App/UI orchestration | `rust/src/app/mod.rs`, `rust/src/app/*.rs` の state/render/input/session/update/filelist/tab_state/tabs/bootstrap/cache/worker_bus/worker_runtime/worker_tasks 変更 | `cd rust && cargo test`; bounded worker scheduling または shutdown を変えた場合は TC-150、TC-151、TC-153 の focused tests; tab ownership transfer を変えた場合は TC-154 と owner-focused tests | render facade/module 境界を変えた場合は `cd rust && cargo test --locked render_tests` と `cd rust && cargo test --locked run_ui_frame` を追加確認する。dialog / focus / tab 操作、検索結果描画、入力応答性、tab 描画、または structural refactoring を変えた場合は `scripts/gui-smoke-fixture.sh` を実行し、`docs/GUI-TESTPLAN.md` の該当 `GSM-*` を `rust/target/gui-smoke/evidence/GUI-TESTREPORT.local.md` などの実行証跡へ記録する。routing / lifecycle を触った場合は `tab_contract.rs` / `tab_lifecycle.rs` / `tab_background_responses.rs` / `tab_result_cache.rs` / `session_restore.rs` と `index_pipeline/filelist_lifecycle.rs` の owner regression を追加確認する。window trace の observable output を変えた場合は TC-120 の focused smoke を追加実施する |
| VM-003 Indexing path | `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/filelist_hierarchy.rs`, `rust/src/indexer/walker.rs`, `rust/src/indexer/filelist_writer.rs`, `rust/src/app/index_worker.rs`, `rust/src/app/index_coordinator.rs`, `rust/src/app/adaptive_walker.rs`, `rust/src/app/workers.rs`, `rust/src/app/mod.rs`, `rust/src/app/pipeline.rs` の index/filelist/walker 経路 | `cd rust && cargo test`; bounded index scheduling を変えた場合は TC-152 の focused tests; `cargo test perf_filelist_stream_is_faster_than_metadata_probe_baseline --lib -- --ignored --nocapture`; FileList read path を変えた場合は `cargo test perf_filelist_stream_reuses_line_buffer --lib -- --ignored --nocapture`; `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`; adaptive walker 評価時は `cargo test perf_adaptive_walker_reports_local_dataset_metrics --lib -- --ignored --nocapture` | 大規模 root で GUI 手動試験。worker/index trace の observable output を変えた場合は TC-120 の focused smoke を追加実施する |
| VM-004 Search/query contract | `rust/src/query.rs`, `rust/src/search/mod.rs`, `rust/src/search/match_eval.rs`, `rust/src/search/cache.rs`, `rust/src/search/config.rs`, `rust/src/search/execute.rs`, `rust/src/search/rank.rs`, `rust/src/ui_model/mod.rs`, `rust/src/ui_model/highlight.rs`, `rust/src/app/cache.rs`, `rust/src/app/preview_flow.rs`, ignore-filter caller、highlight / sort 契約変更 | focused TC-155; `cd rust && cargo test`; search performance path 変更時は `cargo test --release --locked perf_search_100k_cold_warm_query_shapes --lib -- --ignored --nocapture` と workflow diff | GSM-002 で主要 query (`'`, `!`, `^`, `$`, `|`)、regex/plain、case、multibyte highlight の GUI 手動試験 |
| VM-005 CLI / build / release / updater | `rust/src/main.rs`, `rust/build.rs`, `rust/src/updater.rs`, `rust/src/updater/*.rs`, `scripts/build-rust-*.sh`, `scripts/build-rust-*.ps1`, `scripts/common-win-gnu.ps1`, `scripts/validate-release-bundle.sh`, `.github/workflows/*`, `docs/RELEASE.md` | `cd rust && cargo test`; updater staging は TC-157、activation/recovery は TC-158/159 を focused 実行; updater platform apply/helper は Windows/Linux の TC-160 inert dummy transaction と `cd rust && cargo check --locked --target x86_64-pc-windows-gnu`; changed PowerShell scripts の parser check; `scripts/test-build-rust-win.ps1`; PowerShell native build 変更時は `scripts/build-rust-win.ps1 -CheckOnly -NoInstall` と既存依存による `scripts/build-rust-win.ps1 -NoInstall`、続けて `scripts/test-windows-build-artifact.ps1`; release bundle変更時は `bash -n scripts/validate-release-bundle.sh scripts/test-validate-release-bundle.sh` と `bash scripts/test-validate-release-bundle.sh`、tagged workflow上のbundle検証; release 前は `cargo clippy --all-targets -- -D warnings` と release build logs の warning ゼロを確認する | 実行中 FlistWalker binary の置換と外部 application 起動は禁止。PowerShell native build では両 EXE の hash、`.rsrc`、manifest、GUI subsystem、import DLL を確認する。release/update 導線や platform 資産を変えた場合は該当 manual test と release doc review。workflow 変更時は署名鍵と配布公開鍵の一致、同一tag releaseの上書き禁止、期待24 asset/22 checksum、archive/sidecar license notice、tag workflow の preflight 条件、Windows native test、Windows GNU cross build、`cargo audit`、perf regression workflow の役割分担も確認する |
| VM-006 CI coverage gate / GUI validation docs | `.github/workflows/ci-cross-platform.yml` の coverage command、`docs/TESTPLAN.md` の coverage/render validation 方針、`docs/GUI-TESTPLAN.md`、`docs/GUI-TESTREPORT.template.md`、`scripts/gui-smoke-fixture.sh`、`scripts/gui-headful-smoke.*` | `cd rust && cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75`; workflow diff review。GUI docs/script だけの変更では `bash -n scripts/gui-smoke-fixture.sh`、`bash -n scripts/gui-headful-smoke.sh`、PowerShell parser で `scripts/gui-headful-smoke.ps1` を確認、`scripts/gui-smoke-fixture.sh`、`rg -n "GUI-TESTPLAN|GUI-TESTREPORT|GUI-HEADFUL-SMOKE|gui-smoke-fixture|gui-headful-smoke|GSM-" docs/TESTPLAN.md docs/GUI-TESTPLAN.md docs/GUI-TESTREPORT.template.md scripts/gui-headful-smoke.sh scripts/gui-headful-smoke.ps1` を required validation とする | Rust 実装に触れない場合 `cargo test` は coverage run に含まれるため別実行不要。coverage threshold を 80% へ上げる場合は fresh baseline を再測定し、`TESTPLAN.md` と `CURRENT_STATUS.md` へ測定値、追加 test、残る不足領域を更新する。Headful GUI launch は release/nightly smoke とし、通常 PR の CI 必須にしない |
| VM-007 Supportability docs/templates | `.github/ISSUE_TEMPLATE/*`, `docs/SUPPORT.md`, README support links | affected doc/template diff review; `rg` で redaction / telemetry wording and forbidden internal update override names を確認 | Rust 実装に触れない限り `cargo test` は不要 |
| VM-008 Runtime config bootstrap | `rust/src/runtime_config.rs`, `rust/src/main.rs`, `rust/src/search/config.rs`, `rust/src/app/index_worker.rs`, `rust/src/app/shell_support.rs`, `rust/src/app/session.rs`, `rust/src/updater.rs` | `cd rust && cargo test` | 初回起動で config file が生成されること、既存 file が env より優先されること、seed-only 挙動を manual smoke で確認する |
- 大規模 docs cleanup や plan 撤去のような docs-only 変更では、doc diff review と `rg` 参照整合確認を必須にする。Rust 実装に触れない限り `cargo test` は不要だが、変更対象が docs と `AGENTS.md` に限定されることを `git diff --stat` でも確認する。
- app architecture のような構造改善後も、恒久的な検証基準は VM-001 / VM-002 / VM-003 を直接適用する。
- `ui_model/` は display/highlight/preview concern に限定し、action decision は `actions.rs` 側の unit test と `TC-107` で固定する。
- Commands:
- `cd rust`
- `source ~/.cargo/env`
- `cargo test`
- release 前 warning gate: `cargo clippy --all-targets -- -D warnings` を実行し、release asset build logs に warning が残っていないことを確認する
- `cargo audit`
- audit warning posture: `docs/OSS_COMPLIANCE.md` の accepted transitive warning を確認し、release candidate ごとに `cd rust && cargo audit` を再実行する
- coverage gate: `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75`
- coverage uplift target: 80% は release 直前の義務ではなく中期品質目標として扱う。80% へ上げる前に app/GUI owner seam の不足領域を追加 test で補強し、fresh baseline を再測定する。
- heavy perf regression workflow: `.github/workflows/perf-regression.yml` の manual dispatch または weekly schedule で `perf_filelist_stream_is_faster_than_metadata_probe_baseline`、`perf_walker_classification_is_faster_than_eager_metadata_resolution`、`perf_adaptive_walker_reports_local_dataset_metrics` を実行する
- lightweight PR perf gate: `.github/workflows/ci-cross-platform.yml` の linux-native job で `perf_filelist_stream_is_faster_than_metadata_probe_baseline` を実行し、line-only fast path の優位を 1.20x 下限で監視する
- GUI 手動試験: `scripts/gui-smoke-fixture.sh` 後に `cd rust && cargo run --bin flistwalker -- --root target/gui-smoke/root --limit 1000`
- GUI headful smoke: `scripts/gui-headful-smoke.sh --duration 10` または `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-headful-smoke.ps1 -DurationSeconds 10`
- GUI 手動試験: `cargo run --bin flistwalker -- --root .. --limit 1000` で新版検知ダイアログと更新承認導線を確認
- GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode SameVersion`
  Windows sandbox で同一 version の feed でも更新ダイアログ表示を確認する。helper は `SHA256SUMS.sig` を同時生成する。
- GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode Downgrade`
  Windows sandbox で旧 version feed を使った downgrade ダイアログ表示を確認する。helper は `SHA256SUMS.sig` を同時生成する。
- GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode Custom -FeedVersion 0.12.1`
  Windows sandbox で任意 version のローカル feed を生成し、署名付き manifest を使った update 手順を再現する。
- CLI 動作確認: `cargo run -- --cli "main" --root .. --limit 20`
