# TC-167 persistence flush diagnostic protocol

Back to [Test Cases](test-cases.md). This is observational evidence for SP-010/SP-016 history persistence. It does not fix the reported Windows failure or change a product contract.

The existing history-burst, history-cap and history-disabled tests opt in before worker spawn. Their original flush receive timeout remains1second, file-lock budgets and gate/order/content assertions remain unchanged. Production code does not compile the diagnostic. Other writers, including contention/performance collectors, do not opt in. No workflow, dependency, feature, retry or required-check policy changes.

`TC167_FLUSH_DIAGNOSTIC` JSON lines are serialized into one buffer and written directly to stderr after flush/cleanup so successful default-capture `cargo test --locked` retains them. Decode schema `tc167-flush-diagnostic-v1` from trace. Labels identify the3 tests; process ID and available_parallelism are observations, not a guarantee of actual libtest concurrency. RUST_TEST_THREADS env may be absent; record exact CI argv/image/tool/source separately.

Each trace is limited to96 events. Recording uses one try_lock and never waits for diagnostic contention; overflow, unavailable snapshots and dropped events are explicit. Missing/incomplete events are unknown intervals, never zero or evidence of correctness. The snapshot can precede a concurrent worker event, and does not promise that the lifetime has ended. Physical is_finished and explicit completed join evidence are separate from observational phase/worker-return/unwind labels. Status reads also use try_lock and may be absent; accepted/persisted generations, outstanding writes and error/protection flags may advance between reads. No path, query or document data is logged.

Per-writer Instant clock starts at opt-in construction. Relevant points are spawn/worker entry, admitted/dequeued generations, status-lock read, gate lookup/wait, flush send/receive, sidecar acquisition, document read/merge, atomic write (including sync/replace), publication and reply. Sidecar acquisition includes directory/open/OS-lock work; gate includes existing lookup plus any barrier wait. Event append order across threads is not a total causal order: use at_ns and match named begin/end points. Worker receive/reply can precede caller-side return markers. No diagnostic duration is an SLO.

A fixed-size last_receive field records actual receive outcome and begin/end/elapsed independently of event capacity; it cannot be lost merely because the event trace filled. It is observational for the single caller in each targeted test, not a concurrent-flush API. Event loss still leaves detailed intervals unknown.

`flush-recv-Timeout` and `flush-recv-Disconnected` come from the actual recv_timeout error enum. `flush-recv-WorkerError` means a reply was received with a write error; `flush-send-error` means admission failed before receive. The public returned strings remain unchanged, including the legacy generic timeout message for Timeout/Disconnected. A worker lifetime guard observes unwinding without catching/swallowing it. Physical join is emitted only after the existing successful shutdown or exact semantic-fixture ownership check.

Controls exercise actual flush_sender with a real writer held at its pre-I/O gate (Timeout), and a responder that drops its reply sender (Disconnected); both preserve the legacy returned error. A bounded/off control covers96-event overflow and an ordinary writer with no trace. These controlled failures are not reproduction of the hosted incident.

Local focused commands from rust/:

```sh
cargo test --locked --lib tc_167
cargo test --locked --lib tc_168
```

Required ordinary regressions follow VM-008; source-equivalent Windows CI is a separate axis. Inspect the whole exact run and the original3 labels even if its other jobs fail. First delivery uses one normal draft PR CI. Any retry must have a recorded hypothesis and comparison condition; a green run without reproduction leaves the original root cause unresolved. Do not extend timeouts, serialize the entire suite, rerun until green, mix B0 timing data, or merge the diagnostic draft under this task.
