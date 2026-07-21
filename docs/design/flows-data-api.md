# Flows, Data Model, and API Contract

## Main flows
- Flow-001: 起動 -> （FileList 優先モード有効時）FileList 検出 -> 読み込み -> 検索 -> 選択 -> アクション。
- Flow-002: 起動 -> FileList なし -> walker 走査 -> 検索 -> 選択 -> アクション。
- Flow-003: アクション失敗 -> エラー整形 -> 表示 -> 非ゼロ終了（CLI）/エラー通知（GUI）。
- Flow-004: GUI 起動 -> 非同期インデックス -> 最新要求優先検索（古い要求を破棄） -> プレビュー -> 実行/オープン。
- Flow-005: GUI 起動 -> update worker が上限付きで GitHub Releases を確認 -> 新版あり -> 利用者承認 -> `SHA256SUMS` / `SHA256SUMS.sig` を先行取得 -> strict parse と署名検証 -> binary/sidecar を private create-new file へ上限付き streaming download/hash 検証 -> `VerifiedUpdateBundle` -> executable parent 内へ同一 directory 準備 -> durable parent/helper registration と acknowledgement -> 本体終了 -> sidecar 適用 -> binary-last atomic commit -> 再起動。precommit/restart failure は旧 bundle へ rollback し、中断は起動時 marker/hash recovery へ収束する。ignore list sample は別途起動時初期化で補完する。
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
- `stage_update_assets(candidate, transport, limits) -> VerifiedUpdateBundle`
- `prepare_update_transaction(bundle, current_executable) -> PreparedUpdateTransaction`
- `recover_update_transaction(marker, filesystem) -> RecoveryOutcome`
- CLI: `flistwalker [query] [--root PATH] [--limit N] [--cli]`
