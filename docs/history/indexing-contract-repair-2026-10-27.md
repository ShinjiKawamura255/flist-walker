# Indexing comparison contract repair — 2026-10-27

## Observed failure and scope

[Run 40409432092](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/40409432092) is completed/failure, candidate `4cc35b451a46578cadaff024ac62b4e4a207f121`, published v9 reference `6edfb599d69fc31657032a1419bdd255a165fe80`. The oracle addition in PR #200 (`1ba32ae`) changes `extensions/driver.rs`, declares `finished_offer` in `extensions/mod.rs`, and adds `extensions/finished_offer.rs`. Comparing different work contracts must reject. The previous investigation found nine observed-valid component receipts across three groups, successful builds/discovery/measurements, matching raw hashes and source before/after, but no `summary.json`; none is a numeric gate pass. This repair preserves that rejected identity and does not reuse its rows as calibration.

## Local repair

The comparator now selects every tracked path/digest under the indexing harness directory, rather than deriving its inventory from a fixed required-source subset. The receipt minimum also names the previously omitted current harness modules. Both path-set changes and content changes reject on either side. Production changes remain eligible for measurement. The same function runs immediately after exact reference checkout, before any build, and again during final triplet admission. Existing per-leg source, runtime, oracle, physical-return, budget, provenance and numeric checks remain.

Synthetic regressions cover every present harness file, future leaves, both comparison directions, product differences, and no collection after a preflight mismatch. A real disposable shallow repository exercises fetch/clone/exact checkout, keeps candidate HEAD and clean state unchanged, retains the invalid setup receipt, and proves that no build starts. Synthetic timing controls remain synthetic.

## Prepared reference successor — not admitted

The separate local branch `codex/indexing-reference-finished-offer`, commit `0977b3a526518ce68c2328e9672d056d2d372a76` (tree `c9c90b4c4ed291af352a650d3c88204e3ce783b5`), starts at the healthy v9 reference. Only the three reviewed PR #200 harness paths are transplanted; the entire harness matches candidate `4cc35b4`. All 265 non-harness Rust/build/workflow/shell/PowerShell inputs from the healthy reference are unchanged. The companion `indexing-contract-repair-2026-10-27/reference-equivalence.json` preserves path/hash evidence. This is not a rebaseline to current master, and no product regression is hidden by it.

A future reference pin and protocol `same-job-RCR-f1-matched-21-observer-v10` are proposals only. The active constants and numeric enforcement remain unchanged. No dirty overlay or historical native result admits the successor.

## Additional reference admission blocker

The exact preserved healthy lockfile now fails a fresh audit (advisory DB `3279c99911354c400e1d41e47c7d60e39258b147`, 1378 advisories): `rustls-webpki 0.103.15` is affected by RUSTSEC-2026-0382 and `webpki-roots 1.0.7` by RUSTSEC-2026-0381. Audit exits1, and additionally reports the yanked `clipboard-win 5.4.1` warning. This is a current dependency gate failure, not a Python contract bug or native-test failure. Current candidate audit passes. The test-only successor cannot be called native-admissible as prepared.

Already reviewed master commits `223f868043c7b92b1dddee375cd7659d060fd02f` (two TLS dependency fixes plus notices/OSS evidence) and `b23006e1` (yanked clipboard replacement) identify a possible narrow dependency refresh while retaining healthy product Rust. Adopting such a refresh needs an explicitly revised reference/input-equivalence record, notices/OSS consistency, new validation and the same fresh source-bound campaign; it is not a pure test-only reference. No lockfile update, audit suppression or baseline redefinition is performed here. Decide this source scope before publishing a reference intended for admission.

## Validation and activation boundary

Local verification: Python suite162 PASS; candidate macOS library1668 PASS/20 ignored plus integrations PASS; prepared reference library1619 PASS/19 ignored plus integrations PASS; reference focused indexing94 PASS/4 heavy ignored. Both source trees pass fmt and all-target Clippy. Candidate fresh audit PASS; prepared reference fresh audit FAIL as above. Repository contract, guardian policy and all8 workflow YAML parses PASS. The first sandboxed candidate suite failed two Unix-socket tests; the full native retry passed. Ignored heavy performance tests remain NOT RUN. Logs and hashes are retained in `indexing-contract-repair-2026-10-27/local-validation.json` and the task handoff. Python synthetic/real-Git tests, macOS correctness tests, format, Clippy and audit do not establish Linux/Windows native admission, native GUI behavior or a performance campaign.

Before activation:

1. Resolve the dependency-scope decision above before seeking reference admission. Obtain new exact-source Linux/macOS/Windows native evidence, applicable builds, Clippy, coverage and Windows GNU E2E through a separately authorized validation route. Current `ci-cross-platform.yml` triggers only master push/PR; an ordinary PR from the old reference branch to current master can test a synthetic merge containing newer product code. Such results do not admit the prepared reference unless the actual checkout/tree is proven equal. Keep the input-equivalence record attached. Never merge an old reference into product master; do not change workflows, repository settings or native gate requirements to manufacture admission.
2. Prepare the v10 collector pin/protocol change and obtain its own exact-source native gates. Independent review must inspect the final source and the unchanged numeric/correctness policy.
3. Reserve five wholly fresh sessions before observing outcomes: exploratory 01–03 and held-out 04–05, all three groups per session (`f1`, `matched`, `stable`), all original rows/receipts and source-CLI replays. Freeze source, reference, protocol and existing policy before session01; any contract change retires the reservation and starts anew. Do not substitute successful siblings or old v9 rows.
4. Run each group serially on the intended Ubuntu24 hosted runner under the existing shared budgets. Coordinate competing machine load before any local diagnostic timing. Compare full Rust/build inputs in source-bound null campaigns. Preserve failures without removing outliers.
5. Adopt the new pin/protocol only after reference/source admission, fresh campaign acceptance and independent review. Required GitHub checks and publication/merge authority remain separate.

NOT RUN here: GitHub Actions/manual reruns, push/PR/merge, new native reference/collector CI, full prospective campaign, Windows host operations, native GUI. They require separately authorized external operations or another platform; no Windows session was opened. The weekly comparison remains blocked at work preflight until admission is complete. This record claims a local tooling repair and a prepared successor, not a restored passing weekly gate.
