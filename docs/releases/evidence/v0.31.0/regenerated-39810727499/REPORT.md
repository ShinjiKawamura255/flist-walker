# v0.31.0 regenerated candidate — 2026-10-23 JST

**BLOCKED: fresh Cargo Audit found two vulnerabilities. No distribution was generated.** This record belongs to [run39810727499](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39810727499), attempt1, source `d1bf330fd599cc04a333fc903c64dec77fd50aee`, `workflow_dispatch` on `master`. The original [run39087986481 packet](../PREPARATION.md), bytes, failed compiler observations and Windows ZIP remain historical and unchanged. They do not certify this source.

The user requested regeneration, verification and Windows preparation. No tag, release, repository setting, installed application or OS appearance change is authorized here. The workflow was dispatched once; no retry or gate bypass was used. Evidence edits are on `codex/v031-regenerated-candidate-evidence`, after a successful new-change worktree preflight from clean synchronized master. They do not change the dispatched source.

## Audit stop

[Release Cargo Audit](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39810727499/job/119936063927) exited1 on 2026-10-22T16:08:09Z. It scanned452 lockfile dependencies against1349 advisories. Both findings are present in the candidate's `rust/Cargo.lock`:

| Dependency | Candidate version | Advisory | Patched version |
| --- | --- | --- | --- |
| rustls-webpki | 0.103.15 | [RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382), certificate verification may accept distrusted intermediates | >=0.103.16, <0.104.0-alpha.1, or >=0.104.0-alpha.8 |
| webpki-roots | 1.0.7 | [RUSTSEC-2026-0381](https://rustsec.org/advisories/RUSTSEC-2026-0381), bundled roots can trust distrusted emSign intermediates | >=1.0.10 |

The official advisories were read on2026-10-23 JST and agree with the hosted findings. `webpki-roots 0.26.11` re-exports its separately resolved1.x parent; its presence does not remove the affected1.0.7 dependency. Existing audit exceptions do not cover either advisory. This is a security failure, ineligible for an external Action warning exception. The candidate source was not changed to conceal or bypass it.

Before another candidate: resolve both dependencies to patched compatible versions through the normal protected PR path, review updater/TLS and OSS impact, run the selected VM-005/009 tests and fresh audit, obtain independent review, and use the merged source in a new run. Keep this failed run. A dependency repair and its PR are follow-up work, not an unpublished modification of this candidate.

## Verification results

| Check | Result | Evidence / limit |
| --- | --- | --- |
| Source/version/predecessor | PASS | Dispatch head matches masterd1bf330; Cargo/root lockfile0.31.0; latest public releasev0.30.2. v0.31.0 tag absent at preparation. |
| Current master CI | PASS with residual warning | [39808717920](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39808717920), exactd1bf330:9 jobs succeeded, Cargo Audit correctly skipped for the GUI-only diff. Fresh candidate audit is separate and failed. |
| Candidate native locked tests/clippy | PASS | Linux/macOS/Windows locked tests and all-targets clippy with `-D warnings` succeeded; preflights report Rust1.97.1. [CI review](CI-REVIEW.json) retains all actual jobs/steps, log hashes and annotations. No local duplicate full Rust run. |
| Fresh security audit | FAIL | Two vulnerabilities above; source-CI green cannot satisfy this gate. |
| 28 assets /26 checksum inventory, archives, sidecars, architecture, PE resources/imports | NOT RUN | Audit is a required dependency of asset production; no candidate artifact exists. |
| New manifest signature / all8 embedded keys / OS signatures | NOT RUN | No new binaries or signed manifest. No macOS compiler-provenance or code-signature PASS is inferred from source repair or preflight compiler alone. |
| Public predecessor trust | PASS, predecessor only | [PREDECESSOR-TRUST.json](PREDECESSOR-TRUST.json): fresh publicv0.30.2 manifest/binary hashes, unique embedded Ed25519 key, actual raw64-byte signature and tamper rejection; binary not executed. |
| N-1 self-test | PASS | `python3 scripts/test-updater-n-minus-one-compatibility.py`: explicit shipped0.30.2 capability and exact26-entry0.31.0 positive case/non-increasing negatives. |
| Actual candidate manifest N-1 | NOT RUN | No candidate manifest. Self-test and original candidate PASS are not substitutes. |
| Windows native GUI / TC-193 | NOT RUN, procedure prepared | [Windows handoff preparation](windows/HANDOFF.md); no runnable new handoff ZIP or binding can be issued without the new artifact. |
| Helper failure-recording regression | PASS, preparation only | Existing helper's5 stdlib mock tests succeeded, including timeout partial output, first-result retention and unchanged performance threshold. No Windows timing claim. |
| Release text completeness | INCOMPLETE | Existing0.31.0 section predates PR193–195 and omits initial Source status, tab-end clipping, font-dependent Root/shared-row clipping and multiline/offscreen-row repairs. Reconcile before promotion; no release text was rewritten into this failed candidate. |

## Complete release selection

`validate_change.py --base v0.30.2 --plan` selected VM-001/002/005/006/008/009. The complete range includes the original packet's settings/theme/footer/history changes, evidence/calibration work, macOS build/helper repair, Linux diagnostic/CI rollout, and PR193–195. Selected details and intent checklists were inspected. This task changes evidence/preparation only; product TDD and another full local Rust suite are inapplicable.

| Axis / responsibility | Disposition | Original evidence and comparison to this candidate | Remaining action |
| --- | --- | --- | --- |
| Every-release distribution/signature/N-1/current CI | RUN | New bytes require fresh verification; old packet identity never transfers. Current source CI above; candidate audit FAIL. | Repair security dependencies, then new source/run and all actual asset gates. |
| Settings restoration/themes/dialog footers/Light results | RUN, incomplete | PR186/187/188 native results retain their original identities in the old packet. Later shared-row/font/tab rendering in PR194/195 prevents blanket native reuse; current candidate bytes/build comparison also unavailable. | Changed and adjacent native subflows on macOS/Windows, not the complete GSM matrix. |
| Initial Source status and tab-end control visibility | RUN — native NOT RUN | [PR193](https://github.com/ShinjiKawamura255/flist-walker/pull/193): deterministic source/FileList/Walker/status and24 tab-scale/width conditions PASS; native appearance/liveness NOT RUN. Index scheduling itself unchanged. | Check detecting→source→ready, refresh old snapshot, stale/tab isolation and scrolled add button on a prepared native session. |
| Root typography / multiline labels / shared row layout | RUN — native incomplete | [PR194](https://github.com/ShinjiKawamura255/flist-walker/pull/194) and [PR195](https://github.com/ShinjiKawamura255/flist-walker/pull/195): deterministic typography/cache-count tests PASS; macOS static display PASS at original staged builds. Native list interaction and Windows rasterization/DPI NOT RUN. GUI automation timeout is a tool limitation, not a product FAIL. | Preserve those limits; new candidate identity/build reconciliation and affected native interaction still required. |
| Actual OS theme notifications | RUN — NOT RUN | Initial System rendering/deterministic state are not real OS notification delivery. | Separate selected native observation with exact session/OS-setting authority. |
| TC-193 Windows startup | RUN — NOT RUN | PR187 changed shared startup configuration; v0.30.2 timing cannot certify it. PR190 helper repair retains partial timeout evidence without changing protocol/threshold. | Fresh valid Windows artifact; fixed200 files,5 warmups+25 samples/variant, ratio<=0.70. |
| Index/search runtime throughput and WarmReclaim | REUSE in unchanged scope | [v9 evidence](../../../../history/warm-reclaim-v9-2026-10-18/measurement-diagnosis.md) keeps original source63fc68aa…; later production deltas change display/bootstrap preferences, not index/search scheduling or reclaim policy. No new throughput claim. | New TLS dependency repair must receive its own component impact review; do not pre-approve future changes. |
| Persistence/history readiness and flush | REUSE tests; native restore above remains selected | PR188 readiness plus PR190 final-patch-generation test repair; production persistence contract unchanged after PR187. Exact master CI exercises current tests. | No duplicate contention fixture solely for evidence docs. |
| macOS compiler / CI diagnostics and protection | RUN actual build; REUSE completed controlled rollout | PR190 merged compiler selector and source-bound Linux fault collection; [rollout record](../../../../history/linux-coverage-rollout-2026-10-19/diagnostic-validation.md) and PR191 proof preserve exact identities. Original SIGSEGV cause remains unknown; current coverage success is not an explanation. | Inspect actual new macOS binary compiler after an eventual successful build; no repository-setting changes here. |
| VM-003 indexing performance / VM-010 soak / full native matrix / external actions / UNC / IME / multiple displays | NOT REQUIRED for this task | No corresponding production responsibility change in the reconciled range. Native scale/font boundary checks are selected above; this does not assert full platform certification or erase historical residuals. | Preserve unselected debt without inventing PASS or expanding prior deviations. |

## Warning decision and completion boundary

[CI-REVIEW.json](CI-REVIEW.json) records exact run/attempt/job/step outcomes, complete available job-log hashes and all annotations. Actual warnings are separated from checkout hints, command arguments and test-name matches. No external Action exception is granted: security and artifact gates are incomplete. Old run exceptions do not transfer. Source CI's download-artifact DEP0005 remains a separate exact-run observation; it is not the cause of the audit failure.

Master CI also emitted one real Rust tool-install warning at `ci-cross-platform.yml:358`: the default compiler was implicitly overridden to1.97.1 by the toolchain file during cargo-llvm-cov installation. This remains unresolved and cannot use the external Action exception. The analogous audit-install job was skipped in this source CI, so silence there is not proof of repair. Resolving the workflow selector requires its separately authorized immutable CI route. Official download-artifact latest stablev8.0.2 resolves to the pinned9000827ccba6bdab643e8b6fd33ac0654aef8333 and declares node24; those facts alone do not prove the DEP0005 callsite/input/impact harmless. No new diagnostic or warning exception is claimed.

[Master coverage inspection](MASTER-COVERAGE.json) binds the diagnostic artifact's ZIP/report hashes. Synthetic self-test SIGSEGV stacks and the expected coverage abort are distinguished from real faults: this execution completed with coverage_exit0 and no real coverage SIGSEGV. It does not resolve the historical fault's cause.

No new Windows ZIP is represented as ready, no old binary is rebound, and no native GUI/TC-193 PASS is claimed. The final repository check and independent evidence review are recorded in [VALIDATION.md](VALIDATION.md). Candidate generation remains failed; release readiness remains BLOCKED.
