# Native-admitted v10 reference and input boundary

The immutable reference `fcdea8ee05a5f08c5ee19a9ae8b58b8c8736a645` (tree947632e4526b5095cc7162f61268845f96e0bb11) preserves healthy v9 product6edfb599 rather than replacing it with current master. [PR217](https://github.com/ShinjiKawamura255/flist-walker/pull/217) is validation-only and never merges. [Reference CI40543471307](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/40543471307) and Guardian40543471293 passed attempt1, all10jobs including audit/GNU build/E2E/CI Gate. All9 actual execution checkouts2054bfa26665876d75b7088c5a9c65fefb76fc5b have parents43f+fcde and tree equal the intended head. Linux1633/macOS1624/Windows1654/coverage1633 library tests passed; original two correctness smokes and both GUI guards passed all four axes. Original ZIP SHA256 `56c34c342b5d30cdae7d6724a4c2e835fe67736c4d05d3f4e05589dd4fd69d94`,13 selected original log hashes and all checkout/tree proofs are in the archive support.

[Exact input maps](reference-input-equivalence.json) independently enumerate240 healthy Rust/Cargo/fixture paths and241 reference paths:234 identical paths,6 modified and1 added. The complete union is241 paths. Changes are explicitly bounded:

| Input | Reason and comparison impact |
| --- | --- |
| Cargo.lock | rustls-webpki0.103.15→0.103.16 (RUSTSEC-2026-0382), webpki-roots1.0.7→1.0.10 (RUSTSEC-2026-0381), clipboard-win5.4.1→5.4.3 (yanked warning, not a vulnerability assertion). Cargo.toml/version0.30.2 and other dependency inputs remain healthy-source bytes. |
| extensions/driver.rs, mod.rs, runner.rs, added finished_offer.rs | Reviewed test harness/oracle and measured finite completion trace, including PR200 finished-offer revocation. Original two A→B→A pressure inputs, owner/epoch, all oracle/deadline/overlap conditions remain; fixed measured Ready-B/kind/A completion is documented in TC-229. |
| app/render_tabs.rs | Exactly upstream2d32faaf03c56c12bd6f0c9ec94fa9962af41639 three-line hunk (two comments and one physical-pixel spacing call), separately approved as the only additional product-Rust exception. This corrects fractional geometry and is reached by measured frames; no zero-cost claim. |
| app/tests/tab_drag.rs | Exact upstream43-line fractional-scale regression, no tolerance relaxation. |

No selection/reveal implementation or current indexing/process product changes were transplanted. Current master is not used as the reference. The native carrier's CI/tooling and reviewed test changes are provenance; the reference's non-harness product source remains healthy except the explicit pixel-padding exception and lock updates above.

Null source `597a7d4dd48ffca67b9529a3afcec5459c3a9276`, tree `db2db91af8cc03c9a7a477bc91e2e6de44f569f7`, [PR218](https://github.com/ShinjiKawamura255/flist-walker/pull/218), keeps all241 Rust inputs byte-identical to R and changes exactly four pin/expected-test literals across two of301 collected files;299 remain identical. All11 measured harness files match reference, null and application product base43f. Current product differences remain recorded and comparable, so a later ordinary product comparison need not claim Rust null equality.

[Null native CI40553677031](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/40553677031) and Guardian40553677122 passed attempt1/all10jobs. All9 actual checkouts8bf688a9b4e3af3424cf3b1f806a2c806d3ac956 have parents43f+597 and tree equal the intended head. All four native/coverage axes passed the same original smokes/GUI guards and counts as R. Original ZIP SHA256 `de541a76c75fc9e163ced3ae3d8c28564e95f68e549b557773649b570ed1add9` and13 hashes are bound in the original32-support manifest and archive. Previously rejected local/CI source states remain recorded under their own identities and are not reused as admission.

The [fresh accepted prospective five](measurement-diagnosis.md) is bound to these actual sources. Local old-source100k diagnostics, historical v9 results and native GUI/manual platform checks do not supply its numeric evidence. Actual interactive GUI/personal-Windows tests and explicit ignored TC-235 are NOT RUN here. The new application-source native/required gates and ordinary current-product RCR remain separate PR/run evidence; carrier PRs never merge into product master.

## CI and tooling range from the healthy checkpoint

The exact collected non-Rust source maps are also in the input-equivalence JSON.22 paths differ between6ed and R. All of R's `scripts/`, `.github/workflows/` and `.cargo/` path/digest maps equal product master43f. This tooling comparison excludes `rust/Cargo.toml`, `rust/Cargo.lock` and all other `rust/` inputs; their preserved-source differences are enumerated above. This carrier introduces no new trusted-policy/security setting. The workflow changes and diagnostics were already adopted on master and are used for new native admission, not claimed as original6ed environment equivalence. RCR uses the same current collector/toolchain/build settings for every role, with each source separately recorded.

- `.github/workflows/ci-cross-platform.yml`
- `.github/workflows/ci-policy-guardian.yml`
- `.github/workflows/security-audit.yml`
- `scripts/build-rust-macos.sh`
- `scripts/check-updater-n-minus-one-compatibility.py`
- `scripts/check_ci_policy.py`
- `scripts/indexing_perf.py`
- `scripts/indexing_perf_contract.py`
- `scripts/indexing_perf_gate.py`
- `scripts/linux_coverage_diagnostics.py`
- `scripts/linux_coverage_gdb.py`
- `scripts/linux_coverage_runner.py`
- `scripts/prepare-release.sh`
- `scripts/test-updater-n-minus-one-compatibility.py`
- `scripts/tests/fixtures/linux_coverage_fault.c`
- `scripts/tests/test_build_rust_macos.py`
- `scripts/tests/test_check_ci_policy.py`
- `scripts/tests/test_indexing_perf.py`
- `scripts/tests/test_indexing_perf_gate.py`
- `scripts/tests/test_linux_coverage_diagnostics.py`
- `scripts/tests/test_release_readme_literals.py`
- `scripts/tests/test_windows_candidate_helper.py`

The null source changes only the comparator reference/protocol and the corresponding hard-coded expected literals; all other collected tooling is byte-identical. The adoption application does not change any workflow, Cargo input, packaging script or product Rust from current master.
