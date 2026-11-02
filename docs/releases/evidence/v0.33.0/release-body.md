## Summary
- Release: `v0.33.0`
- Date: `2026-11-02`
- Windows asset は `x86_64-pc-windows-gnu` + mingw-w64 により Linux/WSL 系導線から生成する。
- standalone バイナリ配布に対応するため、各 OS 向け `README` / `LICENSE` / `THIRD_PARTY_NOTICES` sidecar asset も同時に添付する。

## Downloads
- macOS の `.app` bundle 自体は notarization 用の中間成果物であり、GitHub Release の添付対象には含めない。
- `FlistWalker-0.33.0-linux-x86_64`
- `FlistWalker-0.33.0-linux-x86_64.tar.gz`
- `FlistWalker-0.33.0-linux-x86_64.README.txt`
- `FlistWalker-0.33.0-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.33.0-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.33.0-linux-x86_64`
- `FlistWalker-0.33.0-windows-x86_64.exe`
- `FlistWalker-0.33.0-windows-x86_64.zip`
- `FlistWalker-0.33.0-windows-x86_64.README.txt`
- `FlistWalker-0.33.0-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.33.0-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.33.0-windows-x86_64.exe`
- `FlistWalker-0.33.0-macos-x86_64`
- `FlistWalker-0.33.0-macos-x86_64-app.zip`
- `FlistWalker-0.33.0-macos-x86_64.tar.gz`
- `FlistWalker-0.33.0-macos-x86_64.README.txt`
- `FlistWalker-0.33.0-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.33.0-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.33.0-macos-x86_64`
- `FlistWalker-0.33.0-macos-arm64`
- `FlistWalker-0.33.0-macos-arm64-app.zip`
- `FlistWalker-0.33.0-macos-arm64.tar.gz`
- `FlistWalker-0.33.0-macos-arm64.README.txt`
- `FlistWalker-0.33.0-macos-arm64.LICENSE.txt`
- `FlistWalker-0.33.0-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-0.33.0-macos-arm64`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added
- ResultsのCtrl+クリック（Windows）／Command+クリック（macOS）でTabと同じPINを切替。既定ON、SettingsでOFFにでき、再起動後も保持。

## Changed
- macOSのobjc2を0.6.5に更新。既存ライセンスと配布構成を維持。
- v0.32.1からの更新互換性を26件のchecksum inventoryで検証。

## Fixed
- Indexing中、同一項目のPreviewの不要なクリア／再読込を抑制。対象、実内容・メタデータ変更と古い応答の分離は維持。
- 修飾キー即解放時もクリック時点の状態でPINを判定し、修飾ダブルクリックで二重切替しない。

## Breaking
- なし。

## Deprecated
- なし。

## Security
- query history は既定で平文永続化される。必要なら runtime config の `history_persist_disabled=true` を案内する。
- 自動更新の対象ビルドは埋め込み公開鍵で `SHA256SUMS.sig` を検証し、その後 `SHA256SUMS` の checksum を照合する。
- 当面の暫定運用として、macOS 配布物は未 notarized のまま publish する場合があり、その場合は本 release の `Known issues` または `Security` に明記する。

## Known issues
- macOS配布物は未notarized。
- 過去のTC207ストレス条件の100ms超過、Linux coverageのSIGSEGV、Windows保存ワーカー終了遅延と自己更新失敗は原因未確定。通常条件の成功を原因解消とは扱わない。
- v0.24.3 の updater は `fw-*` を含む現在の checksum manifest を読めないため、v0.24.3 利用者は同じvariantのbinaryと`SHA256SUMS`を手動で取得・検証して一度置き換える必要がある。v0.24.4以降へ移行後は通常の自動更新を再利用できる。

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.33.0-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.33.0-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
