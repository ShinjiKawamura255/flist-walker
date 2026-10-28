## Summary
- Release: `v0.32.0` — preparation draft, not published.
- Date: 未確定（準備日: 2026-10-28）。公開前に実際の公開日へ確定する。
- 格納フォルダでのファイル選択表示と、FileList・検索catalogの失敗時保護を改善する予定。

## Downloads
- 配布物はまだ生成・添付されていません。候補・tagged runの実物検証後、実際に添付するアセットだけを列挙して本文を確定します。

## Added
- Shift+Enter / Shift+ダブルクリックでファイルの格納フォルダを開くとき、元ファイルを選択表示し、同じフォルダの複数選択を保持する。Windows / macOS と対応Linuxファイルマネージャーで利用でき、Linuxの非対応環境では再検証後に親フォルダを開く。

## Changed
- インデックス性能テストの比較基準と測定前後の契約検査を整合させた。性能閾値や正当性条件は維持する。
- 公開版の検証記録を整理し、v0.31.1からの更新互換性の事前検査を追加した。

## Fixed
- FileList作成の取消・失敗時に、他の処理による対象の変更を保護し、安全に復元できない場合は報告する。
- named root / presetの保存で置換後の同期に失敗した場合は旧状態への復元を試み、復元にも失敗した場合は状態未確定を明示する。
- 不正なcatalog名をCLIの引数エラーとして設定読込前に拒否し、終了コード2を返す。

## Breaking
- なし。

## Deprecated
- なし。

## Security
- 依存バージョンと自己更新の署名検証方式の変更はない。macOSで既存objc2依存の機能を追加で有効化した。
- macOS配布物は未notarized。

## Known issues
- 過去のWindows検証で終了時の保存処理の遅延と自己更新の失敗を観測したが、原因・発生条件・実利用への影響は未確定。新しいソースの成功を原因解消の証明とは扱わない。
- TC-207の48並列ストレス条件での100ms超過と、過去のLinux coverage実行中のSIGSEGVは原因未確定。通常条件の成功とは区別する。
- Shift+ダブルクリック、実UNC上での選択表示、Linuxの実表示には未確認の範囲が残る。省略した検証を成功や恒久的な省略許可とは扱わない。
- v0.24.3利用者は同じvariantのbinaryとSHA256SUMSを手動で取得・検証して一度置き換える必要がある。v0.24.4以降は通常の自動更新を利用できる。

## Verify checksum
配布物の生成・添付と署名検証が済んだ後に使用する例です。

PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.32.0-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.32.0-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```
