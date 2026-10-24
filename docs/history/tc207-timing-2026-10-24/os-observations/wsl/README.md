# TC-207: WSL2 task-scoped OS events

Original TC-207 cause remains **UNKNOWN**, and the strict wall `<100ms` assertion is unchanged. This WSL run collected real `PERF_RECORD_SWITCH` events without root or system-wide tracing. It did not reproduce a natural100ms failure and does not establish the cause of the earlier macOS failures.

## Environment and identity

The user supplied SSH access and `~/work/flistwalker`. Sandbox SSH returned `Operation not permitted`; the same strict-known-host/BatchMode command in the authorized host context succeeded. Connection address and key contents are excluded from evidence. The repository resolves to `/mnt/d/work/flistwalker`, clean at `8a452b3858c80f70a5f3064834c8add4d9b1d347` (v0.30.0), with review preflight PASS. No branch, ref, worktree registration or repository file was changed there. [Postcheck](postcheck.json) confirms the same clean HEAD and no live owned diagnostic processes.

WSL2 kernel `6.18.54.1-microsoft-standard-WSL2`, Ubuntu24.04.5, x86_64,32 logical CPUs. The existing Rust1.97.1 matches the local pinned compiler commit. A fresh owned `/tmp/tc207-f5e2062-a68x38ez` on the Linux filesystem received only the public committed Rust sources/license and two compile-time reference files from `f5e20628b4da774bc640ecc1f5351c7179849ed8`. GitHub metadata confirmed public visibility after automatic review initially rejected a whole-repository transfer; that rejected transfer did not occur.

Offline build first failed because the locked `rustls0.23.46` was absent. A normal locked dependency fetch/build then exposed the two omitted `include_str!` reference files; supplying their exact committed bytes allowed `cargo +1.97.1 test --locked --offline --lib --no-run --message-format=json` to pass. These were preparation failures, not TC reruns. Cargo.lock was unchanged. Cargo JSON uniquely selected the test-profile executable `flist_walker-664499314caf2350`, SHA-256 `ef4be2be4c6214b02afe8b02bc5e68fd8248f640fcf05b41db07bcd1d4027836`. [Source/tool binding](source-binding.json) and [binary identity](binary.json) retain hashes/profile/compiler; no binary is included.

## Capture capability and limits

[Initial preflight](preflight.json) could not find `perf` on PATH. Direct discovery found `/usr/lib/linux-tools/6.18.54.1-microsoft-standard-WSL2/bin/perf`, version `6.18.54.1.g26d1ceb55087`. [Direct probes](perf-probe.json) confirm user-scoped `perf stat -- true` succeeds. However this build has `libtraceevent=OFF`, `perf sched` is not a command, tracefs is root0700, and `sudo -n true` requires a password. An empty `perf list sched:*` is not proof of absent kernel tracepoints. No package/kernel/tool install, sysctl/tracefs permission change, mount, root acquisition or Windows PowerShell/ETW capture occurred.

The [owned sleep probe](switch-probe.json) demonstrated the narrower mechanism actually available:

```bash
PERF=/usr/lib/linux-tools/6.18.54.1-microsoft-standard-WSL2/bin/perf
"$PERF" record --switch-events --clockid mono -e task-clock:u -F 99 \
  -o "$TRACE_DIR/probe.data" -- python3 -c 'import time;time.sleep(0.2)'
"$PERF" evlist -v -i "$TRACE_DIR/probe.data"
"$PERF" script --ns --show-switch-events -i "$TRACE_DIR/probe.data"
```

The [kernel perf record documentation](https://github.com/torvalds/linux/blob/master/tools/perf/Documentation/perf-record.txt) defines SWITCH recording; [the UAPI](https://github.com/torvalds/linux/blob/master/include/uapi/linux/perf_event.h) defines PID/TID/TIME metadata and the preemption flag as TASK_RUNNING at switch-out. This is a guest switch timeline, **not** wakeup-to-running latency. Nonpreempted OUT→IN combines blocked time and any later scheduling wait; a preemption flag does not prove uninterrupted runnable state throughout the interval. A guest “on-core” interval does not exclude Windows withholding its vCPU.

## Declared campaign and results

[linux-record.py](../linux-record.py) declared CPU/sleep controls once each, one serial process with four natural cases, then12 parallel processes with four natural cases each. One libtest thread/process,30s deadline, no replacements, no concurrent Cargo build introduced. Perf samples task CPU at99Hz and records SWITCH metadata for the owned worker/test descendants; it can perturb timing. Host background work was uncontrolled. Perf status, actual test receipt and export status are retained independently. All15 executions/exports succeeded; both controlled tests retained exit101 and100ms assertion failure; all52 natural cases passed.

| Observation | Wall ms | Matched SWITCH overlap ms | Interpretation |
| --- | ---: | ---: | --- |
| CPU control |118.402594 |0 | known busy loop;11 matching PID/TID user CPU IP samples in window, no guest switch-out recorded there; no host-pause exclusion |
| Sleep control |150.279359 |150.052017 | conservative frame overlap of a recorded nonpreempted OUT→IN; known sleep stimulus |
| Natural maximum wall, Walker kind frame222 |1.686900 |0 | strict gate PASS; no guest switch overlap, mechanism of this smaller extension unresolved |
| Natural maximum non-on-core overlap, default commit frame47 |1.004904 |0.644013 | actual guest non-on-core interval, wait mechanism unknown |
| Natural preempted example, file filter frame43 |0.608799 |0.309955 | switched out with TASK_RUNNING flag; precise wakeup/runnable duration not established |

There are **52 natural cases /9,246 frames, zero natural100ms failures**. CPU and sleep controls add two frames. Sleep PID10855/TID10856 has full event interval `1116917822857→1117067876155ns` (150.053298ms). Removing the whole measurement bracket from both frame-window edges conservatively clips that to150.052017ms. The file-filter preemption example is PID11083/TID11365, interval `1118228080774→1118228390729ns`, wholly inside that frame. These are interval overlaps from events, not durations inferred from sampling counts.

All15 event attributes specify `use_clockid=1`, `clockid=1` (CLOCK_MONOTONIC), `context_switch=1`, `sample_id_all=1` and IP|TID|TIME|PERIOD. [linux-analyze.py](../linux-analyze.py) rejects other layouts, decodes exact binary PID/TID/TIME, sorts raw per-CPU buffer delivery by the common timestamp, checks alternating events and window boundary coverage, and clips only matching PID/TID intervals with a full-bracket margin. All9,248 frame windows have usable boundary coverage; zero LOST/LOST_SAMPLES records or lost/unordered export diagnostics were observed. Absence of recorded loss is not proof against every possible kernel/tool failure. Raw perf data and exports remain available to recheck this conclusion.

## Newly observed CPU-clock inconsistency

The CPU control's emitted `cpu_lower_ns=120200237` exceeds wall `118402594ns` despite only8048ns of bracket overhead. Overall9,165 of9,248 emitted lower fields exceed their frame's wall. These CPU/off-CPU fields are preserved **as raw readings**, not accepted as strict duration bounds for this WSL environment. Saturating subtraction producing zero is not evidence of no wait. The label `experiment_valid` validates the declared test/control outcome and exports; it does not calibrate clocks or establish a cause.

[Five additional declared busy loops](clock-calibration.json), each stopping at120ms of CLOCK_THREAD_CPUTIME_ID, measured only118.477088–118.499815ms of enclosing CLOCK_MONOTONIC. This confirms the inconsistency independently of the Rust measurement helper. Current clocksource is `tsc`; the reason for the clock-rate mismatch is **UNKNOWN**. No correction factor or clocksource change was applied. Reproduce this diagnostic separately from TC outcomes:

```python
import time
for _ in range(5):
    m0 = time.clock_gettime_ns(time.CLOCK_MONOTONIC)
    c0 = time.clock_gettime_ns(time.CLOCK_THREAD_CPUTIME_ID)
    while time.clock_gettime_ns(time.CLOCK_THREAD_CPUTIME_ID) - c0 < 120_000_000:
        pass
    c1 = time.clock_gettime_ns(time.CLOCK_THREAD_CPUTIME_ID)
    m1 = time.clock_gettime_ns(time.CLOCK_MONOTONIC)
    print(m1 - m0, c1 - c0)
```

## Evidence and remaining discriminators

[CAPTURE.json](CAPTURE.json) binds [capture-v1.tar.gz](capture-v1.tar.gz), all237 file hashes/238 members, sidecar hashes and exact used collector bytes. It contains all15 raw perf files, binary dumps, event exports, attributes, test logs/receipts, commands, build attempts and helper sources. [ANALYSIS.json](ANALYSIS.json) is reproducible with `python3 ../linux-analyze.py <extracted-records-v1-directory>`. No success-only retry or post-review replacement cohort was collected. The owned remote `/tmp` build/evidence is retained for continuation; its location is not used as durable current truth.

The strict100ms wall acceptance remains authoritative. No Rust/specification/budget/CI/release change is justified by this non-reproduction. The WSL results are a separate environment axis from the earlier macOS measurements and must not be pooled as one performance campaign.

Next, capture a naturally failing frame with complete `sched_switch` plus wakeup events on an authorized host, using the parent report's Linux recipe after verifying a libtraceevent-enabled perf, actual kernel tracepoints and recording access. Root0700 tracefs and the current perf build prevent that recipe here; package installation or privilege/trace-scope changes are not implied. First resolve or bound this guest's CPU-clock inconsistency before using CPU subtraction for causal claims. Windows host ETW/Hyper-V tracing is a separate scope; it was not performed. Original TC-207 cause, exact blocked-versus-runnable split and host-vCPU pause remain unknown.
