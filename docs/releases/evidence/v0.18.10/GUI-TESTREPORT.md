# GUI TESTREPORT: v0.18.10

## Summary
- Date: 2026-06-13
- Tester: Codex
- Build/version: Windows GNU release build, FlistWalker 0.18.10
- Scope: native folder dialog regression after `native-dialog` 0.9.7 update
- OS/display: Windows, native desktop window
- Overall: PASS with macOS GUI SKIPPED

## Results
| ID | Status | Notes |
| --- | --- | --- |
| Native folder dialog | PASS | `Browse...` opened the Windows `Open a Folder` dialog at the configured root and `Escape` canceled it cleanly. |
| Windows GNU launch | PASS | The v0.18.10 release executable launched and displayed the main FlistWalker window. |
| Prior GSM-006 / GSM-008 coverage | PASS | The saved-root and FileList workflows remain covered by the v0.18.9 candidate evidence and unchanged automated tests. |
| macOS GUI | SKIPPED | No interactive macOS desktop was available. macOS native CI tests and both release builds are required to be warning-free before publish. |

## Evidence
- Interactive verification used Windows Graphics Capture and UI Automation during the release thread.
- The folder dialog appeared as a FlistWalker-owned window titled `Open a Folder`.
- Binary screenshots were not exported by the automation interface.

## Residual Risk
- Interactive macOS folder dialog behavior was not tested.
- The tagged workflow must pass both macOS architectures without project or future-incompatibility warnings.
