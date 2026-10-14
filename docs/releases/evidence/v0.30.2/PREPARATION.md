# v0.30.2 preparation evidence

Previous public release: v0.30.1, published2026-10-07; initial base5866b979967ffcd1f29bb80fb6fdaf1609d3ef50. This is a preparation record, not a publication claim.

## Complete release-range classification

| Commit | Responsibility / note mapping |
| --- | --- |
| 47e9ced3 | Previous-release closure evidence; release documentation maintenance under Summary/Changed, no old runtime fix newly credited |
| 990c9d09 | Draft/public URL distinction; Changed release documentation |
| 0342ef26 | cfg(test) activation/restore guards; Changed regression quality, no new timing claim |
| 0cb871d9 | GUI unavailable-body information/Reload; Fixed preview information |
| e7e966b6 | Guard unresolved non-regular link targets; Fixed preview worker safety |
| 4fd7ffb3 | Official artifact-upload action7.0.2 pin; Changed distribution CI maintenance, no Rust dependency change |
| 5866b979 | Update commands tolerate valid color preferences; Fixed CLI update alias |

## Shipped predecessor capability

The actual v0.30.1 tag's rust/src/updater/manifest.rs lines73–104 accepts FlistWalker- and fw- asset families while rejecting unsafe names and invalid checksum rows. The downloaded public SHA256SUMS has exactly26 entries and SHA-25672551ce0621220f121cbe84497e7f4cfed329b393c8eb0cc39a5e2ce22c0ec0f, matching public release metadata. Registration is based on these shipped bytes, never on ordering of version numbers.

The new exact26 v0.30.1→v0.30.2 test failed before registration with `unsupported previous release capability: 0.30.1`; its subsequent self-test result is recorded in the preparation PR. Actual candidate/tagged/public manifests remain separate mandatory checks.

Only the root Cargo package version changes. Dependency versions/features/toolchain/release profile and license notices remain unchanged; actual archive/sidecar contents are checked for each new bundle. Existing product FR035/SP006+014/DES005+014/TC169 and FR045/SP025/DES026/TC221 trace is retained.

[Proposed single-version release body](RELEASE-BODY.md) is reconciled against every commit above and the v0.30.2 CHANGELOG section. Windows candidate verification is assigned through the user-requested local handoff; no native PASS or old PR-only waiver is claimed here.

## Local preparation verification

N-1 self-test PASS after the focused RED; host cargo test --locked --offline PASS (1600 library tests, unchanged19 ignored, all bins/integration/harness/doc tests); format and locked all-target clippy -D warnings PASS with no warnings. validate_change --quick PASS, repository contract and120 tooling unit tests. Independent preparation interim review: blocking/major0; minor1 ambiguous inert-color wording corrected. Final release review remains pending until actual artifacts/native/publication/closure evidence.
