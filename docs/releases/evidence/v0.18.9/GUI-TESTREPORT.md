# GUI TESTREPORT: v0.18.9

## Summary
- Date: 2026-06-13
- Tester: Codex
- Build/version: Windows GNU release build, FlistWalker 0.18.9
- Base commit: `5aed09f46cab2a1ace3119d93825bf013e1b093b` plus the v0.18.9 release preparation working tree
- Scope: `GSM-006`, `GSM-008`, saved-root edit regression, headful launch
- OS/display: Windows, native desktop window
- Fixture root: `rust/target/gui-smoke/root`
- Overall: PASS with macOS GUI SKIPPED

## Results
| ID | Status | Notes | Evidence |
| --- | --- | --- | --- |
| GSM-006 | PASS | Fixture root displayed `Source: FileList (FileList.txt)`. `Create File List` showed the Walker explanation. Cancel returned cleanly. Continue reached the existing FileList overwrite dialog, and Cancel returned `Create File List cancelled`. | This report; `GUI-HEADFUL-SMOKE.md`; `gui-headful-smoke.log` |
| GSM-008 | PASS | Manage list draft add, row selection, edit focus/select-all, duplicate rejection, edit Cancel, removal-mode Cancel, selected draft removal, Apply, and OK were exercised. | This report |
| Windows headful launch | PASS | The GUI process stayed alive for 10 seconds against the standard fixture. | `GUI-HEADFUL-SMOKE.md`; `gui-headful-smoke.log` |
| macOS GUI | SKIPPED | No interactive macOS desktop was available. The substitute evidence is the successful macOS native test/build coverage in GitHub Actions run `27411612608`; tagged release native tests/builds remain a publish gate. | `https://github.com/ShinjiKawamura255/flist-walker/actions/runs/27411612608` |

## Regression Found And Fixed
- Initial Windows verification found that edit focus moved to the path field but did not select the existing text; typing appended to the path.
- Root cause: `start_editing_manage_root_list_item` set `edit_select_all_requested` to `false`.
- The release candidate now sets the request to `true` and includes `manage_root_list_edit_requests_focus_and_select_all`.
- The corrected release build visibly selected the full path, replacement typing did not append, and duplicate validation kept the editor open with a field error.

## Visual Checkpoints
- Manage list draft addition showed the fixture root selected.
- Edit mode showed the complete fixture path highlighted before replacement typing.
- Duplicate edit showed `This folder is already in the list`.
- Removal mode showed the fixture checkbox, Cancel preserved it, and Remove selected removed only the draft entry.
- Apply showed `Applied saved roots list`; OK closed the manager.
- FileList source and both Create File List dialogs were visually inspected through Windows Graphics Capture during the release thread. Binary screenshots were not exported by the automation interface; this dated report and generated headful log are the durable evidence.

## Residual Risk
- macOS native GUI interaction was not performed.
- The tagged workflow must still pass macOS native tests and both macOS release builds before publish.
