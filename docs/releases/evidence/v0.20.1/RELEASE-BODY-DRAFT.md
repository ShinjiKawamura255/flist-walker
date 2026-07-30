# Release Body Draft: v0.20.1

## Summary

FlistWalker v0.20.1, released on July 30, 2026, stabilizes Windows self-update file replacement and completes an internal responsibility split across the walker, CLI/TUI, GUI rendering, and updater transaction code without changing their existing user contracts.

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

- Split internal walker, CLI/TUI, GUI rendering, and updater transaction responsibilities into focused owner modules while preserving existing search, interaction, and update contracts.
- Separated the updater transaction marker model, filesystem safety policy, OS process/replacement primitives, and characterization tests so rollback, recovery, and marker-v1 compatibility can be validated independently.

## Fixed

- Replaced the Windows updater's external PowerShell file replacement with native in-process `ReplaceFileW`, preventing repeated console windows and failures caused by an unavailable PowerShell executable.
- Preserved verbatim Windows paths and verified replacement behavior with and without backups, including source retention when replacement fails.

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
