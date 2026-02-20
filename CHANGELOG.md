# Changelog

このファイルは Keep a Changelog 形式と SemVer に沿って管理します。

## [Unreleased]
### Added
- 

### Changed
- 

### Fixed
- 

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.3.0] - 2026-02-20
### Added
- Root 行に `Set as default` を追加し、次回起動時の既定 Root を保存・適用。
- 既存 `FileList.txt` / `filelist.txt` がある場合の上書き確認ダイアログを追加。

### Changed
- ウィンドウ位置/サイズ、Results/Preview 分割幅、Preview 表示状態を永続化。
- `Create File List` は再 Walker を行わず、最新インデックス結果を再利用する方式へ変更。
- インデックス中の `Create File List` は完了後に遅延実行する動作へ変更。
- 結果行の自動スクロールを「可視範囲外になった時のみ追従」に調整。
- 複数トークン検索時、各トークンの完全一致候補を優先しやすいスコアリングへ調整。

### Fixed
- `Entries` 表示が Root 変更/Refresh Index 後に古い件数を保持する問題を修正。
- `Ctrl+L` の検索窓フォーカス動作をトグル化。
- 検索窓フォーカス中の `Ctrl+Shift+C` で検索文字列がコピーされる問題を修正（Results パスのみコピー）。
- 全角入力時に Space/Shift+Space が効かない問題を修正（半角スペース挿入）。
- FileList 作成中に Root 変更が発生しても UI 状態を巻き戻さないよう安定化。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.2.1] - 2026-02-19
### Added
- 結果一覧で `Ctrl+V` / `Alt+V` によるページ移動を追加。
- `FileList` のパス解釈を拡張し、バックスラッシュ相対パス・引用符付き行・Windows絶対パス（WSL変換含む）を許容。

### Changed
- `Use FileList` の初期値を OFF に変更。
- `Files` / `Folders` は再インデックスではなく表示フィルタとして即時反映する動作へ変更。
- リリースzip同梱の実行ファイル名を `flistwalker.exe` に変更。
- リリースzip同梱 `README.txt` を拡充（操作手順・検索演算子・ショートカット・オプション説明）。

### Fixed
- インデクシング中にオプション切替すると `Source: None` 固定や更新停止が起きる問題を修正（旧リクエストの中断制御を追加）。
- インデクシング中のスクロール位置が先頭へ戻る問題を修正。
- オンデマンドファイルに対する無駄なプレビュー読み込みを抑止。
- `Tab` 複数選択時に検索窓とボタン間でフォーカスが不安定に移動する問題を緩和。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.2.0] - 2026-02-18
### Added
- FileList 作成フローの状態遷移テスト（成功/失敗/再インデックス）を追加。
- CLI 契約テストを拡張（スコア付き出力、0件、不正 root）。

### Changed
- インデックス中の逐次表示方針を改善し、空クエリ時は即時反映、非空クエリ時は時間窓で間引き反映する方式に更新。
- 正規表現検索はクエリ単位で事前コンパイルして再利用。

### Fixed
- Windows で `.ps1` を選択した際に実行されない問題を修正（PowerShell 明示起動）。
- 無効な正規表現が「0件ヒット」に見える問題を修正し、GUI notice へ検索エラーを表示。
- 起動直後/Root 変更直後に結果が一瞬表示後に消える不具合を修正。
- プレビューキャッシュの上限未設定によるメモリ増加リスクを修正（FIFO 上限）。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.1.0] - 2026-02-17
### Added
- 初回公開。
- Windows 向けに `exe単体 + zip` を提供。

### Changed
- Rust 版をメイン実装として整理。

### Fixed
- 

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

[Unreleased]: https://github.com/ShinjiKawamura255/flist-walker/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.3.0
[0.2.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.2.1
[0.2.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.2.0
[0.1.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.1.0
