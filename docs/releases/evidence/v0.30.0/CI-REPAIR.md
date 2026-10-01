# v0.30.0 Preparation CI Regression Repair

Date: 2026-10-01 UTC. Draft PR [#160](https://github.com/ShinjiKawamura255/flist-walker/pull/160), initial head `d860896cd425222443943114843da2ea924b59eb`, base `48798929a73cc49411c5d7c50265efd743c33e3c`. Initial CI [36885118875](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/36885118875) failed; it is retained as failed evidence. CI Policy Guardian passed. No merge, candidate, tag or publication was performed on that head.

## H-03: time slice and count budget were conflated
Medium severity, high confidence; a test reliability defect. TC-151 asserted exactly 512 paths after the default 4 ms poll, although the production contract allows the time slice to yield earlier. Actual counts were Linux 509, Mac 240, Windows 449 and coverage 151. The deterministic zero-budget probe reproduced the invalid throughput expectation (expected 512, actual 0).

The test now first verifies zero-budget progress is zero and the last-good results remain unchanged, then uses an explicit one-second test budget to verify the unchanged 512-path admission and 4096-path backlog bounds. Production 4 ms, 512 and 4096 limits are unchanged. Focused TC-151 red/green and the group passed. The repair changes only test budget control; it does not excuse slow production frames.

## H-04: autonomous frame-start retries lacked exact owners
Medium severity, high confidence for retained background finish and a deterministically proven activation retry path; a harness ownership defect. Mac seed `0x1839`, step 123, reported an inactive tab legitimately completing its retained terminal work before the selected response. A subsequent local full run at seed `0x183e`, step 125, observed the active waiting-for-reclamation notice. Its old trace omitted the pending activation ID; the historical occurrence alone does not conclusively identify that path.

An independent read-only diagnosis traced `prepare_frame` to `retry_pending_background_index_finish` and `retry_pending_tab_activation`. Both run before the selected incoming response. The harness previously admitted submitted/deferred response owners but omitted these pre-existing retry debts. The repair adds only the first inactive pending finish whose request matches its tab state and request route, plus the current and still-existing target tab of a pending activation. Other in-flight routes and closed targets acquire no ownership. Failure diagnostics now include pending activation identity.

Two failing-first guards proved the omissions before the owner repair. The activation guard additionally fills and pauses the bounded reclaimer, gives the target a superseded 1024-entry build, and invokes the actual frame-start poll: activation remains pending and production emits the waiting notice. It then verifies unrelated tabs stay isolated. Negative assertions still reject third-tab digest mutation, a waiting notice without pending activation, in-flight routing without retained terminal debt, and a closed target. No blanket notice/root exemption or production code change was added. TC-183 traceability is updated.

## Validation and release posture
Final test/source identities and local metrics are recorded in LOCAL-VALIDATION and the fresh independent review. Earlier local/CI failures remain part of the evidence; rerunning an unchanged failing head was not used as remediation. Existing performance results apply to the unchanged production implementation. The updated exact PR head must pass required platform CI before merge. Native GUI, signed candidate/actual N-1, tagged build and publication remain NOT RUN.

Raw red/green, original CI, final full/coverage/endurance and seed replay logs remain Git-ignored under target. This committed record preserves the failure, causal boundary, narrow repair and limits without personal fixture paths.
