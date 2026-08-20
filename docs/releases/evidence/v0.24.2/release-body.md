## Summary
- Release: `v0.24.2`
- Date: `2026-08-21`
- Patch release from `v0.24.1` covering asynchronous state ownership, index completion settlement, Windows updater recovery, and release validation hardening.

## Downloads
- `FlistWalker-0.24.2-linux-x86_64`
- `FlistWalker-0.24.2-linux-x86_64.tar.gz`
- `FlistWalker-0.24.2-linux-x86_64.README.txt`
- `FlistWalker-0.24.2-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.24.2-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.24.2-windows-x86_64.exe`
- `FlistWalker-0.24.2-windows-x86_64.zip`
- `FlistWalker-0.24.2-windows-x86_64.README.txt`
- `FlistWalker-0.24.2-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.24.2-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.24.2-macos-x86_64`
- `FlistWalker-0.24.2-macos-x86_64-app.zip`
- `FlistWalker-0.24.2-macos-x86_64.tar.gz`
- `FlistWalker-0.24.2-macos-x86_64.README.txt`
- `FlistWalker-0.24.2-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.24.2-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.24.2-macos-arm64`
- `FlistWalker-0.24.2-macos-arm64-app.zip`
- `FlistWalker-0.24.2-macos-arm64.tar.gz`
- `FlistWalker-0.24.2-macos-arm64.README.txt`
- `FlistWalker-0.24.2-macos-arm64.LICENSE.txt`
- `FlistWalker-0.24.2-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`
- `SHA256SUMS.sig`
- macOS の `.app` bundle は notarization 用の中間成果物であり、Release asset には含めない。

## Added
- 固定 seed の stateful endurance corpus と、実 worker の scheduled soak 検証を追加した。

## Changed
- tab identity、active owner、request routing を非同期 interleaving 後も維持し、古い応答による別 tab の状態巻き戻しを防止した。
- FileList/index completion settlement、大規模候補検索 baseline、GUI release-candidate evidence の検証導線を強化した。

## Fixed
- index completion 後の結果件数表示を最新 snapshot と同期するようにした。
- 混在した日本語 preview 文の表示を整えた。
- Windows self-update の restart failure recovery に bounded retry、normalized path fallback、旧 bundle の検証付き再起動を適用した。

## Breaking
- None.

## Deprecated
- None.

## Security
- query history は既定で平文永続化される。必要な場合は runtime configuration で永続化を無効化できる。
- self-update は埋め込み公開鍵で `SHA256SUMS.sig` を検証し、その後 `SHA256SUMS` の checksum を照合する。
- 今回の変更範囲に依存関係の追加・更新はない。

## Known issues
- macOS 配布物は notarization 環境が整うまで未 notarized の場合がある。

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.24.2-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.24.2-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
