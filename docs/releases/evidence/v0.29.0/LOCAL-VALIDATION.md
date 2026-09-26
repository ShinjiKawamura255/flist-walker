# v0.29.0 Validation Evidence

## Identities

- Source commit: `ec4d9b8b12e0b92a5a441f45016f55621faac93e`
- Annotated tag object: `47af1388692dd7e2e4ac33e459dcf589a805e281`
- Accepted candidate run: `36237306042`
- Tagged run: `36248617696`
- Tagged validated bundle artifact: `10908955593`
- Published release ID: `397273244`

## Automated Validation

- Candidate and tagged workflows completed successfully on the exact source commit.
- Linux, Windows, macOS x86_64, and macOS arm64 release builds passed.
- Locked tests, locked clippy with warnings denied, cargo-audit, bundle assembly, signature validation, archive/sidecar checks, and draft creation passed where applicable.
- Tagged bundle readback: 28 files, 26 valid checksum entries, 64-byte signature, zero missing or unlisted payloads.
- N-1 compatibility passed against v0.28.0 using the exact tagged manifest.
- Published release readback: non-draft, non-prerelease, latest, exact reviewed body, 28 assets, zero name/size/digest mismatches.
- Protected PR exception: PR #150 run `36079919522` attempt 1 failed macOS TC-183 at seed `0x1838`, step 110, on unchanged head `4e03a598618d2909cafe5089884dcea4db1df468`. The user authorized one failed-jobs rerun only; attempt 2 passed. Diagnostic PR #151 and 50 local Windows seed replays did not reproduce the failure. The cause remains unknown, and focused independent evidence review passed before PR #150 merged as `29907c149acd41957f8516ea8cf90128806544b9`.

## Windows GUI Validation

- Accepted GUI executable SHA-256: `fd9b3fb62e3b56544ddb560a9330282680ad775eb9143ebf8ce399fc5f2bcee6`.
- Deterministic GUI suite: 14/14 groups PASS.
- Native representative addendum: 20,011 entries, search operators and recovery, tab behavior, 601-line Japanese/long-line preview, CSV/TSV column colors in both themes, tooltip, isolated settings save/restart, and two-tab query restore passed.
- Changed-file Reload: activation-confirmed reobservation changed the rendered selected-file body from the original 68 B state to the expected 24 B state; PASS.
- Color toggle: mouse PASS; keyboard FAIL. The formal FAIL and v0.29.0-only exception are retained in the release record and public body.
- Residual Windows axes listed in the release record remained NOT RUN under the approved one-time deviation.

## macOS GUI Validation

- Native GSM-001..013: NOT RUN under the explicit v0.29.0-only waiver.
- CI tests and both packaged macOS builds passed; they are not treated as native GUI PASS.

## Warning Inventory

- Candidate run: one external `actions/download-artifact@v8.0.1` Node `[DEP0005] Buffer()` warning.
- Tagged run: the same external warning twice plus one rustup repository-toolchain selection warning during cargo-audit installation.
- PR #150 run `36079919522`: one external `actions/download-artifact` Node `[DEP0005] Buffer()` warning, accepted only for that exact PR run after attempt 2 passed.
- Each exception was explicitly accepted for its exact run and v0.29.0 only. All affected download and audit jobs passed, and downloaded artifacts passed subsequent integrity checks.
