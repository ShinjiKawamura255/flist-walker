# Linux coverage SIGSEGV investigation (2026-10-18 UTC)

## Disposition

**OPEN: cause unknown; actual failure stack NOT RUN.** The original SIGSEGV is
retained as a failure. Subsequent non-reproduction does not establish a fix or
an external transient cause. PR [#190](https://github.com/ShinjiKawamura255/flist-walker/pull/190)
must not merge on the strength of these diagnostic successes alone.

The original v0.31.0 candidate remains BLOCKED. No tag, release, repository
setting, required-check exception, or trusted master policy activation was made.

## Original failure and comparison boundary

[Run39130272096 attempt1](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39130272096),
source `cde026a3bcf873b61308381b6303758313bb498e`, coverage
[job117683872916](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39130272096/job/117683872916):
at13:14:17Z the library test process exited with signal11/SIGSEGV; Cargo exited101.
No assertion failure, active-test record, core, fault address or stack was captured.
The last printed successful tests do not identify the crashing test under parallel execution.

`rust/`, Cargo inputs and workflows have no diff between base
`3194f03c27f9a72f06a836dbd02e9141d8740a26` and `cde026a`.
The macOS build and Windows helper repairs are outside the Linux coverage path.
This does not identify a product, dependency, instrumentation or environment cause.

The earlier successful master [run39086272030](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39086272030)
and the failed run used Ubuntu24.04 image20261012.334.1, Rust1.97.1
(compiler8bab26f4f), cargo-llvm-cov0.8.7 and the same locked workspace lcov
command with75% minimum line coverage. The original binary digest, kernel,
libc digest and hardware were not recorded, so exact host/binary identity
with the original failure cannot be asserted.

## Hosted Linux observations

| Run / source | Actual coverage observation | Other diagnostic result |
| --- | --- | --- |
| [39134318763](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39134318763) / `e13357b114ca3d53f0816f4a165a1f44d487610d` | One PASS under live GDB; no actual fault stack | Native GDB smoke6PASS; main CI SUCCESS; temporary workflow rejected by Guardian as intended |
| [39135917398](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39135917398) / `9d0f4a672e4c4a9a94133a52ce893d53e3fffce9` | NOT RUN,0observations | Fixture core generated, extraction failed; generic error did not identify the failing extraction stage; original core pattern restored |
| [39136431380](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39136431380) / `be37c43d93f033ee484ec7957cc2760cdde4639f` | NOT RUN,0observations | Fixture core generated, no structured stop record; original core pattern restored |
| [39136933899](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39136933899) / `3ee0bcd4d4f3526bca9b374f10f65ece84dce637` | Five PASS without live ptrace, original argv/default parallelism/threshold retained | Core smoke recovered signal11 and `diagnostic_crash` frame in7structured records; actual observations each exited0; final cleanup failed with exit2 after core-pattern restoration |

The final campaign's [coverage job117703377627](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39136933899/job/117703377627)
reported1626 library tests PASS/19ignored on each observation. Library runtimes
were47.13–48.30seconds. Every observed library executable was416003032bytes,
SHA256 `b2164b08861d91c8f74c0cd5e93a9f96be7f9661e4bc2f66880c4d05ca47f414`,
matching the live-GDB observation. These are observed binary identities,
not the missing original failure digest.

Five observations were fixed in advance, bounded to900seconds and configured
to stop at the first failure. The synthetic fault stack verifies the capture
path only; it is not a stack from the original product failure.

## Diagnostic limitations and cleanup failure

GDB15.1 core loading succeeded after using a validated safe absolute filename
without quotes. Earlier attempts lacked enough error detail to prove the precise
loader failure. Structured output discarded incidental headers; raw core,
argument values, locals and environment dumps were not published. Core size
was limited to2GiB; unknown or unavailable evidence was not promoted to PASS.

The final job failed in diagnostic cleanup, after five coverage successes and
`coverage_core_pattern_restored=true`. Its code removed indexed metadata but
required the directory to be empty. Existing TC-165 directly starts a child test
executable (`rust/src/indexer/tests/mod.rs`, hard-abort convergence test), which
intentionally aborts in `rust/src/indexer/filelist_writer.rs`. That child inherits
core settings without the Cargo runner's PID index, providing a concrete source
of unindexed core files. Remaining filenames were not recorded, so this is a
supported cleanup explanation, not a verified identification of every remainder.
Raw remainders stayed inside the disposable runner and were not uploaded.
This does not explain the original parent-process SIGSEGV.

Independent read-only review confirmed the observations and recommends stopping
the fixed campaign rather than adding speculative changes or repetitions.
At `a3342d0def29279c00330ef1af7222f0368c9336`, all temporary workflow changes
and six diagnostic source/test/fixture files were removed; their exact sources
remain in the listed Git history. That workflow was restored byte-for-byte to
base3194f03. Its normal CI/review results belong to PR190 and cannot
retroactively resolve this failure. Subsequent capture-path work follows
[the Linux diagnostic contract](../../../CI_OPERATIONS.md#linux-coverage-fault-evidence)
and keeps this original failure unchanged.

The remaining investigation needs an actual fault stack or a reproducible
condition with a causal failing test. Environment mutation and background-worker
hypotheses have no direct stack evidence and do not justify a product patch.
