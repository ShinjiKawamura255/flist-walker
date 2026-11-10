# Explicit stable Canary: source-bound evidence

This is a historical record, not standing approval for another protection change.
Current procedure remains [CI Operations](../../CI_OPERATIONS.md).

## Source and purpose

PR [255](https://github.com/ShinjiKawamura255/flist-walker/pull/255), source
`61515d9726193a31be6deae10c483f920ac4d618`, base
`2c2f2e1166374b3692565c9a6f8759d59edccef5`, makes both Canary Cargo commands
explicitly select `+stable`. Merely installing stable had not overridden the nested
`rust/rust-toolchain.toml`: original Canary42734271587 installed1.99.1 but its Cargo
step selected pinned1.97.1. A blocking-step contract and negative tests protect the
new selection. Ordinary CI pins, runtime, thresholds and required checks are unchanged.

[CI42845028747](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/42845028747)
passed all10 jobs including native3OS, Clippy/coverage, Cargo Audit, GNU updater E2E
and CI Gate. Actual PR merge checkout `a22a1e5faceb701d7894600aa24895bbc73910fc`
has parents2c2f2e11/61515d97 and tree `b157d782b805843a461d311bff2ff456cfcb9b41`,
identical to the reviewed source. Local policy40 and full Python175 tests passed.
Independent source review and post-base-replay review found no remaining findings.

Old Guardian [42844686158](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/42844686158)
and ready-triggered [42851151624](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/42851151624)
correctly rejected only ci-canary.yml, check_ci_policy.py and its tests as immutable
structural changes. These original failures were not rerun or converted to success.

## Limits and separate findings

Pinned CI success is not a latest-stable compatibility claim. Explicit local1.99.1
Clippy exposed pre-existing `fetch_update` deprecations in index_worker/process.rs24
and worker/channel.rs58. This rollout does not suppress or fix those findings.

Issue253's macOS TC162466ms failure remains causally unresolved. Diagnostic PR254
adds bounded-wait observations without weakening its450ms threshold; its merged
master2c2f2e11 passed [CI42841172471](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/42841172471).
Non-reproduction on that different source does not prove environmental-only failure.
No manual Canary dispatch, unchanged-source CI retry, release or tag was performed.
