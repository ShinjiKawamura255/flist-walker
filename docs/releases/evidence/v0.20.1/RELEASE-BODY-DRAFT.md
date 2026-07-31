# Release Body Draft: v0.20.1

## Summary

FlistWalker v0.20.1, released on August 1, 2026, stabilizes Windows self-update file replacement, fixes two GUI dialog/notice lifecycle defects, and completes an internal responsibility split across the walker, CLI/TUI, GUI rendering, and updater transaction code.

## Downloads

- `FlistWalker-0.20.1-linux-x86_64`
- `FlistWalker-0.20.1-linux-x86_64.tar.gz`
- `FlistWalker-0.20.1-linux-x86_64.README.txt`
- `FlistWalker-0.20.1-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.20.1-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.20.1-windows-x86_64.exe`
- `FlistWalker-0.20.1-windows-x86_64.zip`
- `FlistWalker-0.20.1-windows-x86_64.README.txt`
- `FlistWalker-0.20.1-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.20.1-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.20.1-macos-x86_64`
- `FlistWalker-0.20.1-macos-x86_64-app.zip`
- `FlistWalker-0.20.1-macos-x86_64.tar.gz`
- `FlistWalker-0.20.1-macos-x86_64.README.txt`
- `FlistWalker-0.20.1-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.20.1-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.20.1-macos-arm64`
- `FlistWalker-0.20.1-macos-arm64-app.zip`
- `FlistWalker-0.20.1-macos-arm64.tar.gz`
- `FlistWalker-0.20.1-macos-arm64.README.txt`
- `FlistWalker-0.20.1-macos-arm64.LICENSE.txt`
- `FlistWalker-0.20.1-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added

- No new user-facing features.

## Changed

- Split internal walker, CLI/TUI, GUI rendering, and updater transaction responsibilities into focused owner modules while retaining established search and update semantics; user-visible behavior changes are limited to the fixes below.
- Separated the updater transaction marker model, filesystem safety policy, OS process/replacement primitives, characterization tests, and platform-specific production/test compilation boundaries so rollback, recovery, and marker-v1 compatibility can be validated independently.

## Fixed

- Replaced the Windows updater's external PowerShell file replacement with native in-process `ReplaceFileW`, preventing repeated console windows and failures caused by an unavailable PowerShell executable.
- Preserved verbatim Windows paths and verified replacement behavior with and without backups, including source retention when replacement fails.
- Kept the successful FileList creation notice visible after its automatic re-index completes, so the completion result is not immediately lost.
- Made the startup update-check failure prompt modal: background query and selection input is blocked, while `Return` and `Escape` close only the prompt without executing an item or clearing the query.

## Breaking

- None.

## Deprecated

- None.

## Security

- Query history persists in plaintext by default. Users can disable it with runtime config `history_persist_disabled=true` if needed.
- Signed checksum manifests are verified with the embedded public key before checksum validation.
- The existing no-overwrite, path/type/hash revalidation, sidecars-first/binary-last, rollback, and startup recovery protections remain unchanged.

## Known issues

- macOS artifacts are not notarized yet.

## Verify checksum

PowerShell:

```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.20.1-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.20.1-windows-x86_64.zip
```

bash:

```bash
sha256sum -c SHA256SUMS
```
