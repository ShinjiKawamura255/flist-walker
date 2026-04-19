# Support

FlistWalker does not automatically upload crash reports, logs, file paths, or usage data. Reports are user-initiated through GitHub Issues.

## Before Opening An Issue

- Check the latest release notes and existing issues.
- Reproduce with the smallest root and query that still shows the problem.
- Remove private user names, project names, full paths, tokens, and secrets from anything you paste.
- Prefer a short text reproduction over large screenshots or full logs.

## Bug Reports

Use the GitHub bug report template and include:

- FlistWalker version, for example the release tag or `flistwalker --version`.
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

## Maintainer Triage Checklist

- Confirm the report includes version, OS, launch mode, and reproduction steps.
- Ask for redacted paths when root or FileList behavior matters.
- Ask whether the issue reproduces with a smaller root before requesting broad diagnostics.
- Keep diagnostic collection opt-in and user-controlled.
- Do not request tokens, private repository names, or full home-directory logs.
