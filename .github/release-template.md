## Summary
- Release: `vX.Y.Z`
- Date: `YYYY-MM-DD`

## Downloads
- macOS の `.app` bundle 自体は notarization 用の中間成果物であり、GitHub Release の添付対象には含めない。
- `FlistWalker-X.Y.Z-linux-x86_64`
- `FlistWalker-X.Y.Z-linux-x86_64.tar.gz`
- `FlistWalker-X.Y.Z-windows-x86_64.exe`
- `FlistWalker-X.Y.Z-windows-x86_64.zip`
- `FlistWalker-X.Y.Z-macos-x86_64`
- `FlistWalker-X.Y.Z-macos-x86_64-app.zip`
- `FlistWalker-X.Y.Z-macos-x86_64.tar.gz`
- `FlistWalker-X.Y.Z-macos-arm64`
- `FlistWalker-X.Y.Z-macos-arm64-app.zip`
- `FlistWalker-X.Y.Z-macos-arm64.tar.gz`
- `SHA256SUMS`

## Added
- 

## Changed
- 

## Fixed
- 

## Breaking
- 

## Deprecated
- 

## Security
- query history は既定で平文永続化される。必要なら `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` を案内する。
- draft release から publish する前に、macOS 配布物の notarization 確認を行う。

## Known issues
- 

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-X.Y.Z-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-X.Y.Z-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
