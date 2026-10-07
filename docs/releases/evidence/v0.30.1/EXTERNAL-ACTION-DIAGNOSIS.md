# v0.30.1 external Action DEP0005 diagnosis

Recorded2026-10-07UTC. This is a bounded causal diagnosis, not a security certification or a direct stack trace from the hosted release runner. [RELEASE warning disposition](../../../RELEASE.md#external-action-warning-disposition) owns the decision conditions. Every new candidate/tagged run retains its own latest-version/runtime/log/artifact/review gates.

## Fixed identities

- Original observed release run: [candidate37546901634](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37546901634), source `f9b65fbf9b071999876855017ce82fca8aa3c5b0`, Assemble Release Bundle job112558996052 / Download packaged assets. One actual DEP0005 emission, no Rust/test/clippy/audit warning or warning annotation. Existing stop under the old procedure is preserved.
- Official latest stable checked2026-10-07UTC: [download-artifact v8.0.1](https://github.com/actions/download-artifact/releases/tag/v8.0.1), exact original repository commit `3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c`, equal to the hosted pin. Later-published v3 maintenance releases are not the github.com latest stable. Action uses node24; required runner>=2.327.1, actual hosted assembly runner2.337.0. No pin update is necessary at this snapshot.
- [Official bundled dist/index.js](https://github.com/actions/download-artifact/blob/3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c/dist/index.js): Git blob `d7aade707a79a3bc4130f2605987cbbf2ecfac6c`, locally recalculated Git blob identity; SHA256 `f4a6b7046eb834bed98bb687f6d08feb0bfe6fb32570e9da7de0707884c08b31`, verified unchanged before/after all probes. Chain: download-artifact8.0.1 → @actions/artifact6.2.1 → unzip-stream0.3.4.
- Actual previously validated candidate ZIP: artifact11451841099, SHA256 `c8671961947a7338c2098b204b0257117c85643dd5352317af69dd979c4b7f97`, verified before the probe. Actual produced28 files and source notices have already passed manifest hashes/signature/N-1.

## Full Action execution and controls

The full official bundled Action was executed as its normal entrypoint using Node24.21.0, no npm install or source rewriting. It used the same internal ArtifactClient ListArtifacts / GetSignedArtifactURL → downloadArtifactInternal → streamExtractExternal → unzip.Extract path as the hosted default download. Only metadata/transport were supplied by a127.0.0.1 server. Its unsigned nonfunctional JWT fixture was accepted only by that local server; no real credentials or authentication/permission settings were read or changed. All writes stayed in owned probe destinations.

Three bounded cases were executed once each:

| Case | Observation | Integrity/result |
| --- | --- | --- |
| Full Action with `--trace-deprecation`, no preload | DEP0005 once; full stack below reaches actual bundled UnzipStream constructor | exit0; all28 extracted files byte-equal candidate; ZIP digest equal |
| Same full Action with additional observational Buffer Proxy | Reflect-passthrough calls retain original arguments and return behavior;1419 deprecated constructor calls observed |1391 empty-string calls;28 fixed4-byte calls; exit0 and all28 files byte-equal |
| Same Action/ZIP with `skip-decompress=true` diagnostic control | No DEP0005; the normal download/hash path completes without ZIP extraction | exit0; raw ZIP byte/digest-equal |

`skip-decompress` is only a diagnostic control. Production workflow/inputs, hash mismatch behavior, warnings, validation and permissions were not changed. The observational preload was not the source of the primary trace: the first case has none. No same-condition retries or warning suppression were used.

## Source, arguments and effect

The unmodified full Action stack identifies `UnzipStream` line67720: `this.data = new Buffer('');`, then Extract lines67427/67422, then `streamExtractExternal` line126472. Empty-string initialization produces a zero-length buffer, with no requested uninitialized payload bytes. The28 numeric4-byte calls in the observed extraction occur at line67982; line67983 immediately writes the fixed data-descriptor signature with `writeUInt32LE(...,0)`, initializing all4 bytes before this pattern is used. Other observed constructors initialize/clear an empty buffer in MatcherStream/UnzipStream. No uninitialized bytes are exposed through these observed calls. One Node warning does not mean one constructor invocation.

The hosted log pin, step, normal ZIP-extraction inputs and warning timing agree with this source path. The shared extraction code is independent of the loopback metadata adapter. This explains the original warning as legacy Buffer-constructor use in the unzip dependency; it does not establish a general DEP0005 allowance. Output/archive/digest/signature/N-1 equality and the initialized, fixed inputs bound the observed functional/integrity/security effect of this warning.

Sanitized primary full-Action trace:

```text
(node:<owned>) [DEP0005] DeprecationWarning: Buffer() is deprecated due to security and usability issues. Please use the Buffer.alloc(), Buffer.allocUnsafe(), or Buffer.from() methods instead.
    at showFlaggedDeprecation (node:buffer:224:11)
    at new Buffer (node:buffer:307:3)
    at new UnzipStream (file://<owned-probe>/dist/index.js:67720:17)
    at new Extract (file://<owned-probe>/dist/index.js:67427:24)
    at Object.Extract (file://<owned-probe>/dist/index.js:67422:12)
    at file://<owned-probe>/dist/index.js:126472:33
    at new Promise (<anonymous>)
    at file://<owned-probe>/dist/index.js:126440:16
    at Generator.next (<anonymous>)
    at fulfilled (file://<owned-probe>/dist/index.js:126348:58)
```

## Independent review and remaining boundary

Independent read-only release_prep_review inspected the full official source/hash, unchanged execution, stack, call records, outputs/digests, raw control and the original hosted logs. It accepted this particular warning as lightweight **bounded causal inference**. Source/inputs/effect are sufficiently identified for this case; the older empty-constructor snippet alone was not used as that proof.

This probe uses a local Node, loopback and one real ZIP. It does not observe the original hosted HTTPS/authentication or four parallel downloads, and the original hosted run has no direct stack trace. Those distinctions remain explicit. No full Action vulnerability assessment, HTTPS/auth certification or general warning-code safety is claimed. A different pin/runtime/input/step/callsite/message, missing verification, unexplained increase, security issue, digest/signature failure, asset defect or functional failure stops release and requires renewed diagnosis. New candidate/tagged runs must verify their actual artifacts/logs and independently record the current policy decision.

## Preserved operator evidence

Complete full-Action stdout/stderr, source, protocol adapter, Proxy code,1419 call records and requests are retained in the task-owned `warning-diagnostic/` collection; raw local paths are excluded from this durable summary. Probe material hashes:

| Material | SHA256 |
| --- | --- |
| `full-action-probe.py` | `b1fc513903c23558add96df618653239ba0f9307218bfc5acbcab49ffc47b789` |
| `observe-buffer.cjs` | `540fe5e0daa43e3e6d24e2d149e9198d8e38f674b4592f65322b5d32f539d507` |
| `extract-trace/stdout.txt` | `32fac5e5618b773a9450d70f1a89e18dc434d97de517766fc9c49c7f1c45b9cf` |
| `extract-trace/stderr.txt` | `181b741fdf8cf365830bcb45ee50b4024b80f2e1950fe2469b441e08b0723190` |
| `extract-argument-observation/buffer-calls.jsonl` | `3c83b7eb253b63e15dd3ed2498466668b5574425b55fbca8da3dcc14d8e22753` |
| `raw-control/stdout.txt` | `b17d89a6fb4baec976fa0104a84d417ea471281073fb5be5c2837b444bf76ceb` |
| `raw-control/stderr.txt` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `full-action-summary.json` | `8bd1b90683c6432a9878554332fda3fd7e3b3ef8aaabfbdd3e580d9f635f8e96` |
