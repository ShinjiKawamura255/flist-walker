# Publication validation

Completed readback on 2026-10-07T12:40:14.904221+00:00. [Public v0.30.1](https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.30.1) is Latest, non-draft and non-prerelease, release ID `405723931`, published `2026-10-07T12:38:32Z`. Annotated tag object `8a7b785e95411a4095c98e381a69d8781823e42b` peels to `7f9a7a3a6d191cf2bd876453f1beec33838b394d`. It was created/pushed once after candidate acceptance. Existing tags/releases/assets were not overwritten or deleted.

## Exact runs and fresh distribution gates

Candidate [run37610130218](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37610130218), attempt1, artifact `11479315899` (expires `2026-10-21T11:21:00Z`), ZIP SHA256 `bcdb12fc202a7bdee6817a7d90dcb526def85e1085b1a89d1c39cfd564c037c8`. All10 execution jobs PASS; draft creation skipped. [Candidate validation](CANDIDATE-VALIDATION.json), [review](CANDIDATE-REVIEW.md), [per-run warning disposition](CANDIDATE-WARNING-DISPOSITION.json).

Tagged [run37616202550](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37616202550), attempt1, artifact `11481014258` (expires `2026-10-21T12:11:58Z`), ZIP SHA256 `ecf3514f4734ec925faf1aff427dfc616d4b24314a572e31d5402cf200261c5a`. All11 jobs PASS, including draft creation. [Tagged validation](TAGGED-VALIDATION.json), [review](TAGGED-REVIEW.md), [separate per-run warning disposition](TAGGED-WARNING-DISPOSITION.json).

Both use exact released source `7f9a7a3a6d191cf2bd876453f1beec33838b394d`. Actual native Linux/macOS/Windows locked tests and all-target clippy -D warnings and fresh cargo audit completed successfully. All28assets/26manifest entries/hash/64-byte Ed25519 signature/all8binary embedded keys/predecessor key/N-1/archive exact members and notice/version/architecture/executable modes passed independently. Key fingerprint `37148bac1533e1a08ed52c81e350ac4d614deff2374713e370c2bb7ea96b6757`. Windows universal PE GUI subsystem2, fw console3, AMD64 resources/asInvoker/import contract PASS. Mac app metadata differs from actual v0.30.0 only in the two version fields. Owned ARM universal/fw --version returned0.30.1; no GUI/config/network initialization was reached.

The actual12 Windows GCC/binutils packages and Rust compiler equal public v0.30.0 tagged run37116511150. Unchanged CLI/shared-engine/dependencies/profile/build contracts do not trigger fresh TC-193 timing. Historical LOSGATOS ratio0.46796 retains its original identity; new version probes/static inspections are not timing or headful certification.

## Draft and public route

Final draft was independently reviewed with0 critical/high/medium findings, bound to release ID/source/tagged run/attempt/body hash/actual asset identity hash in [draft review](DRAFT-REVIEW.json) and [independent record](DRAFT-INDEPENDENT-REVIEW.md). Actual28 asset IDs/names/sizes/API digests matched the tagged inventory; release body/stable download links reviewed. Published the same draft once as Latest. Body SHA256 `03a7105aa48225eed898133725379410a5c85f196a8b0e6be20d4220157393a4` equals [retained body](RELEASE-BODY.md). [Public asset readback](PUBLICATION-ASSETS.json) records all28 IDs, sizes, digests and stable URLs.

Authenticated metadata and fresh unauthenticated latest-release feed identify this same release. Every asset was downloaded via its unauthenticated HTTPS public URL and matched tagged bytes/hash. The downloaded manifest and signature were reverified using both the new and actual public predecessor embedded key; strict actual v0.30.0→v0.30.1 N-1 manifest PASS. Exact GUI/fw variants remain available. Installed applications were not replaced or launched for an update transaction. [Public readback](PUBLICATION-READBACK.json).

## Warning boundary and reuse

Candidate and tagged emissions were separately inventoried after complete logs, with current official latest/full SHA/runtime/actual runners checked on each run. Only the bounded, independently diagnosed ZIP extraction DEP0005 case is conditionally accepted under [canonical policy](../../../RELEASE.md#external-action-warning-disposition). No old waiver is inherited; suppression, continue-on-error, verification weakening and unknown/major allowance were not used. Retained full unmodified Action diagnostics distinguish local causal trace from hosted observation and do not certify HTTPS/auth/four-way parallel internals.

Whole48-commit release selection includes VM-001/002/003/004/005/006/008/009/010. Functional/full17 and selected native reuse retains original source/run/binary/session identities and no-impact boundaries in the packet and [native evidence](NATIVE-REUSE.md). Full GSM010/IME and Windows headful NOT RUN are not relabelled PASS or newly waived. macOS remains unnotarized, explicitly disclosed in the public body.

The intermediate source8a15c05 masterCI37606404807 failed before Mac tests/clippy/build at Install Rust (static.rust-lang.org checksum TCP timeout/os error60); skipped steps are not PASS. Exact released7f source masterCI37609742102 and both fresh release runs now PASS. No stale 8a CI or original stopped candidate substitutes for these checks.
