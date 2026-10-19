# Linux coverage fault evidence — 2026-10-19

The original [SIGSEGV investigation](../../releases/evidence/v0.31.0/LINUX-COVERAGE-DIAGNOSIS.md) remains **OPEN: cause unknown, original-failure stack unavailable**. The user explicitly accepted applying the diagnostic path while continuing that investigation. Synthetic fault capture and subsequent non-reproduction do not establish a causal product repair. The original v0.31.0 candidate remains blocked; no tag or release was created.

[PR190](https://github.com/ShinjiKawamura255/flist-walker/pull/190) source `1d93ed589dda141eceeefc6b0877e2be0c01f400`, base `3194f03c27f9a72f06a836dbd02e9141d8740a26`, passed all10ordinary jobs in [run39238474595](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39238474595). The [coverage job118013407175](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39238474595/job/118013407175) ran the original locked workspace command once, at default parallelism and the unchanged75% threshold. Actual checkout `1e5627f7ccf541af3f04a5d3969a8d75a59b7cb2` is the synthetic merge of that head into the fixed base; its tree equals the reviewed source. It is distinct from both the PR head and the later protected merge.

## Retained native results

- [Linux report](linux-report.json):27,484bytes, SHA256 `1f44fddca5b98683c18df055de04579bb3b444c01569f423f8794aa06f0b6d6a`; complete=true, coverage_exit=exit=0, errors empty, core_settings=RESTORED.
- Six native modes passed: normal exit, exit7, parent SIGSEGV, pthread worker SIGSEGV, direct-child SIGSEGV, and intentional child SIGABRT. The four synthetic cores plus the real TC165 intentional SIGABRT were all CAPTURED, fault-first and thread-complete. No original/unexpected SIGSEGV recurred.
- Worker fixture PID7448/fault LWP7449 and TC165 PID47467/fault LWP47470 distinguish the crashing thread from the leader. All5GDB runs exited0 without timeout/interruption/unreaped state. Private argument values, locals, environment dumps, raw GDB headers and raw cores were not published.
- Coverage library:1626PASS/19ignored/49.82seconds. LCOV43,547/50,918lines=85.5237833379%; source LCOV3,529,147bytes SHA256 `17f3f910cdbcb7090078ae24415aa8ab976c4cbf8d1dd2bce7c0d440adbe993d`. Detailed source/binary/host bindings are in [validation](linux-validation.json).
- Observed library binary:415,967,448bytes, SHA256 `f9843217db5e2e9c9a10d74839ce1505c407e4df9a392cdebba1d7623c28e5cb`. This differs from the earlier b216 binary after the test-only patch-leaf fixture correction; the original failed binary digest remains unknown.
- Hosted native tests/builds/clippy passed: Windows1647PASS/20ignored/130.35seconds, macOS1617PASS/19ignored/69.57seconds, Linux1626PASS/19ignored/47.72seconds. The patch-leaf case passed on all3OS. Python156tests and audit passed. These are exact-run observations, not a stability or timing guarantee.

## Warning and review boundaries

The [complete10-log inventory](ci-log-review.json) retains one Windows GNU E2E DEP0005 emission in pinned official download-artifact v8.0.2 extraction, zero product/Rust/test/clippy/audit warnings, and one macOS arm64 queue-capacity NOTICE. The separate Guardian failure is the expected4-path immutable-policy refusal.

Fresh official release/tag/action definition and the full4,432,922byte bundle SHA256 `7834b909df0fd656f5ac1c4c4ab4ee3b5e2673e4f4f763549d8abbca16bc5bbe` matched the [bounded full-Action diagnosis](../../releases/evidence/v0.30.1/DOWNLOAD-ARTIFACT-V8.0.2.md). Node24, runner2.337.0 and digest-mismatch=error were verified. The expected and computed GNU artifact ZIP digest `c33f4b48e0b89a31951dc1b165ba9401b60882dd4151284d0e39a3901c19cb81` matched; extraction and both Universal/Fw sandbox updates succeeded. An independent exact-run review accepted the limited minor external-warning assessment. The frozen inventory preserves its earlier pending-assessment wording; final review is retained with the control evidence. No hosted Buffer argument/stack reproduction or release warning exception is claimed.

Independent read-only review checked source identity, report/log hashes, fault/leader/thread completeness, privacy, CI and warning/annotation classification without additional findings. The collector's failure/cancellation, child-core identity, immutable helper boundary and GDB reap-timeout regressions retain their RED/GREEN evidence in PR190 and Git history.

The [CI Operations contract](../../CI_OPERATIONS.md#linux-coverage-fault-evidence) owns ongoing behavior. Resource limits, VM loss and SIGKILL can leave capture partial; PARTIAL/unknown evidence fails closed and does not hide an original coverage failure. Windows GUI/TC193, a replacement signed candidate and remaining release-specific native checks are separate gates.
