# Updater Recovery

This runbook covers one local FlistWalker installation that reports an ambiguous or recovery-required update state. It preserves evidence and restores service without guessing which updater artifact is authoritative. If a released asset, signature, checksum, update feed, or multiple installations may be affected, use [RELEASE_INCIDENT_RUNBOOK.md](RELEASE_INCIDENT_RUNBOOK.md) instead.

## When To Use This Runbook

Use it when startup or update refuses a new update because transaction evidence is incomplete, inconsistent, or still present. Typical evidence includes updater marker, lock, acknowledgement, helper, new, backup, failed, or temporary files in the installation directory.

## Stop And Preserve

1. Close FlistWalker, `fw`, and any updater helper normally. Do not repeatedly restart the update. If a process will not exit, record its name, PID, executable path, and the current time before deciding whether a forced stop is necessary.
2. Leave the affected installation directory unchanged. Do not delete, rename, edit, or replace updater artifacts, and do not copy an arbitrary backup over an executable.
3. If policy and available space allow it, copy the whole affected installation directory to a separate evidence location before further investigation. Record the local and UTC time, FlistWalker version, OS, launch mode, and the exact displayed message.
4. Record artifact names, sizes, modification times, and SHA-256 values without opening or executing unknown helpers. Redact private path components, user names, repository names, tokens, and secrets before sharing the record.

## Safest Service Recovery

1. Download the intended FlistWalker release from the official release page into a new directory. Do not install over the affected directory.
2. Verify the downloaded asset against the published `SHA256SUMS` and the release verification instructions in [RELEASE.md](RELEASE.md). Stop if the expected release, checksum, or signature cannot be established.
3. Launch the clean parallel installation, check `fw --version`, and perform a basic search against a small non-sensitive root.
4. After verification, direct normal use or shortcuts to the clean installation. Keep the affected directory quarantined and unchanged until a maintainer has assessed its transaction evidence.

## Maintainer Assessment

In-place recovery is allowed only after a maintainer identifies the transaction, validates every relevant artifact type and path, compares available old/new hashes with the marker state, and determines the last durable phase. Generic marker deletion is not recovery. Do not execute an unknown helper or infer that the newest timestamp is the correct binary. If the evidence suggests a release-wide asset, signature, checksum, or feed problem, preserve the local evidence and switch to [RELEASE_INCIDENT_RUNBOOK.md](RELEASE_INCIDENT_RUNBOOK.md).

## Completion Criteria

- A clean, independently verified installation starts and passes the basic search check.
- The affected directory remains unchanged or is retained as a quarantined evidence copy.
- The version, OS, times, exact error, artifact inventory, and verification results are recorded with sensitive details redacted.
- A follow-up issue identifies whether local in-place cleanup is safe or a release incident is required.
