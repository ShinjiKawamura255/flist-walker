# TC-207 timing observations, 2026-10-24

The original 100ms failures remain unexplained. This collection distinguishes CPU execution from time outside CPU execution; it does **not** identify all off-CPU time as scheduler/run-queue delay. No production behavior, 1ms/2,048-entry finalization budget, strict `<100ms` assertion, ignored test, dependency, or CI policy changed.

## Source and collection identity

- Base: `a94590ff31697b59420aa3060fd648a06004b009` (PR #199 rebase-merged tip).
- Initial observation source (schema1): that base plus the three exact test-source hashes in [provenance](evidence/provenance.json), retained in [source-v1.tar.gz](source-v1.tar.gz). The final source adds native thread/time correlation (schema2), with separate [provenance](evidence-v2/provenance.json). The new module and all instrumentation are below `rust/src/app/tests/`, compiled only under `cfg(test)`. No runtime module changed. These uncommitted source identities are not relabeled as the base binary.
- Binary SHA-256: `853f0f241e7b021cea57af506b067f7252486094701188d181a71e4cb2fdf368`.
- Native macOS27.0.1 arm64, 12 logical CPUs, Rust1.98.1, debug test profile, locked/offline Cargo build. Binary identity was checked again after collection.
- The [original collector](collector-v1.py), matching both cohorts' recorded collector hash, declares three negative controls, three serial rounds, and one cohort each of4 and12 parallel processes before execution. Each natural process runs the same four exact 100k-entry cases (default, mixed owners, file filter, Walker unknown kinds), with one libtest thread. Total: **76 cases / 11,713 frames** plus three controlled failing frames. No replacement run or retry. The [current collector](collect.py) additionally handles SIGTERM cleanup; it is not relabeled as the collector used for these retained measurements.
- [Summary and every receipt](evidence/summary.json), [compressed complete stdout/stderr logs](evidence/), [SHA-256 inventory](evidence/sha256.json). Logs retain all frame records and failure/panic text. Receipts identify exact argv, test/request/frame/PID, exit code, progress, clock bounds, and the separately observed process-stop interval.

## Results

All times below are milliseconds. CPU is the sample interval enclosing the measured call; off-CPU is the conservative lower bound for time within the measured call that was not CPU execution. Each row's wall/CPU/off-CPU values describe the **same maximum-wall frame**, not independently chosen maxima.

| Cohort | Cases / frames | Maximum wall | CPU upper | Off-CPU lower | Outcome |
| --- | --- | ---: | ---: | ---: | --- |
| Serial, 3 rounds | 12 / 1,480 | 1.007209 | 0.981875 | 0.025334 | all strict wall gates PASS |
| 4 processes | 16 / 2,149 | 1.044792 | 1.035166 | 0.009626 | all strict wall gates PASS |
| 12 processes | 48 / 8,084 | 7.107250 | 0.997166 | 6.110084 | all strict wall gates PASS |
| Inject120ms of thread CPU | 1 / 1 | 120.401583 | 120.067750 | 0.333833 | **FAIL**, exit101, original100ms assertion |
| Inject150ms sleep | 1 / 1 | 155.095000 | 0.092042 | 155.002958 | **FAIL**, exit101, original100ms assertion |
| Own child SIGSTOP/SIGCONT | 1 / 1 | 155.980833 | 0.331167 | 155.649666 | **FAIL**, exit101, original100ms assertion |

The CPU control's CPU lower bound was120.065708ms. The parent independently observed the stopped child with `waitpid(WUNTRACED)` / `WIFSTOPPED` / `SIGSTOP` and retained **155.031459ms** between observation of the stop and sending `SIGCONT`. That interval proves the controlled child's inability to execute; it is **not** a natural scheduler-contention reproduction or a run-queue-delay measurement. The sleep and stopped controls deliberately produce similar wall/CPU readings. This demonstrates why low CPU alone cannot identify the waiting mechanism.

The12-process maximum was a file-filter frame: `filter_cursor` advanced50,418→50,722 and filtered output12,604→12,680; measurement bracket overhead was0.000750ms. The frame spent at least6.110084ms outside CPU execution while consuming approximately1ms CPU. Across **all** natural frames, the largest CPU upper sample was1.049584ms. These observations show off-CPU elongation without evidence of a100ms CPU-heavy frame. They do not prove the cause of the original failure or exclude an unobserved slow operation.

## Measurement boundaries and interpretation

The preexisting strict wall interval still surrounds `handle_background_index_response` (including source cloning). Outside that interval, the test reads request-owned progress and samples thread CPU immediately before/after. An outer wall interval measures the CPU-sampling bracket as well. JSON emission occurs **after settlement**, or immediately before a timing assertion failure, outside measured frames. Normal tests retain only the maximum-wall, maximum-CPU and maximum-off-CPU rows; trace mode retains every frame. All counters/progress are owned by the measuring test thread; no observer thread or production lock was introduced.

Let `W` be call wall time, `C` the enclosing CPU delta and `O` the outer wall interval minus `W`:

- CPU lower bound: `max(0, C − O)`; CPU upper sample: `C`.
- Off-CPU lower bound: `max(0, W − C)`.
- Bracket preemption can widen the bounds, so `O` is always retained. OS clock resolution/error still applies. Small CPU samples can exceed `W`; this is not interpreted as negative waiting.
- Missing/failed/retrograde CPU clocks produce `null`, never zero CPU or an inferred scheduling verdict. The present native Unix clock succeeded; the non-Unix branch remains `None` and was not executed here.
- **Only `W <100ms` determines acceptance.** A101ms wall/1ms CPU negative guard remains failing. CPU measurements, control classification and successful repeats never waive a failure.

The controlled stimulus is injected only when both explicit test-only trace/control environment options are supplied. The collector uses one exact test in a disposable child. Its `finally` resumes any live child, kills it if necessary, and waits with a finite deadline. Independent review found that the original collector did not route ordinary SIGTERM through this cleanup. The current collector converts SIGTERM into an exception and ignores repeated TERM during cleanup; deterministic stopped-child SIGTERM/SIGINT regressions cover resume/kill/reap. SIGKILL or other uncatchable termination cannot guarantee Python `finally`; no unrelated/GUI process is targeted. No collection was interrupted here; retained measurements and original collector source remain unchanged. Controls are recorded as deliberately failing test executions, not product PASS or original-defect reproduction.

## Final thread/time correlation source: separate cohort

Schema2 adds the OS thread ID (macOS `pthread_threadid_np`, Linux `gettid`; unavailable platforms retain `null`) and a `CLOCK_MONOTONIC` nanosecond window enclosing each measured call. The clock reads are within the reported outer measurement bracket and outside the strict wall interval. This supplies an actual thread/time identity for future trace correlation; a profiler with another clock domain still requires clock calibration. None of these fields asserts a scheduling verdict.

The final binary SHA-256 is `7da08943d0e8adf3cdba49e0af2f25360a190e8ae716b7fd9affda125cc5cf91`. A separate execution of the declared collector retained **76 natural cases /12,720 frames**, plus three controlled failing frames. A full host Cargo regression was started immediately after this collector. Its exact compile/test overlap was not timestamped; therefore these results are **not an isolated load-level comparison** and are not pooled with schema1. See [explicit context](evidence-v2/context.json), [receipts and raw results](evidence-v2/summary.json), [all logs](evidence-v2/), and [hash inventory](evidence-v2/sha256.json). No failure was retried or replaced.

| Final-source condition, with concurrent validation | Natural frames | Maximum wall ms | Same-frame CPU upper ms | Same-frame off-CPU lower ms |
| --- | ---: | ---: | ---: | ---: |
| One collector process, three rounds | 1,510 | 1.042375 | 1.042667 | 0 |
| 4 collector processes | 2,439 | 5.744958 | 0.042500 | 5.702458 |
| 12 collector processes | 8,771 | 78.683833 | 0.226500 | 78.457333 |

All natural wall gates passed. The maximum frame's measurement overhead was0.001209ms; the off-CPU lower bound cannot be explained by that bracket. All12,723 raw frames (including controls) had non-null native thread IDs and monotonic windows. The final-source CPU, sleep and stopped controls again exited101 at the100ms assertion: wall120.426875/155.028000/162.089416ms, CPU120.078666/0.101333/0.551250ms respectively. The parent independently observed160.038584ms of stopped-child time in the last control. This validates the final measurement source, without turning controlled failures into evidence of the original defect.

## Limits and next discriminating evidence

The original health-review failure affected file-filter and Walker kind-queue tests. The retained old log had only100ms assertion failures, without elapsed values or thread CPU. It cannot be retroactively classified. The new bounded campaign did not reproduce a natural100ms failure; it establishes diagnostic behavior and a smaller off-CPU elongation.

`xctrace list templates` was attempted both inside the sandbox and on the host. Both returned exit1: the active developer directory `/Library/Developer/CommandLineTools` is a Command Line Tools instance, and `xctrace` requires Xcode. System Trace / ready-to-run / blocked-stack evidence is **NOT RUN** in this configured environment. No developer-directory, privacy, privilege, or CI setting changed. Windows/Linux native measurements and non-Unix CPU collection are also **NOT RUN**.

For a future natural overrun, preserve the **first** failure and this raw frame record. A large CPU lower bound directs profiling toward the captured collect/kind/filter/commit progress boundary. A large off-CPU lower bound requires an OS scheduling trace correlated to that exact process/thread/time interval: ready-but-not-running supports scheduling wait; a blocked stack/event directs investigation toward locks, allocation/page faults or I/O. Merely observing an involuntary context switch or low CPU does not quantify run-queue delay. No natural observation here is labeled confirmed scheduler delay. Thread/time correlation is now present; collecting that native trace remains necessary before assigning such a cause.

## Validation

- TDD RED: the old diagnostic omitted the required off-CPU/measurement-overhead fields; the focused guard failed for that reason. GREEN after implementation.
- Focused TC-207:87 passed. New clock-bound tests cover CPU-heavy, off-CPU-heavy, bracket uncertainty and unavailable CPU observations. The actual controls additionally exercised measured CPU work, sleep and externally confirmed suspension.
- Full host `cargo test --locked --offline`: lib1,655 passed /20 ignored; integration,48 CLI contract cases and native macOS menu contract passed. Existing ignores were unchanged.
- `cargo clippy --locked --offline --all-targets -- -D warnings`, format, repository contract and diff checks passed.
- Independent review's collector SIGTERM finding: stopped-child regression failed before the handler, passed after it; SIGINT regression also passed. The fake child used by `test_collect.py` is lifecycle evidence only, not timing/performance evidence.
- VM-001 / VM-002 apply. Test-only measurement and history changes do not alter rendering, focus, routing, worker scheduling, index production code or the native GUI; no additional GUI launch was needed. This evidence does not upgrade the previously unperformed Browse manual checks.

## Separate PR200 CI oracle failure

On head `4385d370525cfb12220ca71466a26c6bf33b6a81`, [macOS job120533699471](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39988429966/job/120533699471) failed the unchanged TC-229 `F1-mid-ignore` oracle. The30-second constructive-stall record had zero worker/index/result debt and a completed latest request3. Its revoked predecessor2 recorded `Finished offer/current=true` at275.029833958, permission at275.029872458, mailbox close at275.029928708, send return at275.029940375, and worker return at275.029941583 (Instant seconds). No terminal was published. The existing oracle required `offer_current=false` for a closed mailbox and could never accept this legal offer→refresh-close→failed-publication interleaving. This is an oracle state-classification defect, not a measured30-second worker stall, an external transient, or an explanation of TC-207.

The follow-up test-only helper admits this case only for a planned Ignore List refresh with a same-tab successor, owned admitted/started root/source, positive data work, an unpublished Finished offer, physical return and bookkeeping release, and the complete `admitted≤started≤data≤offer≤permission≤close≤send-return≤body-return` chain. It never promotes the old request to a successful terminal. The required latest-generation success, final snapshot oracle, debt, throughput gates and deadlines remain mandatory. A replay of the reported event order was RED before the change; negative controls reject missing/misordered/foreign evidence. The failed hosted run remains retained; no unchanged retry is used to erase it.

This harness follow-up occurred **after** the two collections above. Neither old binary/source identity nor its observations are relabeled as the follow-up PR head. No benchmark thresholds or calibration values were changed. TC-207's original cause remains unconfirmed and OS scheduling trace remains NOT RUN.
