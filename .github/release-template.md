## Summary
- Release: `vX.Y.Z`
- Date: `YYYY-MM-DD`
- Windows asset は `x86_64-pc-windows-gnu` + mingw-w64 により Linux/WSL 系導線から生成する。
- standalone バイナリ配布に対応するため、各 OS 向け `README` / `LICENSE` / `THIRD_PARTY_NOTICES` sidecar asset も同時に添付する。

## Downloads
- macOS の `.app` bundle 自体は notarization 用の中間成果物であり、GitHub Release の添付対象には含めない。
- `FlistWalker-X.Y.Z-linux-x86_64`
- `FlistWalker-X.Y.Z-linux-x86_64.tar.gz`
- `FlistWalker-X.Y.Z-linux-x86_64.README.txt`
- `FlistWalker-X.Y.Z-linux-x86_64.LICENSE.txt`
- `FlistWalker-X.Y.Z-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-X.Y.Z-windows-x86_64.exe`
- `FlistWalker-X.Y.Z-windows-x86_64.zip`
- `FlistWalker-X.Y.Z-windows-x86_64.README.txt`
- `FlistWalker-X.Y.Z-windows-x86_64.LICENSE.txt`
- `FlistWalker-X.Y.Z-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-X.Y.Z-macos-x86_64`
- `FlistWalker-X.Y.Z-macos-x86_64-app.zip`
- `FlistWalker-X.Y.Z-macos-x86_64.tar.gz`
- `FlistWalker-X.Y.Z-macos-x86_64.README.txt`
- `FlistWalker-X.Y.Z-macos-x86_64.LICENSE.txt`
- `FlistWalker-X.Y.Z-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-X.Y.Z-macos-arm64`
- `FlistWalker-X.Y.Z-macos-arm64-app.zip`
- `FlistWalker-X.Y.Z-macos-arm64.tar.gz`
- `FlistWalker-X.Y.Z-macos-arm64.README.txt`
- `FlistWalker-X.Y.Z-macos-arm64.LICENSE.txt`
- `FlistWalker-X.Y.Z-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`
- `SHA256SUMS.sig`

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
- 自動更新の対象ビルドは埋め込み公開鍵で `SHA256SUMS.sig` を検証し、その後 `SHA256SUMS` の checksum を照合する。
- 当面の暫定運用として、macOS 配布物は未 notarized のまま publish する場合があり、その場合は本 release の `Known issues` または `Security` に明記する。

## Known issues
- notarization 環境が整うまで、macOS 配布物は未 notarized の場合がある。

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
