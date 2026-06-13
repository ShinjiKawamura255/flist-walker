## Summary

FlistWalker v0.18.10 was released on June 14, 2026.

This release improves saved-root editing, restores the complete PowerShell Windows GNU build path, fixes cross-platform path checks, and hardens signed release asset validation. It supersedes the unpublished v0.18.9 release candidate with warning-free macOS release builds.

## Downloads

- `FlistWalker-0.18.10-linux-x86_64`
- `FlistWalker-0.18.10-linux-x86_64.tar.gz`
- `FlistWalker-0.18.10-linux-x86_64.README.txt`
- `FlistWalker-0.18.10-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.18.10-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.18.10-windows-x86_64.exe`
- `FlistWalker-0.18.10-windows-x86_64.zip`
- `FlistWalker-0.18.10-windows-x86_64.README.txt`
- `FlistWalker-0.18.10-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.18.10-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.18.10-macos-x86_64`
- `FlistWalker-0.18.10-macos-x86_64-app.zip`
- `FlistWalker-0.18.10-macos-x86_64.tar.gz`
- `FlistWalker-0.18.10-macos-x86_64.README.txt`
- `FlistWalker-0.18.10-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.18.10-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.18.10-macos-arm64`
- `FlistWalker-0.18.10-macos-arm64-app.zip`
- `FlistWalker-0.18.10-macos-arm64.tar.gz`
- `FlistWalker-0.18.10-macos-arm64.README.txt`
- `FlistWalker-0.18.10-macos-arm64.LICENSE.txt`
- `FlistWalker-0.18.10-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added

- Added direct editing for saved roots, including duplicate and invalid-path validation.
- Added PowerShell-native dependency checks, Windows GNU release builds, and artifact regression verification.

## Changed

- Saved-root editing now focuses and selects the complete existing path before replacement.
- The tagged release pipeline now verifies signing-key consistency, the exact asset set, checksums, archive contents, and license sidecars before creating a draft.
- Updated the native folder dialog backend to remove the future-incompatible legacy macOS Objective-C dependency path.

## Fixed

- Fixed duplicate root detection for equivalent macOS and Windows path representations.
- Fixed cross-platform CI path assumptions and the Windows resource test layout that caused Clippy failures.
- Fixed a release-candidate regression where typing after starting a saved-root edit appended to the existing path instead of replacing it.
- Fixed macOS release build warnings caused by updater code that is unused on macOS.

## Breaking

- None.

## Deprecated

- None.

## Security

- Signed checksum manifests are rejected when the signing key, configured verification key, and client-embedded verification key do not match.
- Existing release tags and assets are treated as immutable and are never overwritten by the tagged workflow.

## Known issues

- macOS artifacts are not notarized. Depending on system security settings, users may need to explicitly allow the application to run.

## Verify checksum

PowerShell:

```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.18.10-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.18.10-windows-x86_64.zip
```

bash:

```bash
sha256sum -c SHA256SUMS
```
