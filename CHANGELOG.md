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

[Unreleased]: https://github.com/ShinjiKawamura255/flist-walker/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.2.0
[0.1.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.1.0
