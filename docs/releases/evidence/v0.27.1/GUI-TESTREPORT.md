# v0.27.1 GUI Validation Evidence

## Summary

- Windows deterministic scenarios: PASS
- Windows isolated headful scripted smoke: PASS
- WSL fixture validation: PASS
- macOS native interaction and liveness: SKIPPED with explicit release approval because no interactive macOS desktop was available
- Native macOS tests and both tagged macOS release builds: PASS as substitute evidence

## Deterministic Windows Scenarios

The deterministic harness completed all selected groups successfully:

| Group | Discovered | Passed | Result |
| --- | ---: | ---: | --- |
| Surface, dialogs, and theme | 68 | 68 | PASS |
| Shortcut help | 4 | 4 | PASS |
| Preset picker | 43 | 43 | PASS |
| IME and window geometry | 24 | 23 selected | PASS |
| Action guard | 8 | 8 | PASS |
| Bounded action | 3 | 3 | PASS |
| Bounded kind resolution | 5 | 5 | PASS |
| Bounded indexing | 10 | 10 | PASS |
| Terminal settlement | 10 | 10 | PASS |
| Tab ownership | 8 | 7 selected | PASS |
| Background routing | 39 | 39 | PASS |
| Stale-response routing | 25 | 25 | PASS |

The harness is deterministic and therefore does not claim native interaction or liveness by itself.

## Isolated Windows Headful Smoke

- Duration: 10 seconds
- Executable: staged Windows GNU release build
- Process and profile isolation: PASS
- Pre/post staged-file allowlist: PASS
- No updater staging artifacts were created: PASS
- Fixture hash and FileList count: PASS
- Visible PID-owned window responded to `WM_NULL`: PASS
- Scripted Unicode query probe: PASS
- Launch-query metadata confirmed the expected eight-character query and half-width-space state.
- The process remained alive for the full interval and was terminated by the smoke harness.

## macOS Decision

Interactive macOS native interaction and liveness were not run because an interactive macOS desktop was unavailable. This was explicitly accepted for v0.27.1. The release still required and passed macOS-native tests plus x86_64 and arm64 tagged asset builds. The public release body discloses that macOS artifacts may be unnotarized.
