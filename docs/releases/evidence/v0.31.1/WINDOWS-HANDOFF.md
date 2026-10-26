# v0.31.1 Windows Codex handoff

## Objective and ownership

Windows native verification of Stop Indexing / Stop All Indexing availability. The release owner works on macOS and retains release/PR/tag/publication responsibility. Read [the release packet](../../v0.31.1.md), repository AGENTS.md, docs/AI_DEVELOPMENT.md, GUI-TESTPLAN.md and testplan/release-validation.md first. Report in the dedicated Windows verification chat; the release owner will collect the result with the connected app tools. The user registered LOSGATOS / D:/work/flistwalker and authorized this chat during preparation.

Product baseline is `93890b5521abcbfa98fbf192f42974b488ec25f1` (PR210). **It is not the version-bumped candidate.** Obtain the exact preparation PR/head SHA from the owner; preparation branch is `codex/release-v0-31-1`. Version must be 0.31.1. Never test an existing Windows v0.31.0 binary or a binary from another checkout.

Source builds may establish functional evidence before candidate artifacts exist. Actual candidate assets require their own run/attempt, identity/hash and startup verification after generation. Do not relabel one as the other.

## Safe source and build

```powershell
git status --short --branch
git worktree list --porcelain
git fetch origin
git rev-parse HEAD
git log -1 --format=fuller
```

Do not switch/reset/clean a dirty checkout or a branch owned by another chat/worktree. If the specified SHA differs, use an authorized managed worktree at that exact SHA; do not reuse an unrelated active checkout. In the selected clean evaluation checkout:

```powershell
python scripts/agent_worktree_preflight.py review
.\scripts\build-rust-win.ps1 -CheckOnly
.\scripts\build-rust-win.ps1 -NoInstall
$sourceExe = (Resolve-Path .\rust\target\x86_64-pc-windows-gnu\release\FlistWalker.exe).Path
Get-FileHash -Algorithm SHA256 -LiteralPath $sourceExe
```

Use installed prerequisites only; report missing tools rather than installing or changing persistent system settings without authority. Do not modify product sources, dependency/workflow policy, commit/push, tag or publish in this verification task. Report defects before implementing fixes.

**scripts/gui-headful-smoke.ps1 uses rust/target/debug/flistwalker.exe, even with -NoBuild. It does not verify the release executable above.** Its fixture/PID-window/staging/isolation/cleanup implementation is the reference, not a source-EXE override. Its ScriptedQueryProbe proves startup/Unicode initialization and window liveness, not full native interaction.

## Isolated release execution

Create a uniquely named run below rust/target/gui-smoke/. Stage the explicitly identified release executable plus ignore/example files; record the pre-launch allowlist, hash and updater-artifact absence. Use System.Diagnostics.ProcessStartInfo with UseShellExecute=false and argument arrays, following the existing headful script. Change environment only on that child:

- LOCALAPPDATA / APPDATA → owned run/profile/local; USERPROFILE → run/profile/home.
- Effective config folder: run/profile/local/flistwalker. No real user profile or installed application is a fixture.
- Disable self-update, history persistence and prior-tab restoration; route window trace into the run.
- Automatic FileList checking may be disabled in the owned profile so fixture rewrites do not shift the toolbar; report this configuration difference.
- Set theme dark/light in the owned runtime JSON and restart to apply. Seed stored window geometry for normal width and 640 logical px (minimum supported width); do not change the OS theme.

Use real local files and a correct root FileList for normal state. A finite blank-line FileList ending with the same few paths can extend parsing without a large candidate set; adjust within a bounded disk/time budget. Deliberate NUL in the owned FileList produces a fast failure. Restore the valid fixture afterward. Do not create infinite input or disrupt real network shares.

## Native cases

Use Computer Use for UI input/observation and identify the owned PID/window. Record input, visual state and liveness separately.

| Case | Expected |
| --- | --- |
| All tabs completed | Both stop controls stay visible and gray. Clicking either changes no stop notice/admission and does not pause future tabs |
| Current tab updating | Both controls enabled; current Stop retains last-good results/query/filter and then disables when no work remains |
| Only background tab updating | Current Stop disabled; Stop All enabled. The current-tab action must not stop background work |
| Stop All | Background work canceled; both controls disabled when work is gone. Tab switching/new tab does not restart it |
| Explicit Refresh | Resumes only its target; other paused tabs stay paused. Controls disable again on completion |
| Failed refresh | Old results preserved, failure visible, stop controls disabled. Repair fixture + explicit Refresh recovers |
| Input during indexing | Typing/deleting query, moving results, scrolling and switching tabs respond; stale terminal/data do not roll state back |
| Dark/Light and 640px | Enabled/disabled distinction readable; wrapped labels unclipped; controls are not removed/repositioned merely on start/completion |

A run that completes before observation does not prove the active state. Exact click/completion race reproduction may reuse TC-236's deterministic dispatch test, with native race NOT RUN if not actually reproduced. A Computer Use timeout is NOT RUN with its exact error, not proof of a locked desktop or product FAIL. WM_NULL/process survival alone does not prove typing or rendering.

## Candidate-asset follow-up

Wait for the owner's exact source SHA, candidate workflow run/attempt, artifact ID/digest, Windows asset name and manifest/signature binding. Extract into a separate owned run, verify expected SHA-256 and version/resource/subsystem identity, then perform an isolated startup smoke. The owner reconciles source-build functional evidence to the candidate's source/compiler/options; only affected behavior is rerun. New asset identity cannot reuse an old binary's hash/startup claim.

## Excluded unless new evidence changes scope

No unconditional TC-193 benchmark rerun: CLI/startup/index engine/dependencies/toolchain/profile are unchanged by this product delta. The candidate's new Windows PE/import/resource/sidecar inspections remain required through artifact validation. IME, alternate DPI/multiple displays, clipboard, external Open/Reveal, real share outage/recovery, updater transaction and full GSM/soak are not automatically selected. Retain existing NOT RUN; never use real clipboard contents or a production share for fixtures.

## Return contract and cleanup

Report source/candidate SHA, 0.31.1 version, build command/compiler/target, candidate run/attempt and asset identity when applicable, staged absolute path/SHA-256, OS/scale/logical dimensions/theme, isolated profile and fixture. For each case give expected/observed result, PASS/FAIL/NOT RUN and separate native-input/visual/liveness result with screenshot/trace/log identity. Failures include exact action/time and whether click dispatch/request/worker response was observed; unobserved cause stays unknown.

Close owned windows normally. Read back exact owned parent/children, unchanged staged binary hash, pre/post allowlist and updater artifact count, and whether any usual profile was touched. Never terminate apps by executable name alone. Do not retry a failure merely to erase its evidence. Raw screenshots/logs remain ignored under rust/target/gui-smoke/evidence/; sanitized release results are sent to the release owner for the single packet, not committed as a competing progress record.
