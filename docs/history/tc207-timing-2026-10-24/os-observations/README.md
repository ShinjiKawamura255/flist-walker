# TC-207: OS-state observations after v0.31.0

The original 100ms failures remain **unexplained**. No production Rust, test assertion, 1ms/2,048-entry work budget, dependency, CI or published release changed. This adds actual macOS thread-state/stack observations, WSL2 owned-task context-switch events, and procedures for the still-missing full switch/wakeup trace.

## Identity and preserved data

Base is `f5e20628b4da774bc640ecc1f5351c7179849ed8`, with the uncommitted diagnostic collector only. The exact libtest executable was resolved from `cargo test --locked --offline --lib --no-run --message-format=json`, not selected by modification time. Native macOS27.0.1/arm64, Darwin27.0.0/xnu13432.1.9,12 logical CPUs, pinned Rust1.97.1, debug test profile. Binary SHA-256, relevant Rust/dependency/helper hashes, clock domain, command arguments and post-collection provenance addendum are retained.

[CAPTURE.json](CAPTURE.json) binds [capture-v1.tar.gz](capture-v1.tar.gz) (130 files) and [ANALYSIS.json](ANALYSIS.json). The archive includes every raw frame, every state query/error, complete test stdout/stderr, receipts, lifecycle records, sample outputs, host probes and an inner SHA-256 inventory. It contains no executable, private key or unrelated process capture. Only owned test/profiler processes were targeted.

The used collector hash is `cef702651e228501775b0058ea97bd6ecb7ff9194f633be5f4e475e334826ab0`, frozen as `collector-used.py` inside the archive. Independent review subsequently found missing nonzero exit on invalid experiment, loss of partial states/lifecycle on exceptional cleanup, and a spawn-to-startup guard gap. The current [observe.py](observe.py) fixes those paths and bounds stored samples; it is **not** relabeled as the collector that made the existing observations. All17 captured processes actually satisfied the original validation predicates. No replacement cohort was collected after review.

## Observations

The campaign was declared before execution: CPU/sleep/SIGSTOP controls once each, one unobserved serial process, one observed serial process, then12 observed parallel processes. Each natural process ran four exact100k-entry tests, with one libtest thread. No retries and no concurrent Cargo validation were introduced. Host background activity was not controlled, so these cohorts do not establish a causal overhead or capacity comparison.

| Condition | Cases / frames | Maximum wall ms | Same-frame CPU upper ms | Same-frame off-CPU lower ms | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| Unobserved serial |4 /493 |1.015666 |1.015875 |0 | all strict gates PASS |
| OS-state observed serial |4 /495 |1.021458 |0.989916 |0.031542 | all strict gates PASS |
| OS-state observed12 processes |48 /8,516 |14.101166 |0.295375 |13.805791 | all strict gates PASS |
|120ms CPU control, also invoking sample |1 /1 |127.720500 |120.094125 |7.626375 | **FAIL**, exit101 |
|150ms sleep control, also invoking sample |1 /1 |150.379125 |0.097333 |150.281792 | **FAIL**, exit101 |
|Owned-child SIGSTOP control |1 /1 |151.227667 |0.117750 |151.109917 | **FAIL**, exit101 |

There are56 natural cases /9,504 frames and zero natural100ms failures. Controlled failures verify that CPU work and waiting still fail the unchanged wall gate; they do not reproduce the original defect.

The worst natural frame is Walker kind-queue request2/frame164, PID83894/native TID25263003. `kind_cursor` advanced47,221→47,340; all100k entries were already collected. Its bracket overhead was0.001001ms. Nine exact-TID queries fit wholly inside its clock window after removing the entire overhead at both ends: all returned state1 (`TH_STATE_RUNNING`). Their cumulative CPU counter did not advance between sampled points. State1 includes **on-core or runnable**, and counters have finite update/clock resolution. This is consistent with non-CPU elongation without sampled blocked state; it does **not** measure continuous ready-to-run duration, exclude waits between samples or establish the original100ms cause.

The matched sleep-control TID25261629 had118 state3 snapshots and44 aggregate stack samples through `diagnostic_control → sleep → nanosleep`. This establishes a blocking mechanism for the **known control**. Sample's aggregate call graph has no per-stack timestamp, so it is not a separate frame-local event timeline. The CPU control had93 state1 and4 state3 snapshots and CPU lower bound above100ms, but its sample output had an empty call graph despite profiler exit0: CPU stack capture is **NOT CAPTURED**, with the original empty output retained.

The SIGSTOP control had118 state3 snapshots, while the parent independently observed `waitpid/WIFSTOPPED/SIGSTOP`. Thus state3 alone also cannot distinguish sleep, process suspension, locks or I/O. Its `resumed_ns` was sampled after sending SIGCONT; that observer span is not labeled an exact stopped-time lower bound. Collection source withheld SIGCONT until at least150ms after the stop observation.

## What the observer does and does not measure

The collector polls only its own child's threads, nominally every1ms. It records both ends of each OS query using explicit `CLOCK_MONOTONIC`, matching the existing frame clock. PID, native TID and a fully contained query interval must all match; a conservative full-bracket margin is removed at each edge. Failed queries never become zero CPU or a scheduling verdict.128 recorded errors were ESRCH query races (123 thread-info,5 enumeration); those intervals have unavailable data. Sampling gaps and observer contention remain visible limits.

macOS flavor15/`proc_threadinfo` is SDK-defined. Diagnostic enumeration flavor28 is private and therefore not a portable product interface. The collector validates112-byte ABI/return lengths and its own `pthread_threadid_np` against enumeration before observing another process. Unsupported enumeration fails admission. [Apple's declarations](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/proc_info_private.h) identify the private flavor; [state translation](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/thread.c) maps `TH_RUN` to state1. The source links explain the interface; they are not proof that every source revision equals this running kernel. Controlled live observations and local SDK definitions are retained separately.

These are state **snapshots**, not context-switch events. No durations are integrated from sample counts. Whole-process `ri_runnable_time` was deliberately not attributed to a test thread because application workers also run. The observer, and especially stack sampling, can perturb scheduling; profiler-collected controls remain separate from natural observations.

The production kind pass checks the1ms wall budget and2,048-entry cap between iterations (`index_finalization.rs::advance`, called by `tabs.rs::advance_request_owned_index_finalization`). Those checks cannot prevent an OS pause within an iteration. This is a possible mechanism for elapsed-time extension, not a causal diagnosis of the old failure or a reason to weaken100ms.

## Continuous OS trace: current prerequisites

| Probe / axis | Result |
| --- | --- |
|Active developer directory |`/Library/Developer/CommandLineTools`; no Xcode app found in standard `/Applications` |
|`xcrun xctrace list templates` on host |exit72, utility unavailable in selected developer directory |
|`ktrace info`, sandbox and host |exit1, `Too many levels of remote in path`; underlying cause not established |
|`sudo -n -l` on host |exit1, password required; no privilege acquisition attempted |
|SIP |enabled; unchanged; not asserted to explain ktrace's error |
|Process-scoped `/usr/bin/sample` |executed for CPU/sleep controls; sleep stacks captured, CPU call graph empty |
|System Trace / context-switch / wakeup events |**NOT RUN** |

No install, global developer-directory change, SIP/privacy change, root tracing or CI dispatch was performed. The shipped release's dated NOT RUN statement remains historical truth.

## macOS event-trace execution recipe — NOT RUN here

Use an authorized Xcode-equipped Mac and the same source/toolchain/profile. Use a fresh owned output directory and the exact binary resolved by Cargo JSON. Select Xcode for this command only; do not change global `xcode-select` or privacy settings implicitly.

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun xctrace list templates
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun xctrace list instruments
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun xctrace help record
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun xctrace record \
  --template 'System Trace' --time-limit 30s \
  --output "$TRACE_DIR/tc207.trace" --target-stdout "$TRACE_DIR/test-output.txt" \
  --env FLISTWALKER_TEST_TC207_TRACE=1 \
  --launch -- "$TEST_BINARY" --exact \
  app::tests::tab_background_responses::tc_207_background_finished_walker_kind_queue_settles_incrementally \
  --nocapture --test-threads=1
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun xctrace export \
  --input "$TRACE_DIR/tc207.trace" --toc --output "$TRACE_DIR/toc.xml"
```

This is a prepared recipe, not a validated invocation on this host. Confirm options against the installed Xcode help and choose a template containing Thread State Trace, context switches, wakeups and blocked stacks; Time Profiler alone is insufficient. Apple documents the [runnable/blocked/on-core distinction](https://developer.apple.com/videos/play/wwdc2026/268/), [Thread State Trace](https://developer.apple.com/videos/play/wwdc2023/10248/) and [CLI profiling](https://developer.apple.com/videos/play/wwdc2022/10106/). Export the actual schema listed in the TOC; do not invent schema/XPath names across Xcode versions. Preserve target stderr/panic and actual test exit independently: profiler success is not test success.

Run controls as separate deliberately failing records, then a predeclared natural cohort. Launch under the profiler so short tests are recorded from creation. If profiling still misses the target interval, record NOT CAPTURED rather than replacing the observation. Capture exact binary/source/profile, PID/native TID, trace start/end and dropped-event diagnostics.

Calibrate trace timestamps to frame `CLOCK_MONOTONIC` before calculating overlap: read bracketed `CLOCK_MONOTONIC`/`mach_absolute_time` pairs and `mach_timebase_info` before/after recording; connect them to the trace's actual absolute clock/start anchor. Retain offset uncertainty/drift and any machine sleep. If the export exposes only a relative timeline without a recoverable anchor, do not infer frame-local causality. A separate test-only signpost/anchor addition may then be needed, outside the strict measured interval; it is not implemented or claimed here.

Kernel scheduling records may include other threads even when filtering a target PID (the installed ktrace manual explicitly says Mach scheduling events are retained). Obtain the appropriate capture scope/privilege first, preferably on a dedicated host. Keep raw traces local and extract only the identified target interval before sharing. Do not disable SIP or alter privacy/privilege policy to force collection.

## Linux alternative — NOT RUN here

On an authorized dedicated Linux VM, verify `perf sched record/timehist -h`, tracepoint availability, kernel/perf versions and permissions first. This collection is system-wide to observe wakeups from other CPUs; a PID-only recording can miss the waker. Run the test as the original user, not as root. Example after setting `TRACE_USER`, `TRACE_DIR` and `TEST_BINARY` explicitly:

```bash
sudo perf sched record -a --clockid mono -o "$TRACE_DIR/perf.data" -- \
  sudo -u "$TRACE_USER" env FLISTWALKER_TEST_TC207_TRACE=1 "$TEST_BINARY" --exact \
  app::tests::tab_background_responses::tc_207_background_finished_walker_kind_queue_settles_incrementally \
  --nocapture --test-threads=1
perf sched timehist -i "$TRACE_DIR/perf.data" --tid "$TARGET_TID" \
  --state --wakeups --with-summary > "$TRACE_DIR/timehist.txt"
perf script -i "$TRACE_DIR/perf.data" > "$TRACE_DIR/events.txt"
```

Confirm that `--clockid mono` is supported and that recorded timestamps really use CLOCK_MONOTONIC. Preserve raw `sched_switch`, `sched_wakeup`/new, migrations and lost-event records, including the first100ms failure and test status. [The kernel's perf documentation](https://github.com/torvalds/linux/blob/master/tools/perf/Documentation/perf-sched.txt) distinguishes switch-out→switch-in wait from runnable→on-core scheduling delay. A latency summary alone is not frame-local evidence. Clip individual intervals to the exact TID/frame window after clock validation; retain boundary uncertainty and never sum whole-test/whole-process wait into one frame. Linux observations do not retroactively diagnose the old macOS failure.

## WSL handoff and completed task-scoped capture

The user supplied a WSL2 SSH session and repository location. [The completed WSL investigation](wsl/README.md) preserves52 natural cases/9,246 frames, actual task-scoped SWITCH events, two failing controls and a newly observed CPU-clock inconsistency. Original cause remains unknown. Full sched_switch/wakeup and host-vCPU tracing remain NOT RUN; the macOS System Trace prerequisite results above are unchanged. The [read-only preflight](wsl-preflight.py) prints structured command results, tracefs permission errors and required scheduler-event inventory. It never installs tools, invokes sudo, records events, mounts tracefs, changes sysctl or changes the kernel. A successful preflight exit means the diagnostic completed, **not** that trace recording is authorized or available. Three parser tests distinguish unreadable/unknown, actually missing events and available inventory without asserting capture permission.

On the target Windows host, first obtain the actual session identity with read-only PowerShell commands:

```powershell
$PSVersionTable.PSVersion
Get-ExecutionPolicy -List
wsl --version
wsl --list --verbose
```

Then run inside the intended existing distribution, from the intended repository after receiving the preflight file, writing only to a fresh owned output directory:

```bash
python3 docs/history/tc207-timing-2026-10-24/os-observations/wsl-preflight.py \
  > "$TRACE_DIR/wsl-preflight.json"
```

Preserve exit/stdout/stderr. If a Codex Windows sandbox reports no distro or missing user tools, compare the same read-only command in the actual Windows-user host context before claiming WSL is absent. Do not start/stop distributions, run `wsl --update/--shutdown`, edit `.wslconfig`, install generic kernel tools or change perf permissions implicitly. Kernel/tool installation is a later concrete decision after observing the actual versions. A missing/unreadable tracefs file is not proof that kernel support is absent; `CONFIG_SCHEDSTATS` being off is not by itself a reason to reject sched tracepoints.

[Microsoft describes WSL2 as a managed VM with a Linux kernel](https://learn.microsoft.com/en-us/windows/wsl/wsl2-about); [version/list commands](https://learn.microsoft.com/windows/wsl/reference) identify WSL1 versus WSL2, and the [Microsoft kernel repository](https://github.com/microsoft/WSL2-Linux-Kernel) owns its perf/kernel build instructions. Inspect the actual installed tool rather than inferring capability from a generic distro package name.

WSL2 can supply Linux guest `sched_switch`/wakeup evidence if supported. Keep it a separate source/environment axis with100ms unchanged. Guest scheduling traces alone do not establish how long the Windows hypervisor withheld a vCPU; high wall time while the guest believes a thread is running stays ambiguous without host-side evidence. Zero guest steal/pressure counters do not exclude host pauses. Host ETW/Hyper-V tracing and whole-VM capture require a separately defined scope; neither is included in this preflight. WSL results cannot retrospectively prove the original macOS cause or certify bare-metal performance.

## Causal decision and next step

| Same failing frame evidence | Supported direction |
| --- | --- |
|CPU lower bound itself high + corresponding CPU stack/code samples | actual processing cost; locate collect/kind/filter/commit or allocation work |
|Off-CPU high + complete blocked interval and matching wait stack/event | lock, I/O, fault or another identified wait; investigate that exact resource |
|Off-CPU high + complete runnable-but-not-on-core intervals | scheduling wait for those intervals, after checking loss/clock boundaries |
|Only low CPU, state snapshots, context-switch counts or adjacent stacks | mechanism still unknown; no threshold exemption |

WSL task-scoped events now demonstrate actual guest switch intervals and a preemption flag inside natural frames, while CPU-clock inconsistency prevents strict CPU subtraction there. The next discriminating observation is a naturally slow/failing frame in a complete calibrated event trace. Preserve the first failure and every predeclared result, with no success-only retry. The native macOS56 natural cases and separate WSL52 cases did not reproduce100ms, so no product fix or causal claim is justified.

## Validation and lifecycle

TDD: correlation identity/window/error tests were RED before the observer existed, then GREEN. Independent review reproduced three collector failure-path defects; regressions were RED before repair and all11 focused tests are GREEN after it (see [validation-tests.txt](validation-tests.txt)). Coverage includes observer exceptions, profiler timeout, independent/idempotent owner cleanup, partial-state/lifecycle retention, spawn/startup interruption, a100,000-row-per-child resource cap and nonzero exit on invalid experiment. The two existing stopped-child SIGTERM/SIGINT regressions also pass. Original successful observations and collector bytes remain frozen, not regenerated by the fixed collector.

The parent owns the test/profiler handles and their logs, while each worker owns one child's sample list. There is no mutable product state or new production lock. At most12 children run concurrently; a30-second test deadline,128-thread enumeration capacity and100,000-row-per-child cap bound observation. Reaching the row/time cap retains partial evidence and fails instead of silently truncating into success; a full enumeration buffer is explicitly unavailable data. The cleanup guard starts before process creation, resumes/kills/reaps only owned live children with bounded waits, attempts each owner independently and preserves partial data/error status. Repeated TERM is ignored after the termination handler starts; SIGKILL, host failure or unwritable/full storage cannot guarantee cleanup/evidence. PID/TID query races remain explicit unavailable samples.

Only diagnostic Python/history changed. Product tests/rendering, OS theme, native GUI, real application actions, threshold/specification and CI policy are unchanged. VM-001 is selected. The observer11 tests, WSL preflight3 parser tests, existing collector2 signal tests, repository contract, diff/local-reference checks and retained archive/hash checks pass. WSL runtime and owned-task perf SWITCH capture are complete; see [WSL evidence](wsl/README.md). Full wakeup/runqueue and Windows host traces remain NOT RUN. The Linux recorder4 and analyzer5 focused tests also pass; execution/export validity is distinct from event/clock interpretation. Original unavailable native GUI axes are not promoted.
