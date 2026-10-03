# v0.30.0 Candidate, Tagged and Public Verification

Recorded 2026-10-03. The [execution packet](../../v0.30.0.md) owns gate decisions. [PUBLICATION-ASSETS.json](PUBLICATION-ASSETS.json) owns the exact public 28 asset IDs, names, sizes, SHA256 values and downloaded URLs.

## Source and workflow identities

- Previous public release at dispatch and immediately before publication: v0.29.0.
- Accepted source: `129ec84cbf24ffb02bba1cade5b75fa042fd0c45`; complete range `v0.29.0..v0.30.0` has 18 commits. The reviewed GUI source patch is byte-equivalent through the protected PR164 merge; later policy/record edits are Markdown only.
- Annotated tag object: `20e34bc6dcd95ad5be93894037c1ac3b46492640`, peeled source equal to the accepted source. Tag create/push and draft publication each occurred once. No existing asset/tag was overwritten.
- Candidate [run 37112693044](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37112693044): 10 successful execution jobs; Create Draft Release skipped.
- Tagged [run 37116511150](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37116511150): all 11 jobs successful, including draft generation.
- Both runs passed Linux/macOS/Windows native locked tests and all-target clippy with `-D warnings`, fresh cargo-audit, four asset builds and assembly/validation. Tagged audit loaded 1,290 advisories and scanned 452 dependencies. Advisory warnings: 0.

## Actual artifact and signature checks

| Identity | Actual downloaded ZIP and retention |
| --- | --- |
| Candidate artifact 11270267335 | 85,581,279 bytes; SHA256 `5eb0add96e632afb2f895a91f552dc616184acbd5eafdefdeec05c10e8ee5608`; API digest equal; expiry 2026-10-17T09:43:35Z |
| Tagged artifact 11272506197 | 85,581,359 bytes; SHA256 `3da436db6ab695eb20d3fd30d298deb0dff329e733b79cd8df69227b317f8506`; API digest equal; expiry 2026-10-17T10:50:47Z |

Each actual bundle contained28 assets and26 checksum entries. All hashes, version/metadata, Windows AMD64/resource/asInvoker/subsystem2GUI and3fw, fw no-GUI-framework imports, exact universal archive members, standalone/archive byte equality, source LICENSE/THIRD_PARTY_NOTICES and sidecars, and Mac app embedded executable/version passed. App bundles themselves were not attached.

Each actual manifest passed Ed25519 verification using the new ARM embedded key and the immutable published v0.29.0 embedded key. Both key fingerprints are `37148bac1533e1a08ed52c81e350ac4d614deff2374713e370c2bb7ea96b6757`. Actual manifest N-1 compatibility passed separately from the reused unchanged checker self-test.

Candidate and tagged source are identical;18/28 asset bytes are identical. Released ARM executable SHA256 `33b8ec5bba1589b9e508fb573ac856a35f09063a7ebe85ae8921387fc131a117` equals the candidate executable. Windows universal SHA256 is `2fd73a785062802ed8f36b69bc896aca2e4250df1ee0a59528839b93b70067c8`; fw is `cb9434efb750a142dbb84b196cd4582535047d5339dc1da8addefbff28c1f226`. Differing Windows byte hashes are not asserted reproducible; their cause remains unknown. Effective Rust 1.97.1 and mingw/binutils package versions matched the candidate and older verified build. Subsystem/import/source/profile/features/startup comparisons support TC-193 reuse; no new measurement or weakened threshold is claimed.

## Native and change-time reconciliation

The [dated GUI repair record](GUI-REPAIR-VALIDATION.md) preserves original FAILs, exact local `705aba…` binary/patch `44b4ea…`, owned fixtures/profiles, two supervised sessions and selected Mac PASS. That local-native evidence is REUSE for released `33b8ec…` after source/toolchain/features/profile/dependencies/GUI-adapter and app metadata reconciliation. It is not a claim that the released executable was GUI-operated. Both release Mac Info.plist files equal the original official candidate, SHA256 `b6932ff9653b831dd526e4f68dd36eaf8b35ccfab8ce749c7faaef5463e7704b`; updater key embedding was separately artifact-verified, and native sessions disabled updater.

Windows GUI remains NOT RUN / ACCEPTED WITH DEVIATION under the user's v0.30.0 Computer Use waiver. Original Windows TC-193 LOSGATOS ratio0.46796 PASS is retained; Mac did not reacquire raw samples. Old Mac text saying TC-193 unperformed does not supersede this Windows result. Other historical unselected native debt stays NOT RUN, without creating a full-matrix release gate or hiding known defects.

Successful baseline functional/index/search/endurance/performance and applicable native subflows were reused under the whole-release selection. GUI repair required fresh Mac confirmation. Current source CI and new artifact/public identity checks were RUN. The policy change in PR163 remains the durable process correction: perform behavior checks at change time, rerun affected subflows and new artifact gates, preserve distinct RUN/REUSE/NOT REQUIRED/DEVIATION and PASS/FAIL/NOT RUN states.

## Full warning inventory and approvals

Candidate full10 root logs and all annotations: product build/test/clippy warnings: 0, cargo implicit-toolchain warning0, one external `actions/download-artifact` Node DEP0005 emission. The user approved only v0.30.0/run 37112693044/that one emission.

Tagged full11 root logs and all annotations: product warnings: 0, implicit-toolchain warning0, DEP0005 twice, once in Assemble Release Bundle and once in Create Draft Release. Two Mac capacity annotations were notices. The user separately approved v0.30.0/run 37116511150/these two emissions before publication. Neither exception carries to other runs or versions. The unchanged pinned Action is `3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c`. Artifact-download context is observed; prior diagnosis supports the cause, without claiming a current runtime stacktrace. Reevaluate at the next Action pin promotion; do not suppress logs or broaden the exception.

## Draft and public readback

Draft ID `402465964` contained the exact28 validated asset identities. Independent review found one major issue: copied draft-only `untagged-*` download links. All28 links were changed to stable v0.30.0 URLs using actual asset names; only the draft body changed. Focused re-review passed, with no remaining findings. The retained public body SHA256 is `00f8c9fd28e16efacc31490dc41124f0dc0a1e98249ca4a557c6befba3085ae7`.

Publication at `2026-10-03T11:11:56Z` converted that existing draft once. Readback completed at `2026-10-03T11:12:06.621336+00:00`: public URL, source/tag/release ID/body, non-draft/non-prerelease/Latest, all 28 names/sizes/API digests and stable URLs matched. All28 URLs were downloaded without authentication and matched the actual tagged files. Public `SHA256SUMS` hash is `c66b1caa383502cffcc1009d4bf1e63da7e4111f004fe89a7d3ad83df2c0a3f2`; signature is 64 bytes. Signature checks using both embedded keys and actual public manifest v0.29.0→v0.30.0 N-1 passed. No publication command was repeated.

## Retained operator evidence

The exact runs/public release and committed sanitized records remain retrievable after temporary artifacts expire. Local immutable operator archives retain raw logs/fixtures/profile evidence and all hashes:

| Archive basename | SHA256 |
| --- | --- |
| flist-walker-v0300-layout-native-accepted.zip | `dd6403d72b0b0bb953ca94beb3867ab33e1811ec07825fdab300808005307b99` |
| flist-walker-v0300-candidate-37112693044.zip | `04409342c48be543841f8141222063cce829086a6d2a98fecfedccc832dfbdfb` |
| flist-walker-v0300-tagged-37116511150-review.zip | `daba1e77bcdc9368d5ca7e871b2e7851b1e29561f4b64e6ec827c0398164c41c` |

The review ZIP records its then-pending warning state; the explicit user decision and public readback resolve it here. Archives are not overwritten. Raw screenshots remain in the CUA transcript; no nonexistent image files are claimed. Historical unintended CUA relaunch/profile impact remains unknown; the later monitored baseline comparison does not resolve that historical uncertainty. Current owned sessions preserved their baseline and clipboard, and both staged children were stopped safely.
