# Summary
- Release: `v0.10.0`
- Date: `2026-03-07`

## Downloads
- `FlistWalker-0.10.0-linux-x86_64`
- `FlistWalker-0.10.0-linux-x86_64.tar.gz`
- `FlistWalker-0.10.0-windows-x86_64.exe`
- `FlistWalker-0.10.0-windows-x86_64.zip`
- `FlistWalker-0.10.0-macos-x86_64`
- `FlistWalker-0.10.0-macos-x86_64-app.zip`
- `FlistWalker-0.10.0-macos-x86_64.tar.gz`
- `FlistWalker-0.10.0-macos-arm64`
- `FlistWalker-0.10.0-macos-arm64-app.zip`
- `FlistWalker-0.10.0-macos-arm64.tar.gz`
- `SHA256SUMS`

## Added
- 

## Changed
- `Esc` を `Ctrl+G` と同じ clear/cancel 操作へ統一し、`Ctrl+Shift+R` を廃止した。
- `Ctrl+R` を query history のファジー検索へ変更し、同じ検索欄で履歴検索中とわかる表示および `Enter` / `Ctrl+J` / `Ctrl+M` による展開を追加した。
- query history を全タブ共通で最大100件まで永続化するようにした。

## Fixed
- 

## Breaking
- 

## Deprecated
- 

## Security
- 

## Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。
- この環境では macOS アセットの実ビルドを完了できないため、公開時は別の macOS 環境での最終生成が必要。

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.10.0-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.10.0-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
