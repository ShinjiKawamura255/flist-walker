# Manual Regression and Traceability

## Environment and data
- Rust stable toolchain（`rustup` 管理）
- OS: Windows 11 / Linux / macOS
- Data:
- 一時ディレクトリに擬似ファイル/フォルダを生成
- `FileList.txt`/`filelist.txt` をケース別に生成
- UNC root 相当のパス比較は Windows 実機またはパス正規化の unit test で確認
- 自己更新手動試験では、必要に応じてローカル HTTP サーバーで release JSON / asset / `SHA256SUMS` / `SHA256SUMS.sig` を配信する

## Self Update Manual Test
1. 同一 version の表示確認:
Windows は `scripts/manual-self-update-test.ps1 -Mode SameVersion` を使い、current version と同じ `tag_name` を返す feed でも更新ダイアログが出ることを確認する。必要なら `-AppPath` で検証対象 exe、`-UpdateBinaryPath` で配信用 binary を差し替える。
2. downgrade 表示確認:
Windows は `scripts/manual-self-update-test.ps1 -Mode Downgrade` を使い、旧 version の release JSON を自動生成して downgrade 候補でもダイアログが出ることを確認する。patch が `0` の場合は `-FeedVersion` を明示する。
3. ローカル feed 確認:
通常は PowerShell helper が sandbox、`latest.json`、対象 binary、sidecar 文書（`*.README.txt`, `*.LICENSE.txt`, `*.THIRD_PARTY_NOTICES.txt`）、`SHA256SUMS`、`SHA256SUMS.sig`、ローカル HTTP server をまとめて用意する。手動で feed を組みたい場合だけ、release JSON、対象 asset、sidecar 文書、`SHA256SUMS`、`SHA256SUMS.sig` を任意ディレクトリへ置き、`FLISTWALKER_UPDATE_FEED_URL` から参照する。
4. update 適用確認:
Windows/Linux 実機で `Download and Restart` を押し、現行プロセス終了後に新 binary が起動し直すことを確認する。更新先ディレクトリに `README.txt` / `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` が新 release の内容で配置されることも合わせて確認する。Windows helper は sandbox 起動なので、元の build 出力が変更されていないことも確認する。
5. suppress 確認:
更新ダイアログで `Don't show again until the next version` をチェックして Later を押し、同じ feed version では次回起動しても再表示されず、feed version を 1 つ上げると再表示されることを確認する。
6. startup check failure dialog 確認:
ローカル feed を無効 URL に向ける、または `api.github.com` 到達不能環境を再現し、`Update Check Failed` ダイアログに原因が表示されることを確認する。`Don't show this again for update check errors` をチェックした場合は次回起動で同種ダイアログが出ないことも確認する。
7. startup check failure dialog 強制表示:
`FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE=1 cargo run --bin flistwalker -- --root .. --limit 1000` で起動し、ネットワーク状態に関係なく `Update Check Failed` ダイアログが表示されることを確認する。必要なら `FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE=\"debug message\"` で表示文言を固定する。

## Diagnostics Trace Smoke Test
1. worker-side trace:
`RUST_LOG=flist_walker::app::index_worker=info cargo test index_worker_trace_smoke_emits_canonical_fields --lib -- --nocapture` を実行し、Started/Finished 応答が通ることに加えて、出力ログに `flow=index`、`event=started|finished|completed`、`request_id=`、`source_kind=walker` が含まれることを確認する。
2. window trace:
window/session/input/update の observable output を変更した場合だけ、`FLISTWALKER_WINDOW_TRACE=1 FLISTWALKER_WINDOW_TRACE_PATH=<temp-path> cargo run --bin flistwalker -- --root .. --limit 1000` または対象 GUI 操作に相当する手順を実施し、変更対象 family の event が trace file に出ることを確認する。window trace に変更がない場合は `not needed` と記録する。

## Structural Refactoring GUI Smoke Test
Phase 9 の structural refactoring 完了判定では、この手順を別環境で 1 回通せば GUI smoke 合格とみなす。

1. 起動と初期表示
`cargo run --bin flistwalker -- --root .. --limit 1000` で起動し、一覧が表示されること。候補がある場合は 1 行目が選択され、入力欄へすぐ打鍵できることを確認する。
2. 検索入力と応答性
非空 query を数文字入力し、`Backspace`、左右矢印、`ArrowUp` / `ArrowDown` を混ぜても UI が固まらないことを確認する。`'`, `!`, `^`, `$`, `|` を含む query を少なくとも 1 回ずつ試す。
3. プレビュー追従
結果一覧で current row を数回動かし、preview が追従することを確認する。preview 非表示切替がある場合は表示/非表示を切り替えて一覧操作が継続できることも確認する。
4. Root 切替
`Ctrl+O` または root selector で root を変更し、旧 root の preview / current row / pinned selection が残らず、新 root の index が始まることを確認する。
5. Tab 操作
新規 tab 作成、tab 切替、tab close、可能なら tab 並べ替えを行い、tab ごとの query / root / result 状態が混線しないことを確認する。
6. FileList 作成
Walker source で `Create File List` を実行し、確認ダイアログ、進捗表示、完了後の notice が動くことを確認する。FileList source で同操作を行い、background Walker 準備メッセージが出ることも確認する。
7. Sort
`Score`, `Name`, `Modified`, `Created` を切り替え、`Modified` / `Created` 中も入力とスクロールが継続できることを確認する。`Score` へ戻したときに一覧が破綻しないことも確認する。
8. Self-update ダイアログ
更新確認ダイアログを出せる環境なら、表示、Later、抑止チェック、失敗ダイアログのいずれか 1 系統を確認する。難しい場合はこの項目だけ `not run` と記録し、理由を残す。

### Structural Refactoring GUI Smoke Automation Map
| Smoke Step | Current Coverage | Owner Test Modules | Phase Target |
| --- | --- | --- | --- |
| Step 2 検索入力と応答性 | partial automated | `shortcuts.rs`, `window_ime.rs`, `app_core.rs` | manual-heavy な操作継続性を `shortcuts.rs` / owner test へ追加 |
| Step 3 プレビュー追従 | partial automated | `session_tabs.rs`, `session_restore.rs`, `app_core.rs`, `index_pipeline/kind_resolution.rs` | 今回の主対象外 |
| Step 4 Root 切替 | strengthened automated | `shortcuts.rs`, `search_filelist.rs`, `session_tabs.rs` | root dropdown selection と ancestor/use-walker cleanup まで固定済み。manual では実 GUI 操作感を継続確認 |
| Step 5 Tab 操作 | strengthened automated | `session_tabs.rs`, `render_tests.rs`, `shortcuts.rs` | tab switch/close/reorder の代表 flow を固定済み。manual では見た目と連続操作を継続確認 |
| Step 6 FileList 作成 | strengthened automated | `index_pipeline/dialogs_and_inflight.rs`, `index_pipeline/search_filelist.rs`, `index_pipeline/filelist_lifecycle.rs`, `render_tests.rs` | root change cleanup と background walker 準備 flow を固定済み。manual では end-to-end 操作感を継続確認 |
| Step 7 Sort | partial automated | `app_core.rs`, `render_tests.rs` | app_core 依存を増やさず owner seam へ寄せられる範囲だけ追加 |
| Step 8 Self-update ダイアログ | manual-heavy | `update_commands.rs` | Slice C の対象外。manual + Slice D 候補 |

### Structural Refactoring GUI Smoke Test Report Template
- Date:
- Environment: OS / display 構成 / build 種別
- Step 1:
- Step 2:
- Step 3:
- Step 4:
- Step 5:
- Step 6:
- Step 7:
- Step 8:
- Overall: pass / fail
- Notes:

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
- 発生条件: include_files/include_dirs 両有効の FileList ストリーム解析で、候補選択のための per-line `metadata` probe を戻してしまい、Windows 由来 `\` 区切り FileList のインデクシングが遅くなる。
- 期待動作: `\` / `/` の差異は候補順序だけで吸収し、初期ストリームは line-only fast path を維持して metadata-probe baseline を上回る。
- 非対象範囲: include_files / include_dirs のいずれか片方のみ有効な場合の種別判定、root 外候補の実行拒否、階層 FileList の新旧判定、probe が必要な後段の kind 解決。
- 関連テストID: TC-082.

## Regression Guard
- 発生条件: 検索窓フォーカス中に `Ctrl+J` / `Ctrl+M`、`ArrowUp` / `ArrowDown`、`Ctrl+I` を押しても `Results` の実行・移動・PIN トグルが効かない。
- 期待動作: 検索窓フォーカス有無に関わらず、`Ctrl+J` / `Ctrl+M` は実行、`ArrowUp` / `ArrowDown` は current row 移動、`Ctrl+I` は PIN トグルを行う。
- 非対象範囲: IME 合成中は既存仕様通りフォーカス優先で一部ショートカットを抑制する。
- 関連テストID: TC-026, TC-027, TC-028.

## Regression Guard
- 発生条件: 起動直後または `Esc` / `Ctrl+G` で検索をキャンセルした直後に、候補が存在するのに current row が `None` のままになる。
- 期待動作: 候補がある場合は 1 行目を既定選択として表示し、検索結果の再適用で意図せず未選択へ戻さない。
- 非対象範囲: 検索結果更新時の行番号維持、手動で未選択へ戻した状態の保持、Root 変更による selection 破棄。
- 関連テストID: TC-070.

## Regression Guard
- 発生条件: `msvc` から `x86_64-pc-windows-gnu` へ切り替えた後、`windres` 生成物が最終 `flistwalker.exe` に入らず Windows Explorer で既定アイコン表示になる。
- 期待動作: GNU Windows ビルドでは `resource.o` を最終 `flistwalker` バイナリへ直接リンクし、Explorer 上で埋め込みアイコンが表示される。
- 非対象範囲: 実行中ウィンドウアイコン、`.lnk` ショートカット個別設定、ファイル関連付けアイコン。
- 関連テストID: TC-090.

## Regression Guard
- 発生条件: ある変更以降、GUI を閉じるたびに worker shutdown timeout まで待たされ、終了操作が体感で重くなる。
- 期待動作: shutdown 要求と request channel 切断後は短時間の join budget だけ待ち、残存 worker があっても close は 250ms 程度の待ちで返る。
- 非対象範囲: OS がプロセス自体を強制終了できないケース、個別 worker の I/O 完了保証。
- 関連テストID: TC-091.

## Regression Guard
- 発生条件: 起動時の更新確認が UI スレッドを塞ぎ、検索欄入力や一覧操作が数秒単位で固まる。
- 期待動作: 更新確認とダウンロードは worker で非同期実行され、失敗しても GUI の通常操作を継続できる。
- 非対象範囲: GitHub 側の API / asset 配信停止そのもの。
- 関連テストID: TC-078.

## Regression Guard: manual-self-update-sidecars

- Scenario: 手動の自己更新 feed が binary と `SHA256SUMS` だけを配り、`*.README.txt` を含む sidecar 文書を生成しないまま自己更新確認に使われる。
- Expected Behavior: `scripts/manual-self-update-test.ps1` は production 相当の feed として binary、`*.README.txt`、`*.LICENSE.txt`、`*.THIRD_PARTY_NOTICES.txt`、`SHA256SUMS`、`SHA256SUMS.sig` を必ず生成し、更新後の sandbox に `README.txt` / `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` がそろって配置される。
- Non-goals: GitHub Releases 本番 asset の内容妥当性確認、README 本文そのものの文言レビュー。
- Related Tests: Self Update Manual Test step 3, step 4.
- Notes for Future Changes: 自己更新 asset 名や sidecar 種別を増減させた場合は、この helper script と manual test 手順を同一変更で更新すること。

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
- TC-141 -> SP-010, SP-016 -> DES-009, DES-017 -> FR-007, FR-026
- TC-142 -> SP-010, SP-016 -> DES-009, DES-017 -> FR-007, FR-026
- TC-143 -> SP-010, SP-016 -> DES-009, DES-017 -> FR-007, FR-026
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
- TC-084 -> SP-001, SP-010 -> DES-001, DES-009 -> FR-001, FR-010
- TC-085 -> SP-001, SP-010 -> DES-001, DES-009 -> FR-001, FR-010
- TC-082 -> SP-001, SP-007 -> DES-001, DES-006 -> FR-001, NFR-001
- TC-083 -> SP-002, SP-007 -> DES-002, DES-006 -> FR-002, NFR-001
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
- TC-087 -> SP-001, SP-010 -> DES-001, DES-009 -> FR-001, FR-010
- TC-088 -> SP-001, SP-010 -> DES-007, DES-009 -> FR-010, FR-007
- TC-089 -> SP-001 -> DES-001, DES-007 -> FR-010
- TC-090 -> SP-012 -> DES-012 -> NFR-005
- TC-091 -> SP-008, SP-010 -> DES-007, DES-009 -> NFR-002, FR-007
- TC-092 -> SP-003, SP-010 -> DES-003, DES-009 -> FR-003, FR-007
- TC-068 -> SP-010 -> DES-009 -> FR-007
- TC-069 -> SP-010 -> DES-009 -> FR-017, FR-007
- TC-070 -> SP-010 -> DES-009 -> FR-018, FR-007
- TC-110 -> SP-015 -> DES-016 -> FR-025
- TC-112 -> SP-015 -> DES-016 -> FR-025
- TC-117 -> SP-015 -> DES-016 -> FR-025
- TC-111 -> SP-016 -> DES-017 -> FR-026
- TC-115 -> SP-016 -> DES-017 -> FR-026, AC-027
- TC-116 -> SP-016 -> DES-017 -> FR-026, AC-028
- TC-127 -> SP-016 -> DES-017 -> FR-028, AC-029
- TC-113 -> SP-017 -> DES-018 -> FR-027
- TC-114 -> SP-017 -> DES-018 -> FR-027
- TC-074 -> SP-014 -> DES-014 -> FR-019
- TC-075 -> SP-014 -> DES-014 -> FR-020
- TC-076 -> SP-014 -> DES-014 -> FR-021
- TC-077 -> SP-014 -> DES-014, DES-007 -> FR-023
- TC-078 -> SP-014 -> DES-006, DES-014 -> NFR-007
- TC-081 -> SP-014 -> DES-014 -> FR-024
- TC-086 -> SP-014 -> DES-014 -> FR-019, FR-020
- TC-140 -> SP-014 -> DES-014 -> FR-019, FR-020
- TC-118 -> SP-004 -> DES-004 -> FR-022
- TC-119 -> SP-014 -> DES-014 -> FR-020, FR-023
- TC-120 -> SP-010, SP-014 -> DES-015, DES-014 -> FR-007, FR-019
- TC-121 -> SP-010 -> DES-009 -> FR-007
- TC-128 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-129 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-130 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-131 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-132 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-133 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-134 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-135 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-136 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-137 -> SP-010 -> DES-009 -> FR-007, NFR-004
- TC-138 -> SP-010 -> DES-009 -> FR-029
- TC-139 -> SP-010 -> DES-009 -> FR-030
- TC-144 -> SP-010 -> DES-009 -> FR-031
