# TESTPLAN

## Scope and priority
- Target: FileList 優先ロジック、walker、検索、実行/オープン分岐、CLI 契約、GUI 主要フロー、逐次表示/キャッシュ境界。
- Priority:
- P0: FR-001/002/003/004/005
- P1: FR-006/007, NFR-002/003/004
- P2: NFR-001（性能計測）

## Test levels
- Unit:
- FileList 検出/解析
- walker 走査
- fuzzy 検索順位
- action 分岐
- GUI ロジック（ui_model）
- Integration:
- 一時ディレクトリで index -> search -> action 連携を確認。
- CLI 実行で出力契約を確認。
- GUI Manual:
- 起動、検索、選択、プレビュー、実行/オープン、再読込を手順化して検証。
- Perf/Sec:
- Perf: 10万件相当ダミー候補で検索時間計測。
- Sec: コマンド引数を配列化しシェルインジェクションを回避。

## Test cases
| TC ID | Level | Purpose | Related SP |
| --- | --- | --- | --- |
| TC-001 | unit | `FileList.txt` / `filelist.txt` の優先検出 | SP-001 |
| TC-002 | unit | リスト未存在時に walker 走査で候補生成 | SP-002 |
| TC-003 | unit | ファジー検索が関連度順・件数制限を満たす | SP-003 |
| TC-004 | unit | 実行可能ファイルで execute 分岐 | SP-004 |
| TC-005 | unit | フォルダで open 分岐 | SP-005 |
| TC-006 | integration | CLI 契約（`--cli`/`--root`/`--limit`）を満たす | SP-006 |
| TC-007 | perf | 10万件検索の遅延計測 | SP-007 |
| TC-008 | unit | 例外時に非ゼロ終了コードを返す | SP-008 |
| TC-009 | unit | モジュール分離により単体テスト可能 | SP-009 |
| TC-010 | manual+unit | GUI で検索/選択/実行/再読込が行える | SP-010 |
| TC-011 | manual | GUI 回帰手順に基づく再検証が可能 | SP-011 |
| TC-012 | unit | プレビューキャッシュが上限件数を超えない | SP-007 |
| TC-013 | manual | インデックス中に空クエリ一覧が逐次更新され、操作が継続できる | SP-002, SP-007 |
| TC-014 | unit | FileList 作成の成功/失敗応答で GUI 状態遷移が正しい | SP-010, SP-008 |
| TC-015 | integration | CLI 非空クエリ出力形式と root 不正系を検証する | SP-006, SP-008 |
| TC-016 | unit | Root 変更時に旧 Root 由来の選択状態（current/pinned/preview）がクリアされる | SP-010 |
| TC-017 | unit | Root 変更時に旧 Root 向け FileList 上書き確認が破棄される | SP-010 |

## Runner and commands
- Runner: `cargo test`
- Commands:
- `cd rust`
- `source ~/.cargo/env`
- `cargo test`
- GUI 手動試験: `cargo run -- --root .. --limit 1000`
- CLI 動作確認: `cargo run -- --cli "main" --root .. --limit 20`

## Environment and data
- Rust stable toolchain（`rustup` 管理）
- OS: Windows 11 / Linux / macOS
- Data:
- 一時ディレクトリに擬似ファイル/フォルダを生成
- `FileList.txt`/`filelist.txt` をケース別に生成

## Entry / Exit criteria
- Entry:
- docs 4文書が揃い、FR/NFR/SP/DES/TC が対応付け済み。
- Exit:
- P0/P1 テストが全成功。
- 未達項目は TODO と根拠を記録。

## Report
- `cargo test` 結果を作業ログに記録。
- 性能計測は `docs/perf-notes.md`（必要時追加）へ記録。

## Traceability (excerpt)
- TC-001 -> SP-001 -> DES-001 -> FR-001
- TC-002 -> SP-002 -> DES-002 -> FR-002
- TC-003 -> SP-003 -> DES-003 -> FR-003
- TC-004 -> SP-004 -> DES-004 -> FR-004
- TC-005 -> SP-005 -> DES-004 -> FR-005
- TC-006 -> SP-006 -> DES-005 -> FR-006
- TC-007 -> SP-007 -> DES-006 -> NFR-001
- TC-008 -> SP-008 -> DES-007 -> NFR-002
- TC-009 -> SP-009 -> DES-008 -> NFR-003
- TC-010 -> SP-010 -> DES-009 -> FR-007
- TC-011 -> SP-011 -> DES-010 -> NFR-004
- TC-012 -> SP-007 -> DES-006 -> NFR-001
- TC-013 -> SP-002, SP-007 -> DES-006 -> NFR-001
- TC-014 -> SP-010, SP-008 -> DES-009, DES-007 -> FR-007, NFR-002
- TC-015 -> SP-006, SP-008 -> DES-005, DES-007 -> FR-006, NFR-002
- TC-016 -> SP-010 -> DES-007, DES-009 -> FR-007
- TC-017 -> SP-010 -> DES-007, DES-009 -> FR-007
