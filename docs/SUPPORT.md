# Support

FlistWalker does not automatically upload crash reports, logs, file paths, or usage data. Reports are user-initiated through GitHub Issues.

## Before Opening An Issue

- Check the latest release notes and existing issues.
- Reproduce with the smallest root and query that still shows the problem.
- Remove private user names, project names, full paths, tokens, and secrets from anything you paste.
- Prefer a short text reproduction over large screenshots or full logs.

## Bug Reports

Use the GitHub bug report template and include:

- FlistWalker version, for example the release tag or `fw --version` (on Windows, use the separately distributed CLI executable).
- Operating system and launch mode.
- Whether the root uses `FileList.txt` / `filelist.txt` or walker indexing.
- Approximate root size when performance or indexing is involved.
- Query text, filter state, sort mode, and shortcut/action used when relevant.
- Expected behavior and actual behavior.
- Minimal terminal output or screenshots only when they add information.

Do not attach unredacted logs from a private home directory or workplace repository. If a maintainer asks for more diagnostics, share the smallest redacted excerpt that demonstrates the failing flow.

## Feature Requests

Use the feature request template and describe the workflow first. Include platform and compatibility constraints when the request touches keyboard shortcuts, file managers, indexing behavior, or release packaging.

## Security Or Privacy Concerns

If a report would require sharing secrets, private paths, or sensitive repository names, do not post those details publicly. Open a minimal public issue that describes the class of problem and ask for a private coordination path.

## Updater Recovery

If startup or update reports an ambiguous or recovery-required installation state, stop repeated update attempts and follow [UPDATER_RECOVERY.md](UPDATER_RECOVERY.md). Preserve the affected installation directory and updater artifacts; do not delete, rename, or overwrite them as a generic fix. Before sharing any file list, hash record, screenshot, or diagnostic text, redact private paths, user names, repository names, tokens, and secrets.

## UI-State Persistence Recovery

If saving roots or query history reports an unreadable or invalid UI-state document, preserve `.flistwalker_ui_state.json` and `.flistwalker_roots.txt` before attempting repair. The writer refuses to replace an existing unreadable, invalid-UTF-8, malformed, non-object, or incorrectly typed JSON document. Startup may still use default values, but saving remains disabled until a successful restart to protect the original settings. A missing file is initialized normally. The GUI footer keeps a session-save failure visible; hover over it to read the full error.

Close all FlistWalker and `fw` instances normally before manual repair, then make a private copy of the affected files. Pending, unflushed history is not guaranteed to survive process exit. Restore a known-good copy or repair the UI-state file as a UTF-8 JSON object while retaining its settings, history and unknown fields. Do not replace it with an empty object as a generic fix, and do not delete the sidecar lock file to bypass another running writer. If the error is an access or storage failure, resolve that condition before retrying.

Restart with the same settings location, retry the failed save, and verify the saved values after another restart. Preserve the original copy until verification succeeds. A rollback-failure message means restoration was incomplete; keep both files and the full redacted error for maintainer assessment instead of assuming the save was undone. This procedure is a manual recovery path; automated tests exercise isolated fixtures, not repairs to user data.

## Maintainer Triage Checklist

- Confirm the report includes version, OS, launch mode, and reproduction steps.
- Ask for redacted paths when root or FileList behavior matters.
- Ask whether the issue reproduces with a smaller root before requesting broad diagnostics.
- Keep diagnostic collection opt-in and user-controlled.
- Do not request tokens, private repository names, or full home-directory logs.
