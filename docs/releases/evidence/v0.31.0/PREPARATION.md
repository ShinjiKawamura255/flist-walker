# v0.31.0 candidate verification

This packet records preparation and verification, not a published release. **Candidate generated; distribution/signature/N-1 checks PASS; release readiness BLOCKED.** Tag creation and release publication are outside the authorized task.

## Candidate identity

- Source: `3194f03c27f9a72f06a836dbd02e9141d8740a26`, clean `master == origin/master` when dispatched.
- Previous public release: `v0.30.2`, source `c54f0237c4c8b55cfd085dd1b97828b4859b897e`; latest-public API readback on 2026-10-18.
- Candidate: [39087986481](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39087986481), attempt 1, `workflow_dispatch`, `master`. Actual run head matches the source above.
- Artifact: [12122471715](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39087986481/artifacts/12122471715), `validated-release-bundle-v0.31.0-3194f03c27f9a72f06a836dbd02e9141d8740a26`, expires2026-11-01T02:28:37Z; ZIP85,936,185bytes, SHA256 `263226a5a005a09f80a0aaaf73a04c98374a1b02b8b254f49d1f22e194f9a03d`. [Binding](CANDIDATE-BINDING.json) carries the manifest/signature/Windows binary digests.
- Evidence branch: `codex/v0-31-0-candidate-evidence`; new-change preflight PASS. Evidence additions do not alter the dispatched source. Other worktrees remain untouched.

## Complete release selection

Selection uses `validate_change.py --base v0.30.2 --plan`, every commit through the candidate, and the conditional VM details. Selected VM-001/002/005/006/008/009 are reconciled below. Ordinary execution uses the existing release procedure and this single packet; no product implementation, publication transition, or new release policy is introduced.

| Check / axis | Disposition | Original evidence / source boundary | Current action |
| --- | --- | --- | --- |
| Version, root lockfile, dated CHANGELOG | RUN | PR189 protected preparation; dependencies/features/toolchain unchanged | Confirm 0.31.0 identity and complete delta |
| Linux/macOS/Windows locked tests, all-target clippy, security audit, release builds | RUN | Current master CI [39086272030](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39086272030) succeeded; candidate conditions remain separate | Candidate run above; no duplicate local full suite |
| Actual inventory, archives, current license/notice/README, architecture, Windows PE resources/imports | RUN | A new build requires actual asset inspection | Bind all 28 assets / 26 manifest entries |
| Detached signature, predecessor verification key, eight embedded public keys | RUN | Previous bundle success cannot certify new bytes | Verify new manifest using the actual public v0.30.2 binary key |
| N-1 grammar | RUN | Shipped v0.30.2 parser accepts both FlistWalker-/fw- families; PR189 adds explicit capability and exact26 positive plus non-increasing negatives | Self-test PASS before dispatch; real generated manifest still required |
| All candidate logs and annotations | RUN | Prior run exceptions are not inherited | Inventory actual warnings, fresh official pin/runtime/input verification; independent review before any exception |
| Dialog footers / narrow height / confirmation layout, macOS native | RUN — NOT RUN for this build; earlier PASS retained | [PR186](https://github.com/ShinjiKawamura255/flist-walker/pull/186), tested ab8f50be43779d227902ae49af9d620177fcba12, executable SHA256 4e787ca95bc925c882e0aa0aa3038367532ea52d99b2f5a63fd4a86bc00f42db; normal/dirty/reload/save/cancel, 640x400 and body scroll | Subsequent settings additions require adjacent Settings checks; Windows adapter remains fresh RUN |
| Query restoration and saved theme, macOS native | RUN — NOT RUN for this build; earlier PASS retained | [PR187](https://github.com/ShinjiKawamura255/flist-walker/pull/187), tested dc62cdb6f24e22f42e6f697faba32596ca9bb140: two roots/filters/active tab, query off/on, disabled child retention, save/cancel/restart, Light/Dark saved rendering | Later production change only results palette; no claim of OS appearance-change delivery |
| Light/Dark results, macOS native | RUN — NOT RUN for this build; earlier PASS retained | [PR188](https://github.com/ShinjiKawamura255/flist-walker/pull/188), isolated executable SHA256 eda25aafaa0c02c84c9bcc6c06c6385b7d609573c7ec9f8d1f1d80d6e066949e: search match, FILE/DIR/LINK, normal/selected/current row, mouse/Down | Later Rust changes are test-only; version changes only root package identity |
| Changed dialogs/settings/themes/results, Windows native | RUN — NOT RUN | No Windows native evidence for the new behavior | Prepare exact candidate handoff; native desktop/session authorization is required before launch |
| System follows actual OS theme changes; fixed themes stay fixed | RUN — NOT RUN | Deterministic tests and initial System rendering do not prove native OS notification delivery | Prepare Windows steps; macOS delivery also remains unverified |
| TC-193 Windows startup performance | RUN — NOT RUN | v0.30.2 ratio is historical only; shared runtime-config initialization changed in PR187 | Fresh exact candidate, fixed200 files, 5 warmups +25 samples per variant, unchanged ratio <=0.70 |
| Search/indexing/runtime performance unrelated to startup | REUSE, selected scope | Retained [v9 evidence](../../../history/warm-reclaim-v9-2026-10-18/measurement-diagnosis.md), measured source63fc68aa72b873d1c8181f3d3f6cf37cfc94d9c5, prospective five whole workflows PASS | No production indexer/search/worker coordination delta; retain original source identities, no new timing claim |
| VM-003 long indexing guards / VM-010 soak | NOT REQUIRED | No production indexing or soak implementation change in this release range; test-driver calibration retains its own evidence | No unrelated rerun |
| Full native GSM matrix, IME, multiple DPI/displays, real Open/Reveal/update/clipboard/UNC | NOT REQUIRED | Outside affected flows | Do not promote old residuals into new requirements; no previous-version deviation is transferred |

## Delta ownership

| Commit | Responsibility |
| --- | --- |
| 4b0fd92 | Historical v0.30.2 publication evidence |
| 63e0bc8 | Dialog action placement and ordering |
| 5c4c9ee | Query restoration and System/Dark/Light preferences; shared config bootstrap |
| be6f76c | Light result contrast; history test readiness |
| fae5ec4 | Real deferred Warm allocation observation and unique test roots |
| 689d9a8 / 7732cba / 5c26783 / 1ae6ca0 | Benchmark reference/schema admission and retained original evidence; no production performance change |
| 3194f03 | Version0.31.0, complete CHANGELOG and shipped0.30.2 N-1 capability/regression |

No dependency, feature, toolchain, updater production parser, build flag, workflow, packaging or license-content change is introduced. Existing product trace remains owned by the SDD; this task produces evidence and a Windows handoff, so product TDD is inapplicable. Verification checks actual artifact identity/content and preserves NOT RUN on native/performance axes.

## Remaining gates and authority

Candidate artifact and warning verification are complete at the operator level. Windows native GUI and TC-193 are preparation only. No release GO is claimed. The release procedure requires independent review of the release selection and any exact-run external Action warning exception; that review remains pending. No exception is granted. No previous warning exception or v0.30.2 native deviation applies automatically.

No tag, draft release, public release, repository setting, trusted workflow or installed application is modified by this task. Candidate files are retained separately from installed binaries. If a check fails, keep the original run/raw result and diagnose before generating another candidate; never retry merely to obtain green.

## Supporting identity readback

The retained PR187 staged executable was read without launching it on 2026-10-18: SHA256 `0d6399244c05cde84fbe7053b929f27acd6383426d7f13fa104df483870bcdec`, size95,370,736. The retained dated report identifies base63e0bc8 plus the task patch, isolated profile/native eframe/CUA procedures, PASS scopes and NOT RUN OS notification. PR187 preserves the source head and states that the staged hash matched its tested build; the numeric digest above is a current retained-file readback, not a claim that the PR originally printed it. Independent review must assess this provenance and candidate build reconciliation before final native reuse acceptance.

The macOS build script explicitly invokes `rustup run stable cargo`; workflow preflight selects1.97.1 separately. Actual release binary `/rustc/<commit>/` provenance is inspected after download. Do not infer the release compiler from the workflow pin alone. Native reuse remains conditional on the resulting component/build comparison; a new relevant drift reopens the affected checks.

Windows helper preparation: four local mock checks PASS (all60 samples retained, unchanged0.70 threshold rejects equal medians, partial output failure retained, previous output refuses overwrite, non-Windows timing refused). These checks exercise the preparation helper only; Windows execution and TC-193 remain NOT RUN. The helper derives from the retained v0.30.2 protocol with identical timed argv/loop/warmup/sample/threshold; changes bind the new identity and strengthen observation/overwrite protection. Native Windows TDD is unavailable on this host, so focused stdlib unittest mocks check those preparation contracts; they do not replace the eventual native measurement.

## Current master CI warning findings

[Master CI39086272030](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39086272030), attempt1, source3194f03, completed successfully in all10 jobs. Full job logs and all check annotations were retrieved; [MASTER-LOG-REVIEW.json](MASTER-LOG-REVIEW.json) retains the three actual emissions and runner identities. Checkout hints, `-D warnings` arguments and a test name containing `deprecated` are scan noise, not additional emissions.

| Actual warning | Source / consequence | Required disposition |
| --- | --- | --- |
| Rust default toolchain implicitly overridden to1.97.1, Clippy and Coverage, 2026-10-18T01:23:26Z | `.github/workflows/ci-cross-platform.yml:358`, implicit `cargo install cargo-llvm-cov --version 0.8.7 --locked --force` executed from `rust/` | Unresolved. This is not an external Action warning and cannot use its minor exception. Explicitly select the intended pinned compiler for tool installation and validate the trusted-policy change through its authorized route. Do not suppress the warning. |
| Same Rust toolchain warning, Cargo Audit, 2026-10-18T01:23:24Z | `.github/workflows/ci-cross-platform.yml:315`, implicit `cargo install cargo-audit --version 0.22.2 --locked --force` from `rust/` | Unresolved. Release workflow already uses `cargo +1.97.1 install` from the root, but normal CI and the analogous scheduled audit install remain inconsistent. Fix/verify separately; candidate success does not erase this source-CI observation. |
| Node DEP0005 Buffer(), Windows GNU Updater E2E, 2026-10-18T01:27:24Z | Fixed download-artifact v8.0.2 extraction. Official stable/full bundle source is unchanged and diagnostic evidence exists; current CI product/build/E2E passed | Independent exact-run disposition remains pending. No old exception is inherited; any candidate-run occurrence receives its own record. |

The two Rust warnings report real toolchain selection, not product compile diagnostics or new vulnerabilities. Their installation jobs and downstream tests succeeded, but green is not warning acceptance. No workflow/checker/required-check/repository-setting change is made in this verification task. These unresolved warnings preclude a claim that release readiness is complete.

## Observed macOS compiler drift

The completed arm64 producer's universal and fw binaries both contain `/rustc/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/`. The [official Rust commit](https://github.com/rust-lang/rust/commit/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4) identifies the1.99.0 stable release. Required CI policy is1.97.1 (`8bab26f4f68e0e26f0bb7960be334d5b520ea452`), which the native preflight actually installed/tested. The actual shipped v0.30.2 arm64 binary contains48a229ceaefd4985c50990b14116b6d856af0985 (1.98.1), so the effective release compiler also changed across releases despite unchanged source pins.

Cause: `scripts/build-rust-macos.sh:34` explicitly chooses `rustup run stable cargo`; this takes precedence over the repository toolchain file. Installing1.97.1 in the workflow does not turn the separately installed `stable` alias into1.97.1. This is an actual asset/policy discrepancy, not merely a theoretical source concern. Verify x86_64 and final bundle bytes separately when available.

Required repair before promotion: make the macOS build entrypoint honor the repository's pinned compiler, cover explicit/implicit target invocation without silently falling back to another compiler, run its focused guards, and generate a new candidate from the protected merged fix. Do not update the pin, change trusted workflow structure/settings, or relabel these binaries as1.97.1 to accept this candidate. No such repair or replacement candidate is performed within this verification task.

The earlier macOS native PASS observations retain their original identity; proposed reuse is **not admitted for this candidate** while the effective build boundary is inconsistent. Reconcile only the affected checks after repair. Byte integrity, signature and N-1 results remain separate from compiler/native eligibility.

## Completed candidate results

| Gate | Result | Evidence / practical limit |
| --- | --- | --- |
| Exact source and whole workflow | PASS |10 jobs SUCCESS; Create Draft Release SKIPPED. Source3194f03, manual/master, attempt1. |
| Four producer ZIPs + final ZIP | PASS | [ARTIFACT-INTEGRITY.json](ARTIFACT-INTEGRITY.json): all5 public API digests, sizes and ZIP CRC verified. Slow transfer was replaced by exact HTTP ranges plus an immutable prefix; the complete reconstructed ZIP matches the original declared SHA256. The redundant owned transfer was stopped only after full verification; its partial bytes/log are retained locally. |
| Producer → candidate bytes | PASS | [PRODUCER-BYTE-MATCHES.json](PRODUCER-BYTE-MATCHES.json): all26 distribution files byte-identical to their producer outputs |
| 28 assets / 26 strict checksums / architecture / archives / sidecars | PASS | [CANDIDATE-VALIDATION.json](CANDIDATE-VALIDATION.json), [SHA256SUMS](CANDIDATE-SHA256SUMS.txt): exact inventories, all hashes, current LICENSE/notice bytes, README version/placeholders/member bytes, app metadata/icons/executable modes, no fw in universal archives |
| Update signature and trust continuity | PASS | Actual manifest verifies with the actual public v0.30.2 embedded Ed25519 key; same key embedded in all8 candidate binaries. [Predecessor trust](PREDECESSOR-TRUST.json), signature hex alongside manifest. Actual arm64 universal/fw `--version` probes PASS using disposable child HOME/self-update disabled; no GUI launched. |
| Real N-1 manifest | PASS | [N-MINUS-ONE.json](N-MINUS-ONE.json): actual signed manifest accepted for v0.30.2→v0.31.0; no compatibility exception |
| Windows PE | PASS | [CANDIDATE-WINDOWS-PE.json](CANDIDATE-WINDOWS-PE.json): x86_64, GUI2/CUI3, icons/group-icon/version/manifest/asInvoker, prohibited runtime imports absent, fw GUI imports absent |
| OS code signatures | INSPECTED; separate policy | [OS-SIGNATURE-INSPECTION.json](OS-SIGNATURE-INSPECTION.json): arm64 valid linker ad-hoc signatures, x86_64 unsigned; no Developer ID; no Windows Authenticode table; notarization NOT RUN under the existing temporary policy. This is not a claim of Developer ID/Authenticode signing. |
| Locked tests/clippy/audit | PASS | Candidate Linux/macOS/Windows native preflight and fresh Cargo Audit succeeded. Current source CI/coverage also succeeded; no duplicate local full suite. |
| Required compiler | FAIL | [COMPILER-PROVENANCE.json](COMPILER-PROVENANCE.json): Windows/Linux1.97.1; Mac arm64 **1.99.0**, Mac x86_64 **1.98.0**, both universal/fw. Actual compiler pin is not honored on macOS. |
| Candidate warnings | BLOCKED | [CANDIDATE-LOG-REVIEW.json](CANDIDATE-LOG-REVIEW.json), [disposition](CANDIDATE-WARNING-DISPOSITION.json): actual arm64 `Atomic::fetch_update` warning at `rust/src/app/worker/channel.rs:58` plus its aggregate summary, and one external Action DEP0005. Two capacity notices are not warning/error annotations. Rust warning is ineligible for the external Action exception; no warning exception granted. |
| Master source CI warnings | UNRESOLVED | Two implicit compiler-selection warnings during tool installs, plus separate DEP0005;10 complete job logs and all annotations reviewed above |
| Windows native / TC-193 | NOT RUN, PREPARED | [Windows handoff](windows/HANDOFF.md), exact binding, stdlib helper, deterministic CI evidence and return template. No native PASS or timing claimed. |
| Tags/releases/master readback | PASS scope boundary | [FINAL-STATE.json](FINAL-STATE.json): master still3194f03, latestpublic0.30.2, v0.31.0 tag/release absent; no settings/installed app/OS appearance changes |

Next repair is the macOS build compiler selection and explicit tool-install selectors in normal CI, through the approved PR/trusted-policy route. Preserve this original candidate and warning record. A replacement candidate must carry its own source/run/artifact and fresh verification; it must not relabel these results. Independent review and the selected native/performance gates remain open. Windows preparation does not authorize launching a native session or changing OS appearance.

## Follow-up repair boundary

The original packet above describes run39087986481, not a rebuilt candidate. Independent review of original evidence117e9f0 confirmed the artifact/signature/N-1 results and identified one minor helper defect: a timed-out process lost its partial output. [Repair validation](REPAIR-VALIDATION.md) records the helper correction, macOS compiler-selection correction and regression checks. Windows handoff revision r2 includes the repaired helper with the original Windows candidate bytes. [Original archive identity](windows/HANDOFF-ARCHIVE.json) remains unchanged; [r2 identity](windows/HANDOFF-ARCHIVE-r2.json) is separate. The candidate remains BLOCKED; original compiler/warning observations are never relabeled by the source repair.
