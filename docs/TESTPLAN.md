# TESTPLAN

## Scope and priority
- Target: FileList 優先ロジック、walker、検索、実行/オープン分岐、CLI 契約、GUI 主要フロー。
- Priority:
- P0: FR-001/002/003/004/005
- P1: FR-006/007, NFR-002/003/004
- P2: NFR-001（性能計測）

## Test levels
- Unit:
- FileList 検出/解析
- walker 走査
- fuzzy 検索順位
- action 分岐（モック）
- Integration:
- 一時ディレクトリで end-to-end に近いインデックス→検索→選択。
- E2E:
- Python CLI 実行（非対話引数モード）で結果表示を確認。
- GUI Manual:
- 起動、検索、選択、プレビュー、実行/オープン、再読込を `docs/GUI-TESTPLAN.md` に従って検証。
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
| TC-006 | integration | Python CLI 契約が Rust 移植前提を満たす | SP-006 |
| TC-007 | perf | 10万件検索の遅延計測 | SP-007 |
| TC-008 | unit | 例外時に非ゼロ終了コードを返す | SP-008 |
| TC-009 | unit | 機能分離によりモジュール単体テスト可能 | SP-009 |
| TC-010 | manual+unit | GUI で検索/選択/実行/再読込が行える | SP-010 |
| TC-011 | manual | GUI テスト計画に基づく回帰を実施できる | SP-011 |

## Runner and commands
- Runner: `pytest`
- Commands:
- `python -m venv .venv && source .venv/bin/activate`
- `pip install -e .[dev,gui]`
- `pytest -q`
- GUI 手動試験: `fast-file-finder --gui --root .`

## Environment and data
- Python 3.11+
- OS: Windows 11 (WSL 開発), Linux/macOS は互換確認対象
- Data:
- 一時ディレクトリに擬似ファイル/フォルダを生成
- `FileList.txt`/`filelist.txt` をケース別に生成

## Entry / Exit criteria
- Entry:
- docs 4文書が揃い、FR/NFR/SP/DES/TC が対応付け済み
- Exit:
- P0/P1 テストが全成功
- 未達項目は TODO と根拠を記録

## Report
- `pytest` 結果を作業ログに記録。
- 性能計測は別途 `docs/perf-notes.md`（将来追加）へ記録。

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
