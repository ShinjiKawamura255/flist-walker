# DESIGN

## Architecture overview
- DES-001 Index Source Resolver
- 役割: `FileList.txt`/`filelist.txt` の検出と優先読み込み。
- 実装: `rust/src/indexer.rs`

- DES-002 Walker Indexer
- 役割: FileList 未使用時の再帰走査。
- 実装: `rust/src/indexer.rs`

- DES-003 Fuzzy Search Engine
- 役割: クエリ解釈（`'` `!` `^` `$`）とスコアリング。
- 実装: `rust/src/search.rs`

- DES-004 Action Executor
- 役割: ファイル実行/オープン、フォルダオープンを OS 差分吸収して実行。
- 実装: `rust/src/actions.rs`

- DES-005 CLI Adapter
- 役割: `clap` 引数を受け取り CLI 出力へ変換。
- 実装: `rust/src/main.rs`

- DES-009 GUI Adapter (egui/eframe)
- 役割: 検索入力、結果表示、プレビュー、複数選択と一括操作を提供。
- 実装: `rust/src/app.rs`, `rust/src/ui_model.rs`

- DES-010 GUI Test Artifacts
- 役割: GUI 回帰手順と結果を管理する。
- 実装: `docs/TESTPLAN.md` の GUI 手順節

## Main flows
- Flow-001: 起動 -> FileList 検出 -> 読み込み -> 検索 -> 選択 -> アクション。
- Flow-002: 起動 -> FileList なし -> walker 走査 -> 検索 -> 選択 -> アクション。
- Flow-003: アクション失敗 -> エラー整形 -> 表示 -> 非ゼロ終了（CLI）/エラー通知（GUI）。
- Flow-004: GUI 起動 -> 非同期インデックス -> 最新要求優先検索（古い要求を破棄） -> プレビュー -> 実行/オープン。

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

## Non-functional design
- DES-006 Performance
- Indexer と search を分離し、GUI ではワーカースレッドで非同期処理する。
- 検索要求は入力ごとに発行しつつ、ワーカーでキューを集約して最新要求のみ処理する。
- インデックス再読込開始時は保留中の検索 request_id を破棄し、旧スナップショット由来の検索応答が UI 結果を上書きしないようにする。
- 非空クエリ時は再読込後の最初のインデックスバッチで検索を即時再開し、中断後の復帰遅延を最小化する。
- 検索は絞り込み後にスコア計算し、上位 `limit` を返す。
- GUI の逐次反映は2系統とする: 空クエリはインデックス蓄積分を即時表示、非空クエリは一定件数/時間の閾値を満たしたときだけ検索用スナップショットを更新する。
- regex モードは include term をクエリ単位で事前コンパイルし、候補ごとの再コンパイルを禁止する。
- プレビューキャッシュは固定上限（FIFO）で運用し、長時間セッションでのメモリ増加を抑制する。

- DES-007 Reliability / Error
- 失敗は `anyhow::Result` に集約し、CLI/GUI で表示責務を分離する。
- 外部コマンドは引数配列で起動し、シェル解釈を避ける。
- GUI 検索はワーカーからエラー文字列を受け取り、notice に反映する。
- Windows の `.ps1` 実行は `powershell.exe -File` を明示起動する。

- DES-008 Testability
- indexer/search/actions/ui_model を独立モジュール化。
- OS 依存処理は抽象境界を薄くして単体テスト可能性を維持。

## Error handling / timeout / logging / metrics
- エラー戦略: ファイルアクセス失敗、実行失敗、正規表現不正を分類して表示。
- タイムアウト: 外部プロセス起動はブロッキング待機しない。
- ログ: 現状は標準出力/標準エラー中心。必要に応じて構造化ログへ拡張。
- メトリクス: 検索遅延(ms)と候補件数を測定対象とする。

## Migration / rollback
- 移行: Rust 本実装を正として機能追加する。
- ロールバック: 不安定な変更は小さな単位で revert し、仕様ID単位で影響範囲を判断する。

## Trade-offs
- GUI フレームワークは `egui/eframe` を採用し、クロスプラットフォーム性と開発速度を優先。
- 検索アルゴリズムは完全互換より操作体験優先で調整可能とするが、SP-003 の演算子契約は維持する。

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
