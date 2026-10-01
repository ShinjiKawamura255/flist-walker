# v0.30.0 Preparation Local Validation
Date: 2026-10-01 UTC. Environment: Tsukuyomi macOS arm64, Rust 1.97.1.
Source: completed GUI fix `2138076` plus the staged v0.30.0 preparation changes. Final source snapshot identity is recorded in the independent review and preparation PR. Only Cargo root version/lock metadata changed after the imported Rust implementation; no production Rust logic was subsequently edited.

## Results
All commands below completed with exit 0 unless explicitly marked unavailable.
| Check | Result / command and measured evidence |
|---|---|
| Required validation routing | `python3 scripts/validate_change.py --base origin/master --plan`: VM-001/002/005/008/009; additionally apply indexing/search/endurance intent checks because the full release range touches those boundaries. |
| N-1 regression | Added exact 26-entry 0.29.0→0.30.0 case reproduced `unsupported previous release capability: 0.29.0` before registration; `python3 scripts/test-updater-n-minus-one-compatibility.py` passes after registration. Actual signed candidate manifest remains pending. |
| Full suite | `cargo test --locked`: 1,520 passed, 0 failed; includes 1 native-menu contract, 1,459 lib, 8 architecture, 3 CLI integration, 47 CLI contract, 2 path contract. 15 lib performance/endurance tests remain normally ignored and selected ones were executed explicitly below. Host execution permits existing Unix socket tests. |
| Formatting / warnings | `cargo fmt --check`; `cargo clippy --locked --all-targets -- -D warnings`: PASS. |
| Coverage | `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75`: PASS. LCOV sum LH/LF = 41,919/49,145 (85.30%). |
| Audit | `cargo audit --file Cargo.lock --json`: 0 vulnerabilities, no warnings. RustSec DB commit 3461c0d8f85d084552dd999c58d97c7123a9e0fd, 1,278 advisories, updated 2026-10-01. |
| Repo/agent tooling | `validate_change.py --quick`: repository contract + 62 scripts tests PASS; `check_ci_policy.py --guardian .` PASS; all 8 workflow YAML files parse with temporary PyYAML 6.0.3. No workflow/trusted checker/settings changed; controlled rollout and mutation/proof checks are inapplicable. |
| GUI deterministic | `bash scripts/gui-deterministic-scenarios.sh`: all 14 canonical groups PASS, includes focused TC-150/151/152/153/154, rendered surfaces, preview/settings and stale/background routing. Native rendering/input are not implied. |
| Focused owners | `cargo test --locked <filter> --lib --` PASS for tc_167_, tc_168_, tab_contract, tab_lifecycle, tab_result_cache, session_restore, filelist_lifecycle, run_ui_frame. Prior canonical groups cover rendered and background routing. |
| Endurance | required `stateful_endurance`, ignored `tc_184_stateful_endurance_extended` (256 seeds × 1000 steps), and `FLISTWALKER_ENDURANCE_SOAK_SECONDS=10 ... tc_184_stateful_endurance_real_worker_soak -- --ignored --nocapture`: PASS; real-worker 5,415 iterations and settled route/load state. Temporary fixture only, no external action/updater. |
| Release helper syntax | `bash -n scripts/validate-release-bundle.sh scripts/test-validate-release-bundle.sh`: PASS. |
| Bundle self-test on Mac | **NOT RUN to completion / environment failure**: Bash 3.2 lacks mapfile and BSD find lacks -printf. Initial run failed; no shim, gate weakening, or PASS claim. Existing Linux candidate/tag CI must run this self-test and real bundle validator. |
| Four-target OSS | locked metadata resolves Windows GNU 292, Linux 328, macOS x86_64 304, arm64 303 packages. Only semver/serde_json patch versions changed, no package names added/removed. Both MIT OR Apache-2.0, local registry license files checked; notices updated. |
| Native/platform/publication | **NOT RUN**: exact signed candidate, real native GUI/input/IME/Windows/manual update and public download readback are later gates. |

## Performance
Each command used `cargo test --release --locked <filter> --lib -- --ignored --nocapture` on the final source, sequentially after other heavy checks.
- `perf_live_query_input_to_dispatch_100k`: all three modes × 5 samples PASS. 100k median 68.718/68.814/68.797 ms; maxima 84.962/68.872/69.879 ms. Measures input event to accepted search request, not search-result completion or native OS delivery. 500k observational medians 498.265/339.392/357.281 ms; maxima 536.322/359.115/359.950 ms. Per-poll budget about 4 ms; ingestion pauses until preparation finishes. Residual disclosed rather than treated as a hard-gate failure.
- `perf_search_100k_cold_warm_query_shapes`: all 11 shapes PASS; 100k medians/maxima 3–12 ms, including unknown-kind ext. TC-185 1m selective p50/p95/p99 116/127/127 ms; dense 162/166/166 ms (7 samples). RSS before/after fixture/peak/after drop = 47,939,584 / 144,244,736 / 455,032,832 / 173,555,712 bytes. Observational, no invented hard RSS bound.
- `perf_filelist_stream_is_faster_than_metadata_probe_baseline`: PASS, 10.32×, 30,000 entries.
- `perf_walker_classification_is_faster_than_eager_metadata_resolution`: PASS, 7.76×, 33,025 entries.

## Evidence durability
Raw logs and screenshots stay Git-ignored under target. This sanitized committed record and exact future PR/run/tag records are durable evidence. No prior v0.29.0 waiver is reused. Local PASS does not assert exact remote candidate/tag/CI PASS.

## Post-CI repair final validation
The initial PR head `d860896` failed platform CI; [CI-REPAIR.md](CI-REPAIR.md) records the two narrow test/harness repairs. These are additional results on the final post-repair source, not a relabeling of that failed run. No production implementation, Cargo dependency or performance limit changed.

- Required routing now also selects VM-010 for the endurance harness; its intent/detail checklist was applied.
- `cargo test --locked`: 1,522 passed (1 menu + 1,461 lib + 8 architecture + 3 CLI integration + 47 CLI contract + 2 path), 0 failed; 15 normally ignored lib profiles retained.
- `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings`: PASS.
- Extended endurance: 256 seeds × 1000 steps PASS. Real-worker soak: 10 seconds, 5,438 iterations, settled routes/load state PASS.
- `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75`: PASS; 41,954/49,145 lines = 85.37%.
- Two exact-debt regression guards: failing-first before owner attribution, then PASS; the final activation fixture invokes actual paused/full-reclaimer retry. Seed replays 0x1839 and 0x183e (128 steps each), stateful suite, final canonical GUI 14 groups and repository quick checks PASS.
- The earlier audit, four-target OSS, syntax and four performance results remain applicable to the unchanged production/dependency inputs. Exact updated PR CI and candidate/tag/native/publication gates remain pending. The Mac bundle environment failure remains unresolved locally and requires the existing Linux gates.

Raw final logs: target/v0300-health-release/owner-final-{0..5}.log, owner-actual-retry-green.log, owner-repair-replay-{0x1839,0x183e}.log, owner-final-gui.log and owner-final-quick.log (Git-ignored).
