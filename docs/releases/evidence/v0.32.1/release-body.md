## Summary
- Release: `v0.32.1`
- Date: `2026-10-30`
- Windows asset は `x86_64-pc-windows-gnu` + mingw-w64 により Linux/WSL 系導線から生成する。
- standalone バイナリ配布に対応するため、各 OS 向け `README` / `LICENSE` / `THIRD_PARTY_NOTICES` sidecar asset も同時に添付する。

## Downloads
- macOS の `.app` bundle 自体は notarization 用の中間成果物であり、GitHub Release の添付対象には含めない。
- `FlistWalker-0.32.1-linux-x86_64`
- `FlistWalker-0.32.1-linux-x86_64.tar.gz`
- `FlistWalker-0.32.1-linux-x86_64.README.txt`
- `FlistWalker-0.32.1-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.32.1-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.32.1-linux-x86_64`
- `FlistWalker-0.32.1-windows-x86_64.exe`
- `FlistWalker-0.32.1-windows-x86_64.zip`
- `FlistWalker-0.32.1-windows-x86_64.README.txt`
- `FlistWalker-0.32.1-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.32.1-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.32.1-windows-x86_64.exe`
- `FlistWalker-0.32.1-macos-x86_64`
- `FlistWalker-0.32.1-macos-x86_64-app.zip`
- `FlistWalker-0.32.1-macos-x86_64.tar.gz`
- `FlistWalker-0.32.1-macos-x86_64.README.txt`
- `FlistWalker-0.32.1-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.32.1-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.32.1-macos-x86_64`
- `FlistWalker-0.32.1-macos-arm64`
- `FlistWalker-0.32.1-macos-arm64-app.zip`
- `FlistWalker-0.32.1-macos-arm64.tar.gz`
- `FlistWalker-0.32.1-macos-arm64.README.txt`
- `FlistWalker-0.32.1-macos-arm64.LICENSE.txt`
- `FlistWalker-0.32.1-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-0.32.1-macos-arm64`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added
- なし。

## Changed
- 公開済みv0.32.0からの更新互換性を、26件のchecksum inventoryで検証する事前回帰テストを追加しました。

## Fixed
- WindowsでOSと異なるLight/Darkを選んだときのタイトルバーと本体の色の不一致を修正しました。実行中にOSテーマが変わっても明示設定を保ち、SystemではOSに追従します。アプリ設定の反映は引き続き次回起動時です。
- PIN中のClear Selectedを落ち着いたアンバー色と枠線で強調し、件数ボタンにPinnedと表示して状態を分かりやすくしました。Tabと解除の操作は維持します。
- Results件数は左、Sorted byとScopeは結果一覧の右端へ配置し直し、文字の高さを揃えました。プレビュー有無や狭い幅に合わせて折り返します。
- Windows GNUで候補パスのverbatim prefixにより既存freshnessテスト3件が失敗する投影漏れを修正しました。同じRootでも要求されたパス表記に揃え、外部パスとaliasを維持します（#227、#228）。

## Breaking
- なし。

## Deprecated
- なし。

## Security
- 依存バージョン・既存依存feature・更新の署名検証方式は変更していません。既存raw-window-handleのWindows直接参照とOSS告知を追加しました。
- 自動更新は埋め込み公開鍵でSHA256SUMS.sigを検証し、各アセットのchecksumを照合します。
- query historyは既定で平文保存されます。不要な場合はruntime configのhistory_persist_disabled=trueを設定してください。

## Known issues
- macOS配布物は未notarizedです。
- 過去のTC-207の48並列ストレス条件での100ms超過とLinux coverageのSIGSEGVは原因未確定です。通常条件の成功を原因解消とは扱いません。
- 過去のWindows保存ワーカー終了遅延と自己更新失敗は原因・実利用への影響が未確定です。今回のパス表記修正による原因解消は確認していません。
- 日本語IME、別DPI/複数画面、実UNC、Linuxの実表示など、今回の変更で影響しない実機確認範囲には未確認事項が残ります。
- v0.24.3利用者は同じvariantのbinaryとSHA256SUMSを手動で取得・検証して一度置き換えてください。v0.24.4以降は通常の自動更新を利用できます。

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.32.1-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.32.1-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
