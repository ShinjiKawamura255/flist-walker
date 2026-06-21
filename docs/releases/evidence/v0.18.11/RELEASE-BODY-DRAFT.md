# Release Body Draft: v0.18.11

## Summary

FlistWalker v0.18.11 is prepared for release on June 21, 2026.

This release adds repository documentation entrypoints, surfaces search sort scope and total match counts, and recenters the Manage list viewport relative to its parent geometry.

## Downloads

- `FlistWalker-0.18.11-linux-x86_64`
- `FlistWalker-0.18.11-linux-x86_64.tar.gz`
- `FlistWalker-0.18.11-linux-x86_64.README.txt`
- `FlistWalker-0.18.11-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.18.11-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.18.11-windows-x86_64.exe`
- `FlistWalker-0.18.11-windows-x86_64.zip`
- `FlistWalker-0.18.11-windows-x86_64.README.txt`
- `FlistWalker-0.18.11-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.18.11-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.18.11-macos-x86_64`
- `FlistWalker-0.18.11-macos-x86_64-app.zip`
- `FlistWalker-0.18.11-macos-x86_64.tar.gz`
- `FlistWalker-0.18.11-macos-x86_64.README.txt`
- `FlistWalker-0.18.11-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.18.11-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.18.11-macos-arm64`
- `FlistWalker-0.18.11-macos-arm64-app.zip`
- `FlistWalker-0.18.11-macos-arm64.tar.gz`
- `FlistWalker-0.18.11-macos-arm64.README.txt`
- `FlistWalker-0.18.11-macos-arm64.LICENSE.txt`
- `FlistWalker-0.18.11-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added

- Added repository documentation entrypoints in `docs/INDEX.md`.
- Added search result visibility for active sort scope and total match counts.

## Changed

- No additional user-facing changes beyond the added search result metadata.

## Fixed

- Recentred the Manage list viewport using the parent window geometry.

## Breaking

- None.

## Deprecated

- None.

## Security

- Query history persists in plaintext by default. Users can disable it with runtime config `history_persist_disabled=true` if needed.
- Signed checksum manifests are verified with the embedded public key before checksum validation.
- macOS artifacts may still be unpublished without notarization while the temporary release posture remains in effect.

## Known issues

- macOS artifacts are not notarized yet.

## Verify checksum

PowerShell:

```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.18.11-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.18.11-windows-x86_64.zip
```

bash:

```bash
sha256sum -c SHA256SUMS
```
