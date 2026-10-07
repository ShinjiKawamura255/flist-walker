# v0.30.1 — newly published download-artifact v8.0.2

The official latest changed while PR175 CI was running: [v8.0.2](https://github.com/actions/download-artifact/releases/tag/v8.0.2), published 2026-10-07T10:09:51Z, tag resolved to full SHA `9000827ccba6bdab643e8b6fd33ac0654aef8333`. Pre-dispatch refresh stopped before a new candidate was started. Source `8a15c0501aba9207f7be7a18915fb03c7ec6c96c` remains the protected warning-procedure merge, not an accepted release candidate.

## Source and impact

The official `action.yml` remains Node24 and is byte-identical to v8.0.1; digest-mismatch defaults to error. `src/download-artifact.ts` is byte-identical. `@actions/artifact` changes 6.2.1→6.3.1, with other Action-internal library updates and bounded HTTP429 retry/Retry-After handling. Runtime/source support must still be checked against each real hosted run; no product Cargo dependency, Rust compiler/profile, signature, updater parser, artifact selection/input, workflow structure or permission changes are proposed. Only the three existing full-SHA references/comment versions in the CI and release workflows change.

Official distribution Git blob `e5fe23803e0378f1f365aa0d6326cd288f5b660c`, 4,432,922 bytes, SHA256 `7834b909df0fd656f5ac1c4c4ab4ee3b5e2673e4f4f763549d8abbca16bc5bbe`. The old complete bundle identity differs, so its exception/diagnostic completion is not inherited. [Previous v8.0.1 record](EXTERNAL-ACTION-DIAGNOSIS.md) remains historical.

## Fresh bounded full-Action diagnosis

The official bundle executes unchanged through its normal internal client/extraction entrypoint. Only metadata/transport points to owned127.0.0.1 fixture endpoints; the synthetic unsigned fixture token has no authority and is accepted only there. No credential, dependency install, warning suppression or source replacement is used. The ZIP is the retained verified original candidate37546901634 (`c8671961947a7338c2098b204b0257117c85643dd5352317af69dd979c4b7f97`); this is a diagnostic fixture, not the new candidate.

| Case | Node / outcome | Warning and actual output |
| --- | --- | --- |
| Full bundle with trace, no preload |24.21.0 / exit0 |One DEP0005; all28 outputs byte-equal to verified ZIP payload |
| Argument-observation passthrough |24.21.0 / exit0 |One DEP0005;1391 empty-string constructors and28 numeric4-byte constructors; all28 outputs byte-equal |
| Same bundle/ZIP raw-download control |24.21.0 / exit0 |Zero DEP0005; raw ZIP byte/digest equal. skip-decompress is diagnostic-only, production inputs unchanged |

The trace is `new UnzipStream` at dist67720 (`new Buffer('')`) → `Extract`67427/67422 → `streamExtractExternal`126654. The unchanged unzip-stream numeric4-byte allocation at67982 is immediately filled in all4bytes by67983 `writeUInt32LE`. Observation preserves original arguments/return behavior; no uninitialized allocation data is exposed on the observed path. Official bundle hash remains unchanged before/after.

This bounded causal evidence supports assessment of the specific extraction case; it is not a hosted stack/HTTPS/authentication/four-way parallel certification or a general DEP0005 allowance. New hosted candidate/tagged logs, latest metadata, actual source/runtime/inputs, all28 assets/26 manifest entries/signatures/N-1 and independent per-run review remain required. New message/callsite/count/input/security/integrity/function uncertainty stops.

## Promotion boundary

[CI_OPERATIONS](../../../CI_OPERATIONS.md#pin-update-triggers-and-promotion) requires two consecutive **scheduled** canary successes for ordinary pin promotion, except its stated security/EOL/deprecation-deadline cases. v8.0.2 was just published; this record does not claim two such runs or a stated deadline. A draft pin-update PR can provide reviewable validation, but must not auto-merge/promote absent the required evidence or explicit user authorization for that precise waiting exception. Required CI Gate/Guardian, protected rebase, independent review and every release artifact gate remain mandatory. No settings/trusted-policy change is needed or proposed.

## Retained raw diagnostic identity

Operator material remains outside the repository; these hashes retain identity without publishing host paths or third-party bundles.

| Material | SHA256 |
| --- | --- |
| `dist/index.js` | `7834b909df0fd656f5ac1c4c4ab4ee3b5e2673e4f4f763549d8abbca16bc5bbe` |
| `full-action-probe.py` | `db28849620d8d5c18ee02bd41b4f154f47d6b56983142f61dcfe12d21555f73f` |
| `observe-buffer.cjs` | `540fe5e0daa43e3e6d24e2d149e9198d8e38f674b4592f65322b5d32f539d507` |
| `extract-trace/stderr.txt` | `52642d73c16c97dc73b41756f3e87544a6fcef54262d94ad580c01c56a851a9b` |
| `extract-argument-observation/buffer-calls.jsonl` | `bb1cade6622d785caef53d34e2edf9d066dbe53a90ad5cc5aca9365dd4ae692e` |
| `raw-control/stdout.txt` | `3fe43c3c04bc25a4064a9e370965d724b6d9e4e5849831bd9c1a5f55d277c744` |
| `requests.json` | `aecb1024aa71e31f1952b6004052ca34795950e4fefa3348721972c8b292dcd8` |
| `full-action-summary.json` | `b7422a3f0f52f9baf806dc73e873178995d3a610517fa37e6848843d71903f4e` |
