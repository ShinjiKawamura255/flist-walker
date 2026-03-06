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

## [0.9.0] - 2026-03-07
### Added
- macOS 向けショートカット差分を整理し、`Cmd+T` / `Cmd+W` / `Cmd+Tab` 系と `Cmd+Shift+C` を主要操作として揃えた。
- タブ単位の検索履歴を追加し、`Ctrl+R` / `Ctrl+Shift+R` で前後移動できるようにした。
- `FLISTWALKER_RESTORE_TABS=1` による opt-in のタブセッション復元を追加した。
- macOS 向け `.app` / zip / tar.gz のリリースアセット生成スクリプトを追加した。

### Changed
- `rust/src/app.rs` を `tests` / `workers` / `session` / `input` / `render` に段階分割し、責務境界を明確化した。
- `Shift+Enter` の格納フォルダオープンは同一フォルダを重複起動しない挙動に統一した。
- README とリリース導線を Windows/macOS 両対応前提へ更新した。

### Fixed
- index/search/preview/action の並行処理を hardening し、古い応答で UI 状態が巻き戻る経路を抑制した。
- worker shutdown と join timeout を見直し、終了時のハングや stale request の取り残しを抑えた。
- incremental query narrowing 用の prefix candidate cache を追加し、絞り込み中の検索応答を改善した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。
- この環境では macOS アセットの実ビルドを完了できないため、公開時は別の macOS 環境での最終生成が必要。

## [0.8.0] - 2026-03-04
### Added
- 

### Changed
- 階層 FileList 展開の判定を「全ディレクトリ探索」から「読み込み済み候補内の `FileList.txt` / `filelist.txt` 完全一致検出」へ変更。
- 階層 FileList 展開の処理キューを浅い階層優先の優先度キューへ変更し、多数のネスト FileList での探索効率を改善。

### Fixed
- 階層 FileList 展開中の子 FileList 解析でも supersede による中断が効くように修正。
- FileList ストリーミング時の不要な全件保持を減らし、メモリ圧迫に起因する体感遅延を抑制。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.7.1] - 2026-03-04
### Added
- 

### Changed
- 

### Fixed
- GUI ワーカーに shutdown フラグ共有と `Drop` 時の `join` を追加し、中断時の終了処理を明示化。
- `Ctrl+C` などのシグナル受信で shutdown 要求を立て、GUI 側で window close へ収束させる終了経路を追加。
- walker インデックスのキャンセル確認間隔を短縮し、多数エントリ走査中の中断反映を改善。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.7.0] - 2026-03-03
### Added
- 

### Changed
- `Files` / `Folders` 切り替え時は、現在のトグル状態（include flags）で再インデックスする動作へ変更。
- `Use FileList` 有効時の `Create File List` は、FileList 優先フローの整合性を維持する実行順へ調整。

### Fixed
- Root 変更時にタブタイトルが即時追従しない問題を修正し、メモリ表示の定期更新も安定化。
- ウィンドウリサイズ時に Preview 幅が意図せず変わる問題を修正。
- 検索窓フォーカス系ショートカットと Tab 操作の回帰を修正。
- `Use FileList` 利用中に `Files` / `Folders` フィルタを操作できてしまう不整合を解消（無効化）。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.5.1] - 2026-02-27
### Added
- タブタイトル上でのマウス中クリックによるタブクローズ操作を追加。

### Changed
- 

### Fixed
- OR 条件（`|`）で左右それぞれに `'` を付けたクエリ（例: `'abc|xyz`, `abc|'xyz`, `'abc|'xyz`）を検索仕様どおりに評価するよう修正。
- 上記クエリで左辺/右辺に `'` を付けた場合に、可視判定とハイライト表示が崩れる問題を修正。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.3.2] - 2026-02-24
### Added
- 

### Changed
- 

### Fixed
- ライトテーマ時に Results の非ハイライト文字色が薄すぎる問題を修正し、可読性を改善。
- 大規模 FileList 読み込みで `Source: None` 表示が長時間残り、再読込反映が遅延しやすい問題を修正（ストリーミング解析 + supersede 中断対応）。
- `Create File List` は一時ファイルをローカル temp に生成してから最終配置へ移動する方式に変更（クロスデバイスは copy フォールバック）。
- 検索窓フォーカス中の `Ctrl+Shift+C` で検索文字列が優先コピーされる回帰を再修正（後段でパスコピー実行）。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.3.1] - 2026-02-23
### Added
- 

### Changed
- 

### Fixed
- Preview の内容が長い場合でも Preview 領域外へ描画がはみ出さないように修正し、フッター（`Entries` / `Results`）表示が隠れないよう改善。
- `Copy Path(s)` 実行時のフッター通知 `Copied path:` から Windows 拡張パス接頭辞（`\\?\`）を除去。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

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

[Unreleased]: https://github.com/ShinjiKawamura255/flist-walker/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.9.0
[0.8.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.8.0
[0.7.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.7.1
[0.7.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.7.0
[0.5.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.5.1
[0.3.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.3.1
[0.3.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.3.0
[0.2.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.2.1
[0.2.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.2.0
[0.1.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.1.0
