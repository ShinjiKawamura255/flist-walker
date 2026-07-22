# Manual Regression and Traceability

## Environment and data
- Rust stable toolchain（`rustup` 管理）
- OS: Windows 11 / Linux / macOS
- Data:
- 一時ディレクトリに擬似ファイル/フォルダを生成
- `FileList.txt`/`filelist.txt` をケース別に生成
- UNC path form は unit test、real UNC の解決・認可は Windows 実機の TC-051 procedure で確認
- 自己更新手動試験では、必要に応じてローカル HTTP サーバーで release JSON / asset / `SHA256SUMS` / `SHA256SUMS.sig` を配信する

## Action Authorization Platform Evidence (TC-050 / TC-051)

この確認は OS action backend を起動せず、worker の recording executor または authorization-only test seam を使用する。選択時の display path、最後に認可された execution path、backend call count、notice、実行環境を証跡として記録する。外部 canonical destination や実在する UNC の機微情報はマスクする。

### Windows junction / link
1. 同一ローカル volume の一時領域に `root/in-root` と `outside` を作成し、`root/escape-junction` から `outside` への directory junction を作る。権限があれば同じ配置の directory/file symlink も作る。
2. `root/escape-junction` を direct action と open-containing-folder の対象にして worker authorization seam を実行する。
3. `root/in-root` の通常対象を含む複数選択に escape junction を混在させ、全 target の事前認可失敗を確認する。
4. recording executor の call count がすべて 0、notice が display path を示し外部 canonical destination を含まないことを確認する。
5. root 内 file link が root 外の file を指す配置では、direct action は拒否し、open-containing-folder は root 内の字句的親を解決した execution path だけを 1 回受け取ることを確認する。
6. junction/link の作成権限がない場合は `not run`、OS/build、失敗した作成操作と理由を記録し、pass として扱わない。一時領域は証跡取得後に削除する。

### Real UNC / Windows path forms
1. 実際に到達可能な UNC root と、その配下 target、同一 share の root 外 target、可能なら別 share target を用意する。認証情報と server/share の実名は証跡へ残さない。
2. authorization-only seam で同一 UNC root 配下が許可され、その解決済み execution path が recording executor に 1 回渡ることを確認する。
3. 同一 share の root 外、別 share、解決不能 UNC は backend call 0 件で拒否されることを確認する。
4. unit evidence では Windows の大小文字差、extended/verbatim prefix、drive-relative、rooted-without-drive を含む UI precheck 表を確認し、曖昧な形式が UI だけで許可されず `Defer` されることを確認する。
5. real UNC が利用できない場合は `not run` と理由を記録する。synthetic path の unit test は real UNC の代替 pass にしない。

### Evidence record
- Date / operator:
- OS / build / filesystem:
- Case: junction / symlink / real UNC / Windows path forms
- Expected result:
- Actual authorization result:
- Recording executor call count and masked execution path:
- Notice display-path check:
- Status: pass / fail / not run
- Residual note: 最終再検証後の filesystem object 差し替えは handle-relative launch がない backend では完全に排除できず、TOCTOU residual risk として残る。

#### Evidence 2026-07-19
- Date / operator: 2026-07-19 / Codex
- OS / build / filesystem: Microsoft Windows NT 10.0.26200.0 / NTFS
- Case: directory junction escape (direct action / open-containing-folder / mixed selection)
- Test data: `C:\tmp\flistwalker-tc051-<masked>\root\escape-junction` -> sibling `outside`; root 内に通常ファイルを併置
- Expected result: 全 target の事前認可を拒否し、recording executor call count は 0。notice は外部 canonical destination を含まない。
- Actual authorization result: `cargo test --locked tc_051_windows_junction_escape_manual_evidence --lib -- --ignored --nocapture` が pass（1 passed）。direct/open-containing-folder とも要求全体を拒否した。
- Recording executor call count and masked execution path: 0 / none
- Notice display-path check: `Action blocked:` を返し、外部 canonical destination の文字列を含まないことを assertion で確認
- Windows path-form automated evidence: case 違いと extended/verbatim prefix の同一実体を worker が許可して canonical execution path を recording executor へ渡す test、および非 Unicode path を lossy normalization 前に UI が `Defer` する test が pass
- Status: pass
- Cleanup: junction を先に削除後、検証用 `C:\tmp\flistwalker-tc051-<masked>` を削除済み
- Revalidation: source type の厳格判定、display-only failure notice、structured result trace の追加後に同じ ignored test を再実行し、1 passed。再作成した一時 junction/tree も同じ手順で削除済み。
- Case: real UNC
- Expected result: 同一 root 配下だけを許可し、同一 share の root 外、別 share、解決不能 UNC は backend call 0 件で拒否する。
- Actual authorization result: 到達可能な検証用 UNC share がこの環境にないため未実行
- Status: not run
- Residual note: synthetic UNC path の UI precheck test は pass しているが、real UNC の worker canonicalization evidence の代替にはしない。

## Self Update Manual Test
この節は VM-005 の operator-only 手順であり、通常の GUI smoke や closure validation では実行しない。`scripts/manual-self-update-test.ps1` が作成する private sandbox の copied executable と loopback 上の inert local feed だけを対象にし、production executable、production feed、外部 network endpoint を指定しない。`Download and Restart` を押す activation 手順は明示承認を得た場合だけ実行する。

1. 同一 version の表示確認:
Windows は `scripts/manual-self-update-test.ps1 -Mode SameVersion` を使い、helper が private sandbox へコピーした executable と current version と同じ `tag_name` の local feed で更新ダイアログが出ることを確認する。`-AppPath` / `-UpdateBinaryPath` は sandbox へコピーする test input だけに使い、実行中または配布用 production binary を指定しない。
2. downgrade 表示確認:
Windows は `scripts/manual-self-update-test.ps1 -Mode Downgrade` を使い、旧 version の release JSON を自動生成して downgrade 候補でもダイアログが出ることを確認する。patch が `0` の場合は `-FeedVersion` を明示する。
3. ローカル feed 確認:
PowerShell helper が private sandbox、`latest.json`、inert 対象 binary、sidecar 文書（`*.README.txt`, `*.LICENSE.txt`, `*.THIRD_PARTY_NOTICES.txt`）、`SHA256SUMS`、`SHA256SUMS.sig`、loopback HTTP server をまとめて用意する。外部 URL や手作業の production-like feed へ切り替えない。
4. update 適用確認:
明示承認を得た Windows/Linux の test environment で、helper が表示した sandbox copy に限って `Download and Restart` を押し、新 binary の再起動と sandbox 内 `README.txt` / `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` を確認する。元の build 出力の hash と timestamp が変わっていないことを確認する。
5. suppress 確認:
更新ダイアログで `Don't show again until the next version` をチェックして Later を押し、同じ feed version では次回起動しても再表示されず、feed version を 1 つ上げると再表示されることを確認する。
6. startup check failure dialog 確認:
helper の loopback server を停止した sandbox、または deterministic failure injection を使い、外部 network へ接続せず `Update Check Failed` の状態遷移と抑止を確認する。
7. startup check failure dialog 強制表示:
`cargo test update_check_failure_opens_failure_dialog --lib` と `cargo test forced_update_check_failure_bypasses_suppression_flag --lib` を実行し、network access なしで failure dialog state と suppression bypass を確認する。native focus/visual confirmation が必要な場合は `GUI-TESTPLAN.md` の GSM-008 として staged window で記録し、専用の安全な staged launch route がなければ `NOT RUN` とする。

## Diagnostics Trace Smoke Test
1. worker-side trace:
`RUST_LOG=flist_walker::app::index_worker=info cargo test index_worker_trace_smoke_emits_canonical_fields --lib -- --nocapture` を実行し、Started/Finished 応答が通ることに加えて、出力ログに `flow=index`、`event=started|finished|completed`、`request_id=`、`source_kind=walker` が含まれることを確認する。
2. window trace:
window/session/input/update の observable output を変更した場合は、まず owner test（例: `cargo test update_check_failed_response_emits_trace_command --lib`）で変更対象 family の event を確認する。native trace file が必要な場合は dedicated staged trace launch だけを使用し、workspace debug executable を直接起動しない。dedicated route がない場合は native trace を `NOT RUN`、理由と owner test evidence を記録する。window trace に変更がない場合は `not needed` とする。

## Transactional Updater Platform Evidence (TC-160)
1. Windows と Linux でそれぞれ同一 filesystem の private temporary directory を作り、copied-helper transaction core に old/new binary と 3 sidecar の inert text fixture を渡す。production executable/feed は使用しない。
2. success injection で sidecars-first/binary-last の操作順、完全な new hash、startup cleanup 後の lock/marker/ack/backup 不在を記録する。
3. 各 sidecar 適用後と `binary_intent` 後の precommit failure injection で old bundle hash へ戻り、originally-absent target が残らないことを記録する。
4. binary commit 後の restart failure injection で old bundle へ rollback し、旧 binary の再起動は recording backend への call としてのみ確認する。
5. `prepared_parent_owned`、`helper_registered` ack 前後、各 target の `intent/applied`、`binary_intent/binary_committed` marker fixture を再読込し、live matching helper の回復除外、precommit rollback、committed promotion、ambiguous retention を記録する。
6. Windows adapter は existing target の PowerShell `File.Replace(..., false)`、Linux adapter は backup/file/parent sync と same-directory rename が実際に成功することを確認する。いずれかの platform 証跡が無い場合、TC-160 と Slice E は未完了とする。

- Evidence directory: test-owned private temporary directories created by `staging::test_unique_update_temp_dir`; each fixture removes its exact directory after assertion.
- Windows result: pass (`cargo test tc160_windows_file_replace_preserves_the_old_dummy_file_as_backup --lib`: 1 passed; `cargo test tc157_ / tc158_ / tc159_ --lib`: 12 / 5 / 22 passed). The adapter executed PowerShell `[System.IO.File]::Replace(..., false)` against inert text files and preserved the old target as backup.
- Linux result: pass under WSL2 Ubuntu (`cargo test tc160_linux_synced_rename_preserves_the_old_dummy_file_as_backup --lib`: 1 passed; `cargo test tc157_ / tc158_ / tc159_ --lib`: 12 / 5 / 22 passed). The production Linux adapter executed synced backup + same-directory rename against inert text files.
- Failure points covered: pre-marker orphan preservation/lone-lock cleanup、ack-before-mutation rejection、live-parent pre-mutation deferral、helper executable/hash/token mismatch、parent wait timeout、operation-time target/prepared hash change、absent-target racing destination、cleanup artifact hash/type mismatch、precommit sidecar failure、restart failure rollback、complete `binary_intent` promotion、unknown hash/invalid transition retention、interrupted postcommit rollback resumption。
- Cleanup/remaining artifacts: committed/rolled-back fixtures assert lock/marker removal; ambiguous fixtures intentionally retain lock/marker/`.new` evidence only inside fixture-owned directories, which fixture teardown removes.
- Production binary/external process untouched: yes. Restart behavior used only a recording/no-launch backend; no production update feed or application executable was targeted.

## Structural Refactoring GUI Smoke Test
GUI-adjacent structural refactoring は [GUI-TESTPLAN.md](../GUI-TESTPLAN.md) と [GUI-TESTREPORT.template.md](../GUI-TESTREPORT.template.md) を canonical owner とする。

1. Windows は `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-deterministic-scenarios.ps1`、Linux/macOS/WSL は `scripts/gui-deterministic-scenarios.sh` を実行し、canonical 10 group と zero ignored を確認する。
2. Native observation が必要な場合は `scripts/gui-headful-smoke.sh --duration 300` または `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\gui-headful-smoke.ps1 -DurationSeconds 300` を実行し、helper が表示した isolated staged window だけを操作する。workspace debug executable は直接起動しない。
3. GSM-001..010 の該当 flowで startup/indexing、query operator、preview/selection、root/tab、FileList dialog、sort、theme、responsiveness を確認する。external action、clipboard、update activation は別途明示承認がない限り実行しない。
4. Deterministic、Native interaction、Liveness を独立して記録する。required native axis が `NOT RUN` の場合は overall PASS にせず、理由と再現手順を残す。

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
- Expected Behavior: `scripts/manual-self-update-test.ps1` は sandbox 限定の inert local feed として binary、`*.README.txt`、`*.LICENSE.txt`、`*.THIRD_PARTY_NOTICES.txt`、`SHA256SUMS`、`SHA256SUMS.sig` を必ず生成し、明示承認された更新後の sandbox に `README.txt` / `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` がそろって配置される。production executable/feed と外部 network は対象外とする。
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
- TC-046A -> SP-010 -> DES-009 -> FR-007
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
- TC-064 -> SP-010 -> DES-009 -> FR-016
- TC-068 -> SP-010 -> DES-009 -> FR-007
- TC-069 -> SP-010 -> DES-009 -> FR-017, FR-007
- TC-070 -> SP-010 -> DES-009 -> FR-018, FR-007
- TC-074 -> SP-014 -> DES-014 -> FR-019
- TC-075 -> SP-014 -> DES-014 -> FR-020
- TC-076 -> SP-014 -> DES-014 -> FR-021
- TC-077 -> SP-014 -> DES-014, DES-007 -> FR-023
- TC-078 -> SP-014 -> DES-006, DES-014 -> NFR-007
- TC-081 -> SP-014 -> DES-014 -> FR-024
- TC-082 -> SP-001, SP-007 -> DES-001, DES-006 -> FR-001, NFR-001
- TC-083 -> SP-002, SP-007 -> DES-002, DES-006 -> FR-002, NFR-001
- TC-084 -> SP-001, SP-010 -> DES-001, DES-009 -> FR-001, FR-010
- TC-085 -> SP-001, SP-010 -> DES-001, DES-009 -> FR-001, FR-010
- TC-086 -> SP-014 -> DES-014 -> FR-019, FR-020
- TC-087 -> SP-001, SP-010 -> DES-001, DES-009 -> FR-001, FR-010
- TC-088 -> SP-001, SP-010 -> DES-007, DES-009 -> FR-010, FR-007
- TC-089 -> SP-001 -> DES-001, DES-007 -> FR-010
- TC-090 -> SP-012 -> DES-012 -> NFR-005
- TC-091 -> SP-008, SP-010 -> DES-007, DES-009 -> NFR-002, FR-007
- TC-092 -> SP-003, SP-010 -> DES-003, DES-009 -> FR-003, FR-007
- TC-110 -> SP-015 -> DES-016 -> FR-025
- TC-111 -> SP-016 -> DES-017 -> FR-026
- TC-112 -> SP-015 -> DES-016 -> FR-025
- TC-113 -> SP-017 -> DES-018 -> FR-027
- TC-114 -> SP-017 -> DES-018 -> FR-027
- TC-115 -> SP-016 -> DES-017 -> FR-026, AC-027
- TC-116 -> SP-016 -> DES-017 -> FR-026, AC-028
- TC-117 -> SP-015 -> DES-016 -> FR-025
- TC-118 -> SP-004 -> DES-004 -> FR-022
- TC-119 -> SP-014 -> DES-014 -> FR-020, FR-023
- TC-120 -> SP-010, SP-014 -> DES-015, DES-014 -> FR-007, FR-019
- TC-121 -> SP-010 -> DES-009 -> FR-007
- TC-127 -> SP-016 -> DES-017 -> FR-028, AC-029
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
- TC-140 -> SP-014 -> DES-014 -> FR-019, FR-020
- TC-141 -> SP-010, SP-016 -> DES-009, DES-017 -> FR-007, FR-026
- TC-142 -> SP-010, SP-016 -> DES-009, DES-017 -> FR-007, FR-026
- TC-143 -> SP-010, SP-016 -> DES-009, DES-017 -> FR-007, FR-026
- TC-144 -> SP-010 -> DES-009 -> FR-031
- TC-145 -> SP-018 -> DES-019 -> FR-032, AC-033
- TC-146 -> SP-018 -> DES-019 -> FR-032, AC-033
- TC-147 -> SP-018 -> DES-019 -> FR-032, AC-033
- TC-148 -> SP-018 -> DES-019 -> FR-032, AC-033
- TC-149 -> SP-010 -> DES-009 -> FR-007
- TC-150, TC-151, TC-152, TC-153 -> SP-010 -> DES-006, DES-007, DES-009 -> FR-007, NFR-008
- TC-154 -> SP-010 -> DES-009 -> FR-007, NFR-009
- TC-155 -> SP-003, SP-009 -> DES-003, DES-008 -> FR-003, NFR-003
- TC-156 -> SP-007 -> DES-006 -> NFR-001
- TC-157 -> SP-014 -> DES-014 -> NFR-010
- TC-158, TC-159, TC-160 -> SP-014 -> DES-014 -> FR-033
- TC-159, TC-160 -> SP-014 -> DES-014 -> NFR-010
- TC-161 -> SP-001, SP-007 -> DES-001, DES-006 -> FR-034, NFR-001
