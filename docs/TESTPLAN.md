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
- Sec: root 外パス実行拒否、履歴永続化無効化、CI の依存脆弱性検査を確認。

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
| TC-018 | unit | `Ctrl+Shift+C`（macOS では `Cmd+Shift+C`）は検索窓フォーカス時でも結果パスを優先コピーする | SP-010 |
| TC-019 | unit | 大規模 FileList 解析はストリーミングで中断可能（新 request で supersede） | SP-001, SP-007 |
| TC-020 | manual+unit | Windows: 異解像度モニタ跨ぎ時に復元サイズが破綻せず、IME `Space` / `Shift+Space` で最低限の半角スペースが検索窓へ反映される（`CompositionUpdate` 同フレームでも挿入される） | SP-010, SP-011 |
| TC-021 | unit | 検索窓フォーカス中でも `Ctrl+N` / `Ctrl+P` が current row を移動する | SP-010 |
| TC-022 | unit | 検索窓フォーカス中でも `Ctrl+G` / `Esc` で query clear + filter reset が実行される | SP-010 |
| TC-023 | unit | `Tab` / `Shift+Tab` はフォーカス非依存で PIN 固定/解除のみ実行し current row を維持する | SP-010 |
| TC-024 | unit | IME スペースフォールバックと composition commit fallback はカーソル位置へ挿入し、挿入後カーソルへ更新する | SP-010 |
| TC-025 | unit | FileList ストリーミング時に種別不明候補を先行表示し、種別解決後に FILE/DIR とフィルタ状態を反映する | SP-001, SP-010, SP-007 |
| TC-026 | unit | 回帰: 検索窓フォーカス中でも `ArrowUp` / `ArrowDown` で current row が移動する | SP-010 |
| TC-027 | unit | 回帰: `Ctrl+I` は検索窓フォーカス有無を問わず PIN をトグルし current row を維持する | SP-010 |
| TC-028 | unit | 回帰: 検索窓フォーカス中でも `Ctrl+J` / `Ctrl+M` が `Enter` 同等に実行/オープンを起動する | SP-010 |
| TC-029 | unit | プロセス shutdown フラグが set/clear 可能で、シグナル経由の終了要求を保持できる | SP-010, SP-008 |
| TC-030 | unit | 階層 FileList 展開は候補集合中の `FileList.txt` / `filelist.txt` 完全一致エントリのみを対象にし、子 FileList 解析中も supersede で中断できる | SP-001, SP-007 |
| TC-031 | unit | indexing 中の非空クエリ自動再検索は小差分では抑制し、十分な差分蓄積時のみ再検索する | SP-007, SP-010 |
| TC-032 | unit | GUI 終了時の worker join はタイムアウト付きで、完了系/タイムアウト系の両方を検証する | SP-008, SP-010 |
| TC-033 | unit | `Ctrl+R` で履歴検索モードを開始し、履歴候補を最新順で表示する | SP-010 |
| TC-034 | unit | query 履歴は空文字と連続重複を保存しない | SP-010 |
| TC-035 | unit | query 履歴はタブ切り替え後も全タブで共通に参照できる | SP-010 |
| TC-036 | unit | Root 変更時は query 履歴の参照位置のみリセットし、履歴本体は保持する | SP-010 |
| TC-037 | unit | 連続打鍵中は query 履歴を保留し、入力区切り後に最終 query だけを保存する | SP-010 |
| TC-038 | unit | IME 合成中の未確定文字列は query 履歴へ保存せず、確定後の query のみを保存する | SP-010 |
| TC-042 | unit | 履歴検索中の `Enter` / `Ctrl+J` / `Ctrl+M` は選択中履歴を query に展開する | SP-010 |
| TC-043 | unit | 履歴検索中の `Esc` / `Ctrl+G` は開始前 query を復元してキャンセルする | SP-010 |
| TC-044 | unit | `Ctrl+Shift+R` は履歴検索を開始しない | SP-010 |
| TC-045 | unit | query 履歴は 100 件上限を維持し、保存状態へ永続化される | SP-010 |
| TC-039 | unit | 保存済みタブ状態の復元候補は無効 root を除外し、active tab index をクランプする | SP-010 |
| TC-040 | unit | タブ復元時は active tab の root/query/filter を復元し、起動直後の index は active tab だけ開始する | SP-010 |
| TC-041 | unit | 復元された background tab は初回 activate 時にだけ lazy refresh を開始する | SP-010 |
| TC-046 | unit | `FLISTWALKER_RESTORE_TABS=1` の間は `Set as default` が無効化され、既定 root を保存しない | SP-010 |
| TC-064 | unit | タブのドラッグ移動は並び順のみを更新し、active tab の実体と保存順を維持する | SP-010 |
| TC-047 | unit | FileList 作成時は祖先の既存 FileList へ子 FileList 参照を重複なく追記し、親 mtime を維持する | SP-001 |
| TC-048 | unit | 祖先探索は callback/失敗時に即停止し、それ以降の上位階層を処理しない | SP-001 |
| TC-049 | unit | FileList に root 外パスが含まれても候補表示時の追加フィルタを行わず、インデクシング経路の挙動を維持する | SP-007 |
| TC-050 | unit | root 外パスは結果一覧に存在しても execute/open 直前で拒否される | SP-004 |
| TC-051 | unit | UNC root 配下のパスは execute/open 直前の root 判定で許可される | SP-004 |
| TC-052 | unit | Create File List で祖先 FileList 追記がありうる場合は確認ダイアログを要求する | SP-001, SP-010 |
| TC-053 | unit | 祖先追記確認を拒否した場合、root 直下の FileList 作成だけを継続する | SP-001, SP-010 |
| TC-054 | unit | `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` のとき query history を保存も復元も行わない | SP-010 |
| TC-055 | manual | README / release docs / release template に平文 history 保存と notarization 手順が明記されている | SP-010, SP-012 |
| TC-056 | integration | CI は Linux/macOS/Windows を対象にし、`cargo audit` を実行する | SP-012 |
| TC-057 | unit | `Score` / `Name` / `Modified` / `Created` のソートモード遷移と `Score` 復帰を検証する | SP-013 |
| TC-058 | unit | query 変更や結果更新時にソートを破棄し、インデクシング経路へ属性取得を追加しない | SP-013 |
| TC-059 | unit | 日付ソートの未キャッシュ path は別 worker へ要求し、古い応答は request_id で破棄する | SP-013 |
| TC-060 | unit | sort metadata cache は上限を超えて増加せず、古い項目を破棄する | SP-013 |
| TC-061 | unit | `created()` 取得不可の項目は `Created` ソート時に末尾へ送る | SP-013 |
| TC-062 | manual | 結果ペインの `Sort` UI から各ソートを選択でき、`Modified` / `Created` 中も入力が継続できる | SP-010, SP-013 |
| TC-063 | perf | ソート機能追加後もインデクシング時の属性取得数を増やさず、既存の逐次表示挙動を維持する | SP-007, SP-013 |

## Runner and commands
- Runner: `cargo test`
- Runner: `cargo test`, `cargo audit`
- Commands:
- `cd rust`
- `source ~/.cargo/env`
- `cargo test`
- `cargo audit`
- GUI 手動試験: `cargo run -- --root .. --limit 1000`
- CLI 動作確認: `cargo run -- --cli "main" --root .. --limit 20`

## Environment and data
- Rust stable toolchain（`rustup` 管理）
- OS: Windows 11 / Linux / macOS
- Data:
- 一時ディレクトリに擬似ファイル/フォルダを生成
- `FileList.txt`/`filelist.txt` をケース別に生成
- UNC root 相当のパス比較は Windows 実機またはパス正規化の unit test で確認

## Entry / Exit criteria
- Entry:
- docs 4文書が揃い、FR/NFR/SP/DES/TC が対応付け済み。
- Exit:
- P0/P1 テストが全成功。
- 未達項目は TODO と根拠を記録。

## Report
- `cargo test` 結果を作業ログに記録。
- 性能計測は `docs/perf-notes.md`（必要時追加）へ記録。

## Regression Guard
- 発生条件: 検索窓フォーカス中に `Ctrl+J` / `Ctrl+M`、`ArrowUp` / `ArrowDown`、`Ctrl+I` を押しても `Results` の実行・移動・PIN トグルが効かない。
- 期待動作: 検索窓フォーカス有無に関わらず、`Ctrl+J` / `Ctrl+M` は実行、`ArrowUp` / `ArrowDown` は current row 移動、`Ctrl+I` は PIN トグルを行う。
- 非対象範囲: IME 合成中は既存仕様通りフォーカス優先で一部ショートカットを抑制する。
- 関連テストID: TC-026, TC-027, TC-028.

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
- TC-018 -> SP-010 -> DES-009 -> FR-007
- TC-019 -> SP-001, SP-007 -> DES-001, DES-006 -> FR-001, NFR-001
- TC-020 -> SP-010, SP-011 -> DES-011, DES-009 -> FR-007, NFR-004
- TC-021 -> SP-010 -> DES-009 -> FR-007
- TC-022 -> SP-010 -> DES-009 -> FR-007
- TC-023 -> SP-010 -> DES-009 -> FR-007
- TC-024 -> SP-010 -> DES-009 -> FR-007
- TC-025 -> SP-001, SP-010, SP-007 -> DES-001, DES-006, DES-009 -> FR-001, FR-007, NFR-001
- TC-026 -> SP-010 -> DES-009 -> FR-007
- TC-027 -> SP-010 -> DES-009 -> FR-007
- TC-028 -> SP-010 -> DES-009 -> FR-007
- TC-029 -> SP-010, SP-008 -> DES-009, DES-007 -> FR-007, NFR-002
- TC-031 -> SP-007, SP-010 -> DES-006, DES-009 -> NFR-001, FR-007
- TC-032 -> SP-008, SP-010 -> DES-007, DES-009 -> NFR-002, FR-007
- TC-033 -> SP-010 -> DES-009, DES-007 -> FR-007
- TC-034 -> SP-010 -> DES-009 -> FR-007
- TC-035 -> SP-010 -> DES-009 -> FR-007
- TC-036 -> SP-010 -> DES-007, DES-009 -> FR-007
- TC-037 -> SP-010 -> DES-009 -> FR-007
- TC-038 -> SP-010 -> DES-009, DES-011 -> FR-007
- TC-039 -> SP-010 -> DES-009 -> FR-007
- TC-040 -> SP-010 -> DES-009, DES-006 -> FR-007
- TC-041 -> SP-010 -> DES-009, DES-006 -> FR-007
- TC-046 -> SP-010 -> DES-009 -> FR-007
- TC-064 -> SP-010 -> DES-009 -> FR-016
- TC-047 -> SP-001 -> DES-001, DES-007 -> FR-001
- TC-048 -> SP-001 -> DES-001, DES-007 -> FR-001
- TC-049 -> SP-007 -> DES-006 -> FR-008
- TC-050 -> SP-004 -> DES-004, DES-007 -> FR-009
- TC-051 -> SP-004 -> DES-004, DES-007 -> FR-009
- TC-052 -> SP-001, SP-010 -> DES-007, DES-009 -> FR-010
- TC-053 -> SP-001, SP-010 -> DES-007, DES-009 -> FR-010
- TC-054 -> SP-010 -> DES-007, DES-009 -> FR-011
- TC-055 -> SP-010, SP-012 -> DES-007, DES-012 -> FR-011, NFR-005
- TC-056 -> SP-012 -> DES-012 -> NFR-005
- TC-057 -> SP-013 -> DES-013 -> FR-012
- TC-058 -> SP-013 -> DES-006, DES-013 -> FR-013, FR-014, NFR-006
- TC-059 -> SP-013 -> DES-013 -> FR-012, FR-014
- TC-060 -> SP-013 -> DES-013 -> FR-015
- TC-061 -> SP-013 -> DES-013 -> FR-012
- TC-062 -> SP-010, SP-013 -> DES-009, DES-013 -> FR-007, FR-012
- TC-063 -> SP-007, SP-013 -> DES-006, DES-013 -> FR-013, NFR-006
