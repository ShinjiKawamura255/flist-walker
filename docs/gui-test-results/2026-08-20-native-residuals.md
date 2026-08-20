# Native GUI Residual Result — 2026-08-20/21

## Scope
- Build: FlistWalker `v0.24.1`
- Base commit: `6da1b0029b14ee73fff96a03d463811631aface8`
- Branch: `codex/gui-native-residuals`
- Host: Windows 11, Japanese and English input methods installed, two 2560×1440 displays, both 96 DPI.
- Fixture: canonical `rust/tests/fixtures/gui-smoke` staged under the isolated GUI smoke root.
- Result: closed with residuals. This is not an all-axis GUI PASS and is not a release-candidate decision.

## Results

| Axis | Status | Evidence and exact disposition |
| --- | --- | --- |
| 10-minute staged liveness | PASS | Run `20260820T145550Z-33768-864d0496`; process stayed alive, fixture/allowlist remained valid, and no updater artifact appeared. |
| Tab read-back | PASS | New-tab action was read back as `Tab: 2/2` before shutdown; it no longer raced the harness timeout. |
| Japanese literal input | PASS | The benign literal `日本語` rendered in the staged query and the result view updated without freezing. |
| Japanese IME composition | NOT RUN | Composition was not attempted because staged-window-only input-profile/IME-mode switch, restore, and read-back were not proven. Literal input is not promoted to composition PASS. |
| Multi-display movement | PASS | Staged window origin moved from primary `x=163` to secondary `x=2903`, accepted further query input, then returned to primary `x=285`. |
| Alternate DPI | NOT RUN | Both observed displays reported 96 DPI, so no cross-scale transition existed. |
| Copy Path/clipboard | NOT RUN | Clipboard preflight reported four formats and sequence 39. A second probe remained sequence 39. No pre-existing content was read and no clipboard mutation occurred. |
| External open/reveal | NOT RUN | No disposable owned handler/session was available; the real default application and its MRU/window state were left untouched. |
| Isolated user-settings persistence | PASS | `show_preview=false`, tab/active state, and window geometry were written only below the staged profile. Restarting the same staged executable/profile restored Preview off and origin `(285,154)`; the staged window then closed normally. |
| Updater loopback failure path | PASS | Run `20260820T151544Z-16988`: literal `127.0.0.1`, no redirect, request observed, staged path confirmed, target hash unchanged, updater artifact count 0, process stopped, server stopped. |
| Updater signed apply/restart | NOT RUN | `FLISTWALKER_UPDATE_SIGNING_KEY_HEX` was absent. No key was synthesized or exposed; production feed variables were absent. |
| Real UNC | NOT RUN | No SMB mapping or filesystem drive with a UNC display root was present. No share was created and no unrelated network path was enumerated. |

## Safety And Raw Evidence
- Child `LOCALAPPDATA`, `APPDATA`, and `USERPROFILE` stayed below the staged run directory. Real user configuration was not read or changed.
- External open/reveal, clipboard mutation, input-profile switching, OS DPI changes, SMB share creation, production network access, and updater apply/restart were not performed.
- Local raw locators (ignored by git):
  - `rust/target/gui-smoke/evidence/GUI-DETERMINISTIC-20260820T151846Z-632.local.md`
  - `rust/target/gui-smoke/evidence/GUI-HEADFUL-SMOKE-20260820T145550Z-33768-864d0496.local.md`
  - `rust/target/gui-smoke/evidence/gui-headful-smoke-20260820T145550Z-33768-864d0496.log`
  - `rust/target/gui-smoke/update-loopback/20260820T151544Z-16988/evidence/LOOPBACK-UPDATE-PROBE.local.json`
  - `rust/target/llvm-cov/lcov.info`
- The durable table above intentionally stores only boolean, count, masked capability, coordinate, hash-result, and run-ID evidence. It contains no clipboard content, UNC name, secret, or real user path.

## Validation
- `cargo llvm-cov --locked --workspace --lcov --fail-under-lines 75`: PASS; 927 tests passed, 0 failed, 11 ignored, and the 75% line-coverage gate passed.
- Windows deterministic GUI wrapper: PASS; report `GUI-DETERMINISTIC-20260820T151846Z-632.local.md`.
- PowerShell parsing for both GUI scripts and host-WSL `bash -n` for all GUI shell scripts: PASS. A sandbox-only WSL launch failure was reproduced as an execution-context boundary; the same read-only checks passed in the host context.
- Canonical fixture generation: PASS. A disposable corrupt-copy `--verify-only` probe was rejected with the expected nonzero exit.
- Documentation/reference consistency, work-item manifest validation, and `git diff --check`: PASS before temporary-control cleanup.

## Remaining Prerequisites
- IME composition: a recoverable staged-window-only input-profile/IME-mode harness.
- Alternate DPI: two configured displays with distinct observed scale factors.
- Copy Path: an initially zero-format clipboard or a disposable OS session.
- External open/reveal: a disposable owned handler/session.
- Updater apply/restart: approved signing material in the existing private sandbox workflow.
- Real UNC: an existing authorized reachable fixture share.
