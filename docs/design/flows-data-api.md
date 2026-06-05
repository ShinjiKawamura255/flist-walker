# Flows, Data Model, and API Contract

## Main flows
- Flow-001: 起動 -> （FileList 優先モード有効時）FileList 検出 -> 読み込み -> 検索 -> 選択 -> アクション。
- Flow-002: 起動 -> FileList なし -> walker 走査 -> 検索 -> 選択 -> アクション。
- Flow-003: アクション失敗 -> エラー整形 -> 表示 -> 非ゼロ終了（CLI）/エラー通知（GUI）。
- Flow-004: GUI 起動 -> 非同期インデックス -> 最新要求優先検索（古い要求を破棄） -> プレビュー -> 実行/オープン。
- Flow-005: GUI 起動 -> update worker が GitHub Releases を確認 -> 新版あり -> 利用者承認 -> asset と sidecar 文書 (`*.README.txt`, `*.LICENSE.txt`, `*.THIRD_PARTY_NOTICES.txt`) と `SHA256SUMS.sig` / `SHA256SUMS` を取得 -> 署名検証 -> checksum 検証 -> 補助 updater 起動 -> 本体終了 -> 置換後に新版本体と sidecar 文書を同一ディレクトリへ配置して再起動。ignore list sample は別途起動時初期化で補完する。
  `FLISTWALKER_DISABLE_SELF_UPDATE=1`、または実行中バイナリと同一ディレクトリに `FLISTWALKER_DISABLE_SELF_UPDATE` ファイルがある場合は update flow を起動せず、通常起動のみ行う。

## Data model
- Candidate
- `path: PathBuf` 正規化済み絶対パス
- `display: String` 画面表示用パス
- SearchResult
- `candidate: Candidate`
- `score: f64`

## API contract (Rust)
- `build_index(root, use_filelist, include_files, include_dirs)`
- `build_index_with_metadata(...)`
- `find_filelist(root)`
- `parse_filelist(filelist_path, root)`
- `search_entries(query, entries, limit, use_regex)`
- `execute_or_open(path)`
- CLI: `flistwalker [query] [--root PATH] [--limit N] [--cli]`
