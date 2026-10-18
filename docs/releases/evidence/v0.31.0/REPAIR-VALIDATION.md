# Candidate preparation repairs (2026-10-18)

## Scope and original evidence

Original source3194f03 / candidate run39087986481 attempt1 / artifact12122471715 remains unchanged and **BLOCKED**. Evidence117e9f048bba7195869293175615de8902ea440e and the original Windows ZIP are retained. The repair PR includes that task-owned evidence followed by the fixes; it does not regenerate or promote the candidate.

Independent read-only review of the original packet, complete candidate ZIP and Windows ZIP confirmed the recorded distribution/signature/N-1 results. Its only new finding was minor: `subprocess.run` timeout bypassed raw observation recording. The full report was returned in the task conversation; the repair and final independent review are recorded in the repair PR. Known compiler/warning/native blockers were not counted as defects introduced by the evidence packet.

## Corrections and regression evidence

| Contract | Failure before repair | Repair and focused guard |
| --- | --- | --- |
| Windows helper retains the first failed subprocess observation | `TimeoutExpired` held partial stdout/stderr, but the raw file held only42 prior version/alias rows; the timed-out process had no row | Catch timeout inside `run`, record role/argv/elapsed/unknown exit/timeout seconds/partial stdout+stderr and stdout digest, then propagate to the existing overall FAIL path. Test partial-output and no-output cases:43 rows, previous observations preserved, no timing PASS. |
| macOS build uses the repository Rust pin | Explicit `rustup run stable cargo` bypassed the1.97.1 pin; cargo-only fallback could also use an unrelated compiler | Read fixed X.Y.Z from `rust/rust-toolchain.toml`, explicitly select it with rustup, and fail if the pin/rustup/toolchain is unavailable. Test implicit/explicit target, inherited stable environment, different fixture pin, mutable/empty/malformed pin, missing rustup and failed fixed-toolchain invocation without fallback. |

`python3 -B -m unittest scripts.tests.test_build_rust_macos scripts.tests.test_windows_candidate_helper` first ran10 tests with8 failures (including subtests) against the original implementations, then passed all10 after repair. The timeout guard mocks the child process only; it does not execute a Windows binary or simulate a native timing PASS. Helper tests retain the5warmup+25samples/variant and0.70 ratio contracts, output parity, partial failure, native-OS guard and refusal to overwrite existing output/raw.

| Executed check | Result |
| --- | --- |
| `python3 -B -m unittest discover -s scripts/tests` | PASS133 tests; includes the10 focused repair tests and existing policy/negative/lifecycle/benchmark guards |
| `python3 -B scripts/check_repo_contract.py` | PASS |
| `python3 -B scripts/check_ci_policy.py --guardian .` | PASS; no trusted workflow/checker/settings edits |
| PyYAML6.0.3 parse of all8 workflow YAML files | PASS; parser installed only into a temporary test directory |
| `bash -n scripts/build-rust-macos.sh` | PASS |
| `cd rust && cargo fmt --check` | PASS |
| `cd rust && rustup run 1.97.1 cargo audit` | PASS;1338 advisories /452 locked dependencies, no vulnerability/warning output |
| `bash scripts/build-rust-macos.sh` | PASS; actual local macOS arm64 release build, both binaries contain only the required Rust compiler commit |
| `bash scripts/build-rust-macos.sh aarch64-apple-darwin` | PASS; explicit-target release build, same compiler identity |

Required compiler: Rust1.97.1, `/rustc/8bab26f4f68e0e26f0bb7960be334d5b520ea452/`. Both actual build logs completed without warnings. These local development builds are not signed replacement candidate assets.

| Target selection / binary | Bytes | SHA256 |
| --- | --- | --- |
| implicit / flistwalker |13414368|433dbbc1a039ce03b28bdbdf077925ccc10b632fa54b59a3c842b6efc307d2f8|
| implicit / fw |5028512|4d2cb002ceb1dfe89c41591f47407e871e96a81ecf6aadd75ee96eb9296a6d9d|
| explicit aarch64 / flistwalker |13414704|58222fae361d1cdd7ce55b03df1ce72cc1d3651aed6e02d99217f3677316f2e7|
| explicit aarch64 / fw |5028576|90523864e52563e9371bfe9fd39d313235fd489ae1d27deebe25a44a0d6ddb0d|

## Windows handoff revision

The original `FlistWalker-v0.31.0-run39087986481-Windows-handoff.zip` and [its identity](windows/HANDOFF-ARCHIVE.json) are preserved. Revision `-r2.zip` contains the repaired helper and updated preparation record; its8 candidate files remain byte-identical to the original verified bundle. [Separate r2 identity](windows/HANDOFF-ARCHIVE-r2.json) binds its digest, size and source/run. This is a preparation-tool revision, not a rebuilt release. Historical compiler/warning/OS-signature/NOT RUN evidence still describes the original candidate.

## Validation selection and remaining boundaries

The repair diff selects VM-001/005/009. Product Rust, Cargo files/features/dependencies, workflow structure, updater trust/format, distribution asset names, archive/license/notice contents and native GUI behavior are unchanged. No new FR/SP/DES/TC contract or OSS notice update is needed; `docs/RELEASE.md` explains the existing compiler-pin contract at the corrected build entrypoint. Full product tests/clippy/coverage and Windows GNU updater E2E are supplied by the repair PR's fresh required CI; equivalent unchanged-Rust local runs are not duplicated. The merge decision requires final independent review and all applicable CI success.

Windows native GUI/TC-193, macOS real OS-theme notification, replacement signed candidate generation/validation and release readiness remain **NOT RUN/open**. Actual Intel release compilation will be verified by a future protected-source candidate; explicit x86_64 forwarding is covered by the focused shell test here. Normal CI tool-install warnings and external Action warnings remain separate unresolved release work. This repair does not grant an exception, weaken a threshold, change CI settings, or create tags/releases.
