## Summary
- Release: `v0.29.0`
- Date: `2026-09-26`
- CSV / TSV preview の列別 syntax color、tab/query/result の一貫性、bounded な session/settings persistence、GUI の細かな表示修正を含むリリースです。
- Windows asset は `x86_64-pc-windows-gnu` + mingw-w64 で生成しています。standalone バイナリ向けの `README` / `LICENSE` / `THIRD_PARTY_NOTICES` sidecar も添付します。

## Downloads
- `FlistWalker-0.29.0-linux-x86_64`
- `FlistWalker-0.29.0-linux-x86_64.tar.gz`
- `FlistWalker-0.29.0-linux-x86_64.README.txt`
- `FlistWalker-0.29.0-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.29.0-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.29.0-linux-x86_64`
- `FlistWalker-0.29.0-windows-x86_64.exe`
- `FlistWalker-0.29.0-windows-x86_64.zip`
- `FlistWalker-0.29.0-windows-x86_64.README.txt`
- `FlistWalker-0.29.0-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.29.0-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.29.0-windows-x86_64.exe`
- `FlistWalker-0.29.0-macos-x86_64`
- `FlistWalker-0.29.0-macos-x86_64-app.zip`
- `FlistWalker-0.29.0-macos-x86_64.tar.gz`
- `FlistWalker-0.29.0-macos-x86_64.README.txt`
- `FlistWalker-0.29.0-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.29.0-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.29.0-macos-x86_64`
- `FlistWalker-0.29.0-macos-arm64`
- `FlistWalker-0.29.0-macos-arm64-app.zip`
- `FlistWalker-0.29.0-macos-arm64.tar.gz`
- `FlistWalker-0.29.0-macos-arm64.README.txt`
- `FlistWalker-0.29.0-macos-arm64.LICENSE.txt`
- `FlistWalker-0.29.0-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-0.29.0-macos-arm64`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## Added
- CSV / TSV preview に列ごとの syntax color を追加し、引用符内の区切り文字、複数行 field、空 field、8列を超える色循環に対応しました。

## Changed
- tab 切替後の query と検索結果を一貫させ、active result の絞り込みを bounded にして遅延 query を保持するようにしました。
- session / settings の保存と復旧を bounded worker へ集約し、失敗を表示しながら既存状態を保持するようにしました。
- path key 生成時の中間 allocation を削減しました。

## Fixed
- 選択行へ hover しても選択解除 control の配置が動かないようにしました。
- 混在していた tooltip 文言、paged preview の font metrics、復元した sort の結果数を修正しました。

## Breaking
- なし。

## Deprecated
- なし。

## Security
- query history は既定で平文永続化されます。保存を避ける場合は runtime settings の履歴保存無効化を利用してください。
- 自動更新対象ビルドは、埋め込み公開鍵で `SHA256SUMS.sig` を検証した後に `SHA256SUMS` の checksum を照合します。
- macOS 配布物は未 notarized です。

## Known issues
- paged preview の `Color: on/off` はマウスクリックで切り替えられますが、v0.29.0 には到達可能なキーボード操作経路がありません。Windows native 検証ではキーボード切替を FAIL のまま記録し、このリリースに限って既知問題として承認しています。
- Windows の変更対応 native addendum は上記キーボード切替以外を通過しました。実環境の UNC、外部 Open/Reveal、clipboard/copy、IME composition、別 DPI／複数 display、updater の loopback／signed apply、固定500,000件 pressure、全 dialog/persistence failure permutation は安全・環境・規模条件により NOT RUN のままです。deterministic、CI、代表20,011件の native 検証結果を代替根拠とした一回限りの v0.29.0 例外です。
- macOS native GUI の GSM-001..013 は、利用可能な native macOS session がなかったため NOT RUN のままです。CI の macOS tests/builds は実行していますが、native GUI PASS へは置き換えていません。この扱いは v0.29.0 の一回限りの免除です。
- macOS 配布物は notarization 環境が整うまで未 notarized です。
- v0.24.3 の updater は `fw-*` を含む現在の checksum manifest を読めません。v0.24.3 利用者は、同じ variant の binary と `SHA256SUMS` を手動で取得・検証して一度置き換えてください。v0.24.4 以降へ移行後は通常の自動更新を利用できます。

## Verify checksum
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.29.0-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.29.0-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
