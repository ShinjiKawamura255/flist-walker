## Summary
- Release: `v0.28.0`
- Date: `2026-09-20`
- This release adds paged text preview, GUI settings and selection UX, and strengthens bounded asynchronous search, persistence, and failure recovery across the application.

## Downloads
- `FlistWalker-0.28.0-linux-x86_64`
- `FlistWalker-0.28.0-linux-x86_64.tar.gz`
- `FlistWalker-0.28.0-linux-x86_64.README.txt`
- `FlistWalker-0.28.0-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.28.0-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.28.0-linux-x86_64`
- `FlistWalker-0.28.0-windows-x86_64.exe`
- `FlistWalker-0.28.0-windows-x86_64.zip`
- `FlistWalker-0.28.0-windows-x86_64.README.txt`
- `FlistWalker-0.28.0-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.28.0-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.28.0-windows-x86_64.exe`
- `FlistWalker-0.28.0-macos-x86_64`
- `FlistWalker-0.28.0-macos-x86_64-app.zip`
- `FlistWalker-0.28.0-macos-x86_64.tar.gz`
- `FlistWalker-0.28.0-macos-x86_64.README.txt`
- `FlistWalker-0.28.0-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.28.0-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.28.0-macos-x86_64`
- `FlistWalker-0.28.0-macos-arm64`
- `FlistWalker-0.28.0-macos-arm64-app.zip`
- `FlistWalker-0.28.0-macos-arm64.tar.gz`
- `FlistWalker-0.28.0-macos-arm64.README.txt`
- `FlistWalker-0.28.0-macos-arm64.LICENSE.txt`
- `FlistWalker-0.28.0-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-0.28.0-macos-arm64`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added
- Added paged text preview with bounded syntax coloring and bounded selection pages for large result sets.
- Added a GUI settings dialog with safe JSON persistence, query field spelling/empty-value guidance, and explicit PIN/selection actions.

## Changed
- Stabilized GUI search, indexing, preview, settings persistence, session restore, background refresh, ignore updates, sorting, and tab lifecycle around bounded workers and request identity.
- Strengthened FileList/config atomic persistence, normalization, durability handling, and repository/CI contract validation.

## Fixed
- Fixed search-worker disconnect and request-send failures so completed responses are drained first, committed results and selection state remain visible, and a persistent restart instruction is shown without automatic replay.
- Fixed large-preview UTF-8 boundaries, syntax-coloring and preview-failure handling, settings-save notices, and tab-local state races.
- Fixed FileList crash recovery, atomic-replace durability failures, updater/config migration rollback, and stale background responses that could roll state backward.

## Breaking
- None.

## Deprecated
- None.

## Security
- Query history is stored as plaintext by default; set `history_persist_disabled=true` in runtime config if persistence is not desired.
- Update-enabled builds verify `SHA256SUMS.sig` with the embedded public key before checking hashes in `SHA256SUMS`.
- macOS artifacts may be published without notarization under the current temporary release posture.

## Known issues
- macOS artifacts may remain unnotarized until the notarization environment is available.
- The v0.24.3 updater cannot parse current manifests containing `fw-*`; v0.24.3 users must manually download and verify the matching binary and `SHA256SUMS` once before normal updates resume on v0.24.4 or newer.

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.28.0-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.28.0-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
