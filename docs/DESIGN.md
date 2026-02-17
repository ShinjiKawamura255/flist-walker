# DESIGN

## Architecture overview
- DES-001 Index Source Resolver
- 役割: `FileList.txt`/`filelist.txt` の検出と優先読み込み。
- 出力: `Path[]` 候補。
- DES-002 Walker Indexer
- 役割: リスト未存在時の再帰走査。
- 出力: `Path[]` 候補。
- DES-003 Fuzzy Search Engine
- 役割: クエリと候補から `(path, score)` を返す。
- DES-004 Action Executor
- 役割: ファイル実行/オープン、フォルダオープンを OS 非依存 API で提供。
- DES-005 CLI Adapter
- 役割: Python 試作の UX を Rust へ移植しやすい I/O 契約で固定。
- DES-009 GUI Adapter (PySide6)
- 役割: 検索入力、結果表示、プレビュー、実行/オープン操作を一画面で提供。
- DES-010 GUI Test Artifacts
- 役割: GUI の回帰手順と結果記録を `docs/GUI-TESTPLAN.md` / `docs/GUI-TESTREPORT.md` で管理。

## Main flows
- Flow-001: 起動 -> FileList 検出 -> ある場合は読み込み -> 検索 -> 選択 -> アクション。
- Flow-002: 起動 -> FileList なし -> walker 走査 -> 検索 -> 選択 -> アクション。
- Flow-003: アクション失敗 -> エラー整形 -> stderr 出力 -> 非ゼロ終了。
- Flow-004: GUI 起動 -> インデックス構築 -> 入力デバウンス検索 -> プレビュー -> 実行/オープン。

## Data model
- Candidate
- `path: Path` 正規化済み絶対パス
- `display: str` 画面表示用相対パス
- SearchResult
- `candidate: Candidate`
- `score: float`

## API contract
- `build_index(root: Path) -> list[Path]`
- `build_index_with_metadata(root: Path) -> IndexBuildResult`
- `find_filelist(root: Path) -> Path | None`
- `parse_filelist(filelist_path: Path, root: Path) -> list[Path]`
- `search_entries(query: str, entries: list[Path], limit: int = 20) -> list[tuple[Path, float]]`
- `execute_or_open(path: Path) -> None`
- `python -m fast_file_finder [query] [--limit N] [--root PATH]`
- `python -m fast_file_finder --gui [--root PATH] [--limit N] [query]`
- `flistwalker-gui [--root PATH] [--limit N] [--query TEXT]`

## Non-functional considerations
- DES-006 Performance
- `os.scandir` ベースの反復走査でメモリ効率を担保。
- 検索は `rapidfuzz` に委譲して CPU コストを削減。
- DES-007 Reliability / Error
- OS コマンド起動失敗を例外として分類し終了コードへ反映。
- DES-008 Testability
- インデックス/検索/アクションを分離し、OS 依存層を薄く保つ。
- GUI ロジックは `ui_model.py` へ分離し、Qt 非依存で unit test 可能にする。

## Error handling / timeouts / logging / metrics
- エラー戦略: `FileNotFoundError` / `PermissionError` / `OSError` をユーザ向けに変換。
- タイムアウト: 外部プロセス起動は非同期起動し、CLI をブロックしない。
- ログ: 試作段階では stderr 出力、Rust 本実装で構造化ログへ拡張。
- メトリクス: 検索遅延(ms)と候補件数を計測対象とする（後続タスク）。

## Migration / rollback
- 移行手順
1. Python の関数境界を Rust crate/module に対応付ける。
2. CLI 引数契約を Rust `clap` 等で再現する。
3. テストケース ID を Rust テストに移植する。
- ロールバック戦略
1. Rust 版が不安定時は Python 試作を fallback コマンドとして維持する。

## Trade-offs
- Python 試作では開発速度と UI 検証を優先し、PySide6 で単画面 GUI を採用する。
- Rust 本実装では性能と配布性を優先し、同一 SPEC を満たす範囲で内部実装のみ刷新する。

## Traceability (excerpt)
- DES-001 -> TC-001 (SP-001)
- DES-002 -> TC-002 (SP-002)
- DES-003 -> TC-003 (SP-003)
- DES-004 -> TC-004, TC-005 (SP-004, SP-005)
- DES-005 -> TC-006 (SP-006)
- DES-006 -> TC-007 (SP-007)
- DES-007 -> TC-008 (SP-008)
- DES-008 -> TC-009 (SP-009)
- DES-009 -> TC-010 (SP-010)
- DES-010 -> TC-011 (SP-011)
