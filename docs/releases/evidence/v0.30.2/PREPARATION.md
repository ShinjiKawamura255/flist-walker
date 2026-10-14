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

Initial preparation changed only the root Cargo package version. Its protected CI then detected newly published rustls advisories; the compatible TLS repair below updates the dependency and license notice. Features/toolchain/release profile remain unchanged; actual archive/sidecar contents are checked for each new bundle. Existing product FR035/SP006+014/DES005+014/TC169 and FR045/SP025/DES026/TC221 trace is retained.

[Proposed single-version release body](RELEASE-BODY.md) is reconciled against every commit above and the v0.30.2 CHANGELOG section. Windows candidate verification is assigned through the user-requested local handoff; no native PASS or old PR-only waiver is claimed here.

## Local preparation verification

N-1 self-test PASS after the focused RED; host cargo test --locked --offline PASS (1600 library tests, unchanged19 ignored, all bins/integration/harness/doc tests); format and locked all-target clippy -D warnings PASS with no warnings. validate_change --quick PASS, repository contract and120 tooling unit tests. Independent preparation interim review: blocking/major0; minor1 ambiguous inert-color wording corrected. Final release review remains pending until actual artifacts/native/publication/closure evidence.

## New advisory repair

Initial head dbd96ae347795d7645f0724e65b9cc774ecf6cd9 failed Cargo Audit in [CI38576500167](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/38576500167), job115895007386. RUSTSEC-2026-0346, 0347, 0348 and 0349 were published2026-10-14. [Official rustls0.23.46](https://github.com/rustls/rustls/releases/tag/v/0.23.46), published2026-10-14T09:51:22Z, fixes all four. Repair updates only rustls0.23.45→0.23.46 and its notice/security record; no ignore or weakened gate. Fresh validation and protected CI remain required; original preparation PASS does not cover this new dependency.

Repair local validation: four packaged metadata graphs preserve package sets, features, edges and licenses (Windows292/Linux328/macOS x86_64 304/arm64 303); rustls MSRV1.71 unchanged, three upstream license files present. Fresh audit1314 advisories/452 dependencies exit0 without vulnerabilities/warnings. Full locked Rust suite PASS1600 library tests plus bins/integrations/doc tests, unchanged19 ignored; locked all-target clippy -D warnings PASS. These loopback updater regressions are not a claim of live TLS endpoint verification; candidate CI and actual signed bundles remain separate gates.
