## Summary
- Release: `v0.30.2`
- CLI 更新 alias と GUI preview の本文失敗時の情報表示を修正し、回帰テストと配布の信頼性を改善したパッチリリース。
- Date: `2026-10-14`
- Windows asset は `x86_64-pc-windows-gnu` + mingw-w64 により Linux/WSL 系導線から生成する。
- standalone バイナリ配布に対応するため、各 OS 向け `README` / `LICENSE` / `THIRD_PARTY_NOTICES` sidecar asset も同時に添付する。

## Downloads
- macOS の `.app` bundle 自体は notarization 用の中間成果物であり、GitHub Release の添付対象には含めない。
- `FlistWalker-0.30.2-linux-x86_64`
- `FlistWalker-0.30.2-linux-x86_64.tar.gz`
- `FlistWalker-0.30.2-linux-x86_64.README.txt`
- `FlistWalker-0.30.2-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.30.2-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.30.2-linux-x86_64`
- `FlistWalker-0.30.2-windows-x86_64.exe`
- `FlistWalker-0.30.2-windows-x86_64.zip`
- `FlistWalker-0.30.2-windows-x86_64.README.txt`
- `FlistWalker-0.30.2-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.30.2-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.30.2-windows-x86_64.exe`
- `FlistWalker-0.30.2-macos-x86_64`
- `FlistWalker-0.30.2-macos-x86_64-app.zip`
- `FlistWalker-0.30.2-macos-x86_64.tar.gz`
- `FlistWalker-0.30.2-macos-x86_64.README.txt`
- `FlistWalker-0.30.2-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.30.2-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.30.2-macos-x86_64`
- `FlistWalker-0.30.2-macos-arm64`
- `FlistWalker-0.30.2-macos-arm64-app.zip`
- `FlistWalker-0.30.2-macos-arm64.tar.gz`
- `FlistWalker-0.30.2-macos-arm64.README.txt`
- `FlistWalker-0.30.2-macos-arm64.LICENSE.txt`
- `FlistWalker-0.30.2-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-0.30.2-macos-arm64`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added
- なし。

## Changed
- worker の生存、tab 復帰の順序と結果保持、activation 証跡の欠落を検出する回帰テストを強化した。
- release 文書の公開リンク案内を整理し、配布 CI の artifact upload を更新した。

## Fixed
- `--color auto` などを含む alias でも `fw --update` / `fw --check-update` が動くようにした。色指定の値省略・空白区切り・`=` 区切りと前後どちらの順序にも対応し、不正な色値や検索/action との併用は引き続き拒否する。query や外部 command の引数は変更しない。
- GUI preview は binary・空ファイル・本文の読込失敗・削除済みファイル・リンク切れでも、取得可能な情報と失敗理由を表示し、Reload で情報と本文を再取得できるようにした。
- 未解決リンクの target を本文 open 前に確認し、directory は folder preview へ振り分け、既知の FIFO/device 等を本文として開いて worker が待ち続けるケースを防止した。

## Breaking
- なし。

## Deprecated
- なし。

## Security
- query history は既定で平文永続化される。必要なら runtime config の `history_persist_disabled=true` を案内する。
- 自動更新の対象ビルドは埋め込み公開鍵で `SHA256SUMS.sig` を検証し、その後 `SHA256SUMS` の checksum を照合する。
- macOS 配布物は未 notarized。

## Known issues
- macOS 配布物は未 notarized。
- v0.24.3 の updater は `fw-*` を含む現在の checksum manifest を読めないため、v0.24.3 利用者は同じvariantのbinaryと`SHA256SUMS`を手動で取得・検証して一度置き換える必要がある。v0.24.4以降へ移行後は通常の自動更新を再利用できる。

- 大規模な候補集合では query 準備中の index 取り込みを一時停止する。v0.30.0 の50万候補検証の query dispatch 中央値339〜498 ms、最大536 msは元版の観測値であり、本版の再測定値ではない。

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.30.2-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.30.2-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
