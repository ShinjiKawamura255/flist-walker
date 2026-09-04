# Validation Matrix and Runner Commands

## Regression Guards

Detailed defect-specific contracts are kept in [Regression Guards](regression-guards.md). Read them when a selected VM or touched test references the guarded behavior.

## Change-Type Checklist
Use this checklist before selecting runner commands. The VM table below remains the normative validation matrix; this section is an operator-friendly entrypoint for common change intents.

The `Typical Targets` in each VM detail are human guidance, not an exhaustive path allowlist. `scripts/validation-rules.json` owns mechanical routing, shared paths may select multiple VMs, and this checklist adds intent-dependent validation that filenames alone cannot infer.

### Docs-only or SDD/TDD Document Updates
- Apply: VM-001.
- Check that the touched docs keep `FR/NFR/CON -> SP -> DES -> TC` references intact when IDs are mentioned.
- Review the affected diff for obsolete assumptions, duplicated instructions, and local Markdown links.
- Run focused `rg` checks for renamed headings, IDs, and file references.
- Do not run `cargo test` when the diff is limited to docs and `AGENTS.md`; confirm that with `git diff --stat`.

### GUI Orchestration, Rendering, Input, Tabs, or Session Changes
- Apply: VM-002.
- Keep heavy I/O and long computation out of the egui frame path.
- Preserve request routing, stale response handling, tab/background response ownership, and the invariant that visible empty Results has no row while visible non-empty Results always has a valid current row.
- Add focused tests under `rust/src/app/tests/` that match the owner module touched.
- Run GUI smoke evidence when rendering, focus, tabs, dialogs, result drawing, or responsiveness changes.
- For tab ownership transfer, run TC-154 and TC-203 through TC-211 plus `tab_contract`, `tab_lifecycle`, `tab_result_cache`, `tab_background_responses`, `query_history`, `session_restore`, and `filelist_lifecycle`; cover non-sparse/sparse allocation identity, lifecycle+committed combinations, Query empty/non-empty × FileList/Walker, active-scratch stale-routing, live/closed LRU, meaningful/instantaneous active tenure, Recent Inactive grace/hard pressure, reclaimer pressure, and the release-mode transition fixture. Ready and protected Recent Inactive activation retain Results; Refreshing/Failed keeps last-good; Evicted reloads without synchronous compaction/drop.

### Bounded Worker Scheduling or Shutdown Changes
- Apply VM-002 to action/kind dispatch, worker bus, load accounting, runtime handle ownership, and shutdown changes; run focused TC-150, TC-151, and TC-153 tests in addition to the full Rust suite.
- For index scheduling, mailbox, pending queue eviction, or terminal settlement changes, include Warm promotion without duplication, deterministic single-victim A→B→C→A, data-lane Full with active/shutdown progress, Started/data/Truncated/terminal sequence, and stale terminal isolation. Scheduler cancel returns to Ready with committed data or Dormant without it; no activation-refresh marker compatibility path remains.
- Apply VM-003 as well when index dispatch, index coordinator, index worker, or stale-before-canonicalize behavior changes; run focused TC-152 and the VM-003 ignored FileList/Walker performance tests.
- Verify the fixed limits exactly: action 2 + 8 = 10, kind 1 + 256 = 257, index 2 workers with Active 1 + Warm at most 1, app pending <= 4/latest one per tab, mailbox data 8 per admitted request plus fixed control/terminal slots, snapshot cache count 2/weight 1,000,000, reclaimer queue 4.
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
- 100k search performance changes run TC-156 explicitly with `cargo test --release --locked perf_search_100k_cold_warm_query_shapes --lib -- --ignored --nocapture`; record median/maximum/evaluated candidates including unknown-kind `ext:` and validate `.github/workflows/perf-regression.yml`.

### CLI, Build, Release, Updater, or OSS Packaging Changes
- Apply: VM-005.
- CLI/TUI contract changes run TC-163 through TC-166 and applicable newer CLI contracts such as TC-170 focused before the full suite; external actions use recording backends and FileList writes use temporary roots only.
- Run the project-local release preflight skill before tag/release/publish work.
- Update `docs/RELEASE.md`, `.github/release-template.md`, OSS notices, and asset sidecar handling together when packaging changes.
- Check release asset names, target OS coverage, update manifest/security behavior, and workflow warning gates.
- Updater staging 変更では TC-157 を failing-first で追加し、trust-first request order、strict manifest、byte/time/redirect bounds、streaming hash、partial cleanup を focused 実行する。
- Updater activation/recovery 変更では TC-158 と TC-159 を failing-first で追加し、Windows/Linux 両方の TC-160 inert dummy filesystem 証跡を必須とする。Windows replacement/process 境界の変更では TC-171 を native Windows host で focused 実行し、PowerShell 非依存、backup 境界、UTF-16 path を確認する。実行中 binary の置換または外部 application 起動を行わない。
- Keep macOS notarization status wording in release notes while the temporary non-notarized publish posture remains active.

### CI Coverage, GUI Validation Docs, or Smoke Script Changes
- Apply: VM-006.
- Validate shell/PowerShell scripts with the parser checks listed in VM-006.
- Keep GUI test plan IDs, report template fields, smoke script names, and workflow references synchronized.
- Treat coverage threshold changes as quality-policy changes that require fresh baseline measurement and docs updates.

### CI Reliability, Version Pins, Security Audit, or Merge Policy Changes
- Apply: VM-009 in addition to VM-005/VM-006 when their release or coverage surfaces are affected.
- Run `python -m unittest scripts.tests.test_check_ci_policy` and parse every `.github/workflows/*.yml` file.
- Verify required workflows use numbered runner generations, Rust/tool versions, full Action SHAs, least permissions, timeout/concurrency, image-version evidence, and download-only Cargo caches; read-only trusted-base guardian以外の`pull_request_target`は禁止する。
- Exercise TC-056/TC-056B negative cases: audit-relevant pathのskipped auditは失敗し、非audit pathだけskipped auditを許容する。heavy CIはallowlisted documentation `A`/`M`だけ全対象jobの`skipped`を許容し、Rust/scripts/workflow/policy、rename/delete、unknown path、base SHA不明、diff失敗では全対象jobの`success`を要求する。GNU E2Eの`needs`が専用GNU producerとchange detectionだけであることを確認する。
- Review `CI Gate` aggregation, scheduled audit/canary issue tracking, exact Dependabot rebase auto-merge registration, and the pin promotion/rollback rules in `docs/CI_OPERATIONS.md`. For local rebase lifecycle changes, use a disposable Git repository to verify that a clean `master == origin/master` can start a feature branch and a rebase-equivalent, PR-identified branch is eligible for the constrained cleanup. Verify that dirty state, divergent master, PR identity mismatch, patch difference, feature-branch merge commit, `master` target, and worktree use stop the operation.
- Repository setting changes require complete before/after API read-back and a protected two-commit以上のmachine PR that rebase auto-merges without admin bypass. Verify rebase-only, linear history, merged-branch auto-delete, both required checks, commit boundary/order/message/author/patch-tree correspondence, one parent per added commit, and no merge commit.

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

### Stateful Endurance Harness or Scheduled Soak Changes
- Apply: VM-010. Production app orchestrationを変更した場合は VM-002、index dispatch/worker pathを変更した場合は VM-003、workflow/policyを変更した場合は VM-009も併用する。
- deterministic profile は sleep、外部 action、updater、network、利用者設定へ依存させず、失敗に seed/step/event/state digest/replay command を含める。
- fixed corpus、seeded profile、invariant self-test、quiescence、ignored extended/real-worker profileを対象に応じて実行する。

## Runner and commands
- Runner: `cargo test`
- Runner: `cargo test`, `cargo audit`
- Validation Matrix:
| ID | Detail | Primary surface |
| --- | --- | --- |
| VM-001 | [Docs only](validation/vm-001.md) | Docs/SDD/TDD text |
| VM-002 | [App/UI orchestration](validation/vm-002.md) | GUI/app orchestration |
| VM-003 | [Indexing path](validation/vm-003.md) | FileList/walker/indexing |
| VM-004 | [Search/query contract](validation/vm-004.md) | Query/search/highlight |
| VM-005 | [CLI / build / release / updater](validation/vm-005.md) | CLI/build/release/updater |
| VM-006 | [CI coverage gate / GUI validation docs](validation/vm-006.md) | Coverage/GUI evidence tooling |
| VM-007 | [Supportability docs/templates](validation/vm-007.md) | Support docs/templates |
| VM-008 | [Runtime config bootstrap](validation/vm-008.md) | Runtime config/bootstrap |
| VM-009 | [CI reliability / pins / merge policy](validation/vm-009.md) | CI/merge/worktree policy |
| VM-010 | [Stateful endurance](validation/vm-010.md) | Stateful endurance |
- 大規模 docs cleanup や plan 撤去のような docs-only 変更では、doc diff review と `rg` 参照整合確認を必須にする。Rust 実装に触れない限り `cargo test` は不要だが、変更対象が docs と `AGENTS.md` に限定されることを `git diff --stat` でも確認する。
- app architecture のような構造改善後も、恒久的な検証基準は VM-001 / VM-002 / VM-003 を直接適用する。
- `ui_model/` は display/highlight/preview concern に限定し、action decision は `actions.rs` 側の unit test と `TC-107` で固定する。
- Commands:
- `cd rust`
- `source ~/.cargo/env`
- `cargo test`
- release 前 warning gate: localでは`cargo clippy --locked --all-targets -- -D warnings`を実行し、heavy PR CIではmacOS/Windows native job、tag workflowではLinux/macOS/Windows nativeの全preflight jobが同じlocked clippyを実行すること、release asset build logsにwarningが残っていないことを確認する（TC-198）
- `cargo audit`
- Windows release ZIP regression: `powershell -ExecutionPolicy Bypass -File .\scripts\test-prepare-release-archive.ps1 -ArchivePath .\dist\vX.Y.Z\FlistWalker-X.Y.Z-windows-x86_64.zip`
- audit warning posture: `docs/OSS_COMPLIANCE.md` の accepted transitive warning を確認し、release candidate ごとに `cd rust && cargo audit` を再実行する
- coverage gate: first create `target/llvm-cov` (`mkdir -p target/llvm-cov` on Unix or `New-Item -ItemType Directory -Force target/llvm-cov` in PowerShell), then run `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75`
- coverage uplift target: 80% は release 直前の義務ではなく中期品質目標として扱う。80% へ上げる前に app/GUI owner seam の不足領域を追加 test で補強し、fresh baseline を再測定する。
- heavy perf regression workflow: `.github/workflows/perf-regression.yml` の manual dispatch または weekly schedule で TC-156/TC-185 共通の `perf_search_100k_cold_warm_query_shapes`、`perf_filelist_stream_is_faster_than_metadata_probe_baseline`、`perf_walker_classification_is_faster_than_eager_metadata_resolution`、`perf_adaptive_walker_reports_local_dataset_metrics` を実行する
- lightweight PR perf gate: `.github/workflows/ci-cross-platform.yml` の linux-native job で `perf_filelist_stream_is_faster_than_metadata_probe_baseline` を実行し、line-only fast path の優位を 1.20x 下限で監視する
- GUI 手動試験（生成 fixture / isolated staged executable）: 操作時間を確保して `scripts/gui-headful-smoke.sh --duration 300`、または `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-headful-smoke.ps1 -DurationSeconds 300` を実行し、スクリプトが表示・記録する staged window だけを操作する。workspace debug executable を直接起動しない。
- GUI headful smoke: `scripts/gui-headful-smoke.sh --duration 10` または `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-headful-smoke.ps1 -DurationSeconds 10`
- Windows scripted query smoke: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-headful-smoke.ps1 -DurationSeconds 10 -ScriptedQueryProbe`
- GUI deterministic scenarios: `scripts/gui-deterministic-scenarios.sh` または `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-deterministic-scenarios.ps1`
- Stateful endurance required profile: `cd rust && cargo test --locked stateful_endurance --lib`
- Stateful endurance extended profile: `cd rust && cargo test --locked tc_184_stateful_endurance_extended --lib -- --ignored --nocapture`
- Stateful endurance single-seed replay: `cd rust && FLISTWALKER_ENDURANCE_SEED=<seed> cargo test --locked stateful_endurance_replay --lib -- --ignored --nocapture`
- Stateful endurance real-worker soak: `cd rust && FLISTWALKER_ENDURANCE_SOAK_SECONDS=10 cargo test --locked tc_184_stateful_endurance_real_worker_soak --lib -- --ignored --nocapture`（closure では 10 秒、scheduled workflow では既定 1200 秒）
- VM-005 self-update 手動試験は通常の GUI smoke / closure validation では実行しない。`scripts/manual-self-update-test.ps1` が作る private sandbox の copied executable と loopback inert feed だけを対象にし、production executable、production feed、外部 network endpoint を指定しない。`Download and Restart` は明示承認がある場合だけ実行する。
- VM-005 GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode SameVersion`
  Windows sandbox で同一 version の feed でも更新ダイアログ表示を確認する。helper は `SHA256SUMS.sig` を同時生成する。
- VM-005 GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode Downgrade`
  Windows sandbox で旧 version feed を使った downgrade ダイアログ表示を確認する。helper は `SHA256SUMS.sig` を同時生成する。
- VM-005 GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode Custom -FeedVersion 0.12.1`
  Windows sandbox で任意 version のローカル feed を生成し、署名付き manifest を使った update 手順を再現する。
- CLI 動作確認: `cargo run -- --cli "main" --root .. --limit 20`
- CLI/TUI 契約変更では `cargo test --test cli_contract` と `cargo test cli_tui::tests` を先行し、`cargo test` を再実行する。terminal evidence は stdout pipe、stdin/stderr non-TTY rejection、partial setup failure、draw/read error restoration、resize/paste、cleanup-before-output を含める。pseudo-TTY で自動化できない項目は Windows 実端末で必須記録とする。
