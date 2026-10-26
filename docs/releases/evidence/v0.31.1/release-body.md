## Summary
- Release: `v0.31.1`
- Date: `2026-10-26`
- Windows asset は `x86_64-pc-windows-gnu` + mingw-w64 により Linux/WSL 系導線から生成する。
- standalone バイナリ配布に対応するため、各 OS 向け `README` / `LICENSE` / `THIRD_PARTY_NOTICES` sidecar asset も同時に添付する。

## Downloads
- macOS の `.app` bundle 自体は notarization 用の中間成果物であり、GitHub Release の添付対象には含めない。
- `FlistWalker-0.31.1-linux-x86_64`
- `FlistWalker-0.31.1-linux-x86_64.tar.gz`
- `FlistWalker-0.31.1-linux-x86_64.README.txt`
- `FlistWalker-0.31.1-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.31.1-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.31.1-linux-x86_64`
- `FlistWalker-0.31.1-windows-x86_64.exe`
- `FlistWalker-0.31.1-windows-x86_64.zip`
- `FlistWalker-0.31.1-windows-x86_64.README.txt`
- `FlistWalker-0.31.1-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.31.1-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.31.1-windows-x86_64.exe`
- `FlistWalker-0.31.1-macos-x86_64`
- `FlistWalker-0.31.1-macos-x86_64-app.zip`
- `FlistWalker-0.31.1-macos-x86_64.tar.gz`
- `FlistWalker-0.31.1-macos-x86_64.README.txt`
- `FlistWalker-0.31.1-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.31.1-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.31.1-macos-x86_64`
- `FlistWalker-0.31.1-macos-arm64`
- `FlistWalker-0.31.1-macos-arm64-app.zip`
- `FlistWalker-0.31.1-macos-arm64.tar.gz`
- `FlistWalker-0.31.1-macos-arm64.README.txt`
- `FlistWalker-0.31.1-macos-arm64.LICENSE.txt`
- `FlistWalker-0.31.1-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-0.31.1-macos-arm64`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added
- なし。

## Changed
- 非同期処理の回帰テスト、テスト間のruntime設定の隔離、TC-207の性能診断と検証記録を補強した。製品の性能閾値は維持する。

## Fixed
- 停止対象がないときにStop Indexing / Stop All Indexingをグレーアウトする。現在タブと全タブの対象を区別し、待機・確定待ち・遅延更新も停止可能なまま扱う。
- 操作直前に処理が完了した場合は停止状態や通知を変えず、ボタン位置と旧確定結果を保持する。
- v0.31.0からの更新互換性をv0.31.1の26件のchecksum inventoryで検証する回帰テストを追加した。

## Breaking
- なし。

## Deprecated
- なし。

## Security
- query history は既定で平文永続化される。必要なら runtime config の `history_persist_disabled=true` を案内する。
- 自動更新の対象ビルドは埋め込み公開鍵で `SHA256SUMS.sig` を検証し、その後 `SHA256SUMS` の checksum を照合する。
- 当面の暫定運用として、macOS 配布物は未 notarized のまま publish する場合があり、その場合は本 release の `Known issues` または `Security` に明記する。

## Known issues
- TC-207の48並列ストレス条件での100ms超過と、過去Linux coverageのSIGSEGVは原因未確定。通常CIの成功を原因解消とは扱わない。
- 大規模候補集合のquery準備中はindex取り込みを一時停止する。過去版の性能値を本版の測定値とは扱わない。
- notarization 環境が整うまで、macOS 配布物は未 notarized の場合がある。
- v0.24.3 の updater は `fw-*` を含む現在の checksum manifest を読めないため、v0.24.3 利用者は同じvariantのbinaryと`SHA256SUMS`を手動で取得・検証して一度置き換える必要がある。v0.24.4以降へ移行後は通常の自動更新を再利用できる。

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.31.1-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.31.1-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
