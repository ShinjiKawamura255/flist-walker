## Summary
- Release: `v0.27.1`
- Date: `2026-09-16`
- This release improves CLI/TUI sorting and filtering ergonomics, hardens asynchronous GUI/FileList ownership, fixes search and preview regressions, and updates rustls for RUSTSEC-2026-0285.

## Downloads
- `FlistWalker-0.27.1-linux-x86_64`
- `FlistWalker-0.27.1-linux-x86_64.tar.gz`
- `FlistWalker-0.27.1-linux-x86_64.README.txt`
- `FlistWalker-0.27.1-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.27.1-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.27.1-linux-x86_64`
- `FlistWalker-0.27.1-windows-x86_64.exe`
- `FlistWalker-0.27.1-windows-x86_64.zip`
- `FlistWalker-0.27.1-windows-x86_64.README.txt`
- `FlistWalker-0.27.1-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.27.1-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.27.1-windows-x86_64.exe`
- `FlistWalker-0.27.1-macos-x86_64`
- `FlistWalker-0.27.1-macos-x86_64-app.zip`
- `FlistWalker-0.27.1-macos-x86_64.tar.gz`
- `FlistWalker-0.27.1-macos-x86_64.README.txt`
- `FlistWalker-0.27.1-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.27.1-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.27.1-macos-x86_64`
- `FlistWalker-0.27.1-macos-arm64`
- `FlistWalker-0.27.1-macos-arm64-app.zip`
- `FlistWalker-0.27.1-macos-arm64.tar.gz`
- `FlistWalker-0.27.1-macos-arm64.README.txt`
- `FlistWalker-0.27.1-macos-arm64.LICENSE.txt`
- `FlistWalker-0.27.1-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-0.27.1-macos-arm64`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added
- None.

## Changed
- Added full-path ascending/descending ordering, file/folder aliases, and progress-only limit truncation warnings across CLI/TUI search flows.
- Moved GUI root-setting persistence to a bounded worker and retained retryable drafts when persistence fails.

## Fixed
- Fixed missing candidates after switching from regex to plain search, UTF-8 preview corruption at 64 KiB boundaries, and repeated full-copy/full-scan work in large incremental search and nested FileList replacement.
- Hardened action freshness and CLI error redaction, deterministic FileList case-variant selection, ancestor symlink boundaries, and FileList create/cancel/rollback transactions.
- Fixed tab-local action failure, background FileList response, session restore, payload ownership, and asynchronous settings races that could roll state backward or block the UI thread.

## Breaking
- None.

## Deprecated
- None.

## Security
- Updated `rustls` to 0.23.45 and `rustls-webpki` to 0.103.15 to resolve RUSTSEC-2026-0285.
- Query history is stored as plaintext by default; set `history_persist_disabled=true` in runtime config if persistence is not desired.
- Update-enabled builds verify `SHA256SUMS.sig` with the embedded public key before checking hashes in `SHA256SUMS`.
- macOS artifacts may be published without notarization under the current temporary release posture.

## Known issues
- macOS artifacts may remain unnotarized until the notarization environment is available.
- The v0.24.3 updater cannot parse current manifests containing `fw-*`; v0.24.3 users must manually download and verify the matching binary and `SHA256SUMS` once before normal updates resume on v0.24.4 or newer.

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.27.1-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.27.1-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
