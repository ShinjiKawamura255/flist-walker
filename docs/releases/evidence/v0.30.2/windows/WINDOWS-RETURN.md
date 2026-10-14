# FlistWalker v0.30.2 Windows候補検証結果

検証日時: 2026-10-15 01:20–01:37 Asia/Tokyo。選択profile: Change-focused candidate。

**CLI・TC-193・Windows資産検査はPASS。観測できたnative GUIケースはPASS。dangling linkと読込中tab切替にNOT RUNが残るため、Windows選択範囲全体のrelease gateはBLOCKED。公開可とは判定していない。**

## 候補identity

- Repository: ShinjiKawamura255/flist-walker。
- Source: `c54f0237c4c8b55cfd085dd1b97828b4859b897e`。
- [Release Tagged Build run 38584767783](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/38584767783)、attempt 1、workflow_dispatch/master、completed/success。
- Create Draft Release job `115943014688`: skipped。その他10 jobs: success。GitHub APIでsource/run/attemptを照合。
- Artifact `11906773075`: `validated-release-bundle-v0.30.2-c54f0237c4c8b55cfd085dd1b97828b4859b897e`、expired=false、workflow_run/source一致。指定nameを専用のGit-ignored evidence rootへ取得。
- 28 files / 26 manifest entriesを確認。独立署名検証・全asset内容・N-1互換性のmacOS側PASSはハンドオフの記録として扱い、Windowsの新規実行PASSへ転用していない。

| 実物 | SHA-256 | Windows照合 |
| --- | --- | --- |
| SHA256SUMS | 988f1cd72229890515347f3b2306e89723fffc364209e4cfd45549584679c6f7 | PASS |
| SHA256SUMS.sig | 23567d4505d6b3f8e7be8f865fe7b80c6b611d57602aa786a3f68003378b5096 | PASS（digest照合） |
| FlistWalker-0.30.2-windows-x86_64.exe | 94b765d5d8191a16ee95a460df92e2125b9713abe4cbb21a9e9031d30d26fc3e | PASS |
| fw-0.30.2-windows-x86_64.exe | 8b0c3be9325d21bd02efd6a9a98a122412ea44d15ead35ff986c34264d9e3237 | PASS |

両variantのdirect child `--version`: exit 0、`flistwalker 0.30.2` / `fw 0.30.2`。universalのstdoutはredirectして同期process終了まで待機。GUIは上記universalとhash一致の専用copyを起動した。

## 環境・隔離

- Windows 11 Pro、Professional、26H2、build 26300.9550、native AMD64。
- PowerShell Core 7.6.5、Python 3.12.10 x64、AMD Ryzen 9 5950X、32 logical processors。
- 既存keyring gh認証とMSYS2 objdump/stringsを使用。前回確認済みのsandbox/host差に従いPython・GitHub・CIMを限定host実行。install、永続設定変更、秘密値取得は行っていない。
- childのみHOME/USERPROFILE/LOCALAPPDATA/APPDATAを隔離profileに設定。childのみself-update無効化、RUST_LOG除去。argv arrays、shell=False。
- GUI起動前にsource/hash/path、fixture inventory（bytes/digest/birthtime/mtime）、profile、選択範囲、cleanupを`gui-frozen-session.json`へ凍結。既存FlistWalkerは操作していない。
- 前回のpassive Explorer capture不一致は、今回は候補windowを一意選択して前面化した後、候補の実画像で確認できた。capture手段は`@oai/sky`。入力・scroll・tab・終了は同じtask-owned windowを対象とした。
- 通常負荷はTC-193直前CPU 18%、直後20%。このタスクは測定中に別build/install/testを開始していない。測定前available memory 16,745,412 KiB / total 33,475,296 KiB。瞬間CIM値であり継続CPU監視ではない。

## CLIとTC-193

| 検査 | 結果 | 内容 |
| --- | --- | --- |
| 添付helper disabled-update alias | PASS | 40件、両variant、5 color形式、更新flag前後。check-update exit0/stdoutのみ、update exit1/stderrのみ。実更新は未実行。 |
| 追加disabled-update alias | PASS | universal `--cli`ありで20件。 |
| 追加否定系 | PASS | 両variant・両update flagで28件。不正色値2形式、query、root、interactive、exec、同時update指定はexit2/argument error。 |
| literal query / exec境界 | PASS | 両variantで8件。`--`後のupdate文字列は検索query、exec/-x内のflagはdry-runで更新をdispatchせず。実コマンド起動なし。argv保持の厳密契約は下記exact CIテストもPASS。 |
| TC-193 | PASS | 固定shallow 200-file fixture、5 warmup+25 measurement/variant、交互のrole順、同期direct process、redirect stdout/stderr。 |

| Variant | Median ms | Measurements |
| --- | --- | --- |
| universal（`--cli`） | 82.3507 | 25 |
| fw | 35.2861 | 25 |

**fw/universal = 0.4284857324 ≤ 0.70。初回のみ。再試行・閾値変更・warmup/sample削減・安全機能無効化なし。** 全60 warmup/measurementのexit0、stderr空、200件の内容・順序・framing一致。50 measurement raw valuesは`windows-verification.json`、60回の一覧は`tc193-raw.tsv`、fixture名/bytes/digestsは`tc193-fixture-inventory.json`。

添付helperは保存した原本を維持し、タスク専用wrapperが元のtimer終了後に各subprocessのargv/exit/stdout/stderr/hash/elapsedをJSONLへ保存。計測argv・timer・回数・thresholdは変更していない。原本/wrapper/instrumented source digestはローカル`runner-identity.json`。初回途中失敗時の部分結果も保持する構成で、今回102 subprocess recordsを取得した。

追加検査の初回local oracleには6件の誤判定があった。`--update`はfuzzy subsequenceとして`--check-update.txt`にも一致し、dry-run出力は`Dry run: ...`である。候補は再実行せず保存済みstdout/stderrを正しい契約で再判定。原記録`alias-supplement.json`を保持し、訂正理由と判定を`alias-supplement-reviewed.json`へ分離した。製品FAILやTC-193再試行ではない。

## PE/resource/import

配布binaryそのものをobjdump/stringsで検査。両variantともpei-x86-64、.rsrcあり、RT_ICON=3、RT_GROUP_ICON=14、RT_MANIFEST=24、asInvokerを確認。universal subsystem=2（Windows GUI）、fw subsystem=3（Windows CUI）。fwにGDI32/OpenGL32/imm32/psapi/dwmapi/uxthemeなし、両variantに禁止MSYS2 runtime DLLなし。Shell32/User32は意図された許容import。`pe-results.json`に検査結果とimport一覧を保存。local buildは代用していない。

## GUI結果（三軸を分離）

Deterministicはこの候補sourceのWindows Release Preflight job `115923813364`のログを照合したREUSEであり、ローカルで再実行した主張ではない。1630 lib tests passed / 20 ignored、48 CLI contract tests passed。下記対象名はいずれも`... ok`。ignored全体をnative PASSに転用していない。

| 選択subflow | Deterministic | Native interaction | Liveness / 証跡 |
| --- | --- | --- | --- |
| GSM-003 通常text | PASS（REUSE: file-information/render契約） | PASS：51 B、Created/Updated、UTF-8本文2行、EOF | PASS：候補実画像、矢印選択。`09-normal-tab2-0.jpg` |
| GSM-003 binary / empty | PASS（REUSE: `gui_initial_body_errors_keep_file_information_and_reload_updates_it`） | PASS：256 B/binary content、0 B/empty file、取得可能metadata | PASS：選択と表示更新。`01-binary-0.jpg`、`03-empty-settled-0.jpg` |
| GSM-003 body-read-denied / 回復 | PASS（同上） | PASS：owned fileのみReadData拒否、stat33 B/read PermissionError、metadataとpermission denied表示。元SDDL復元後Reloadで本文回復 | PASS：正常Reload。`05-denied-final-0.jpg`、`06-denied-recovered-0.jpg` |
| GSM-003 削除→Reload→再作成→Reload | PASS（同上、`initial_error_reload_button_dispatches_and_rapid_selection_keeps_latest_information`） | PASS：削除後size/created/updated unavailable、file not found。再作成78 B/3行で情報・本文を再取得 | PASS：UIを操作でき、更新を視認。`07-deleted-reload-0.jpg`、`08-recreated-reload-0.jpg` |
| GSM-003 dangling/unresolved link | PASS（REUSE: source CIの関連error/header契約のみ。native代用なし） | NOT RUN：owned symlink作成がWinError1314、必要privilegeなし | NOT RUN：link fixtureなし。実システムlink代用や権限変更なし |
| GSM-012 追加ページ失敗保持 | PASS（REUSE: `gui_paged_preview_failed_more_keeps_prior_document_and_requires_reload`） | PASS：100行読込後にfixtureを変更、Moreでchanged/reload要求、確定本文・情報保持。元601行bytesを戻しReloadで回復 | PASS：controls応答。`11-more-changed-0.jpg`、`12-paged-reloaded100-0.jpg` |
| GSM-012 100→600→601 | PASS（REUSE: `gui_paged_preview_loads_initial_and_more_without_duplication`、`initial_and_more_pages_preserve_each_line_exactly_once`） | PASS：100/600/601カウンタ、末尾600/601を視認、EOF、More無効 | PASS：scroll/More応答。`10-paged100-0.jpg`、`13-paged600-0.jpg`、`18-paged601-0.jpg` |
| GSM-012 tab間path/body独立性 | PASS（REUSE: `active_more_response_after_tab_roundtrip_keeps_and_extends_prior_body`、`background_more_failure_preserves_tab_document_and_error_is_tab_scoped`、`initial_error_header_is_request_path_and_tab_scoped`） | PASS：tab2の600行からtab1へ切替ても別file78 B/3行を保持、戻って600行を保持 | PASS：Ctrl+Tab往復。`14-tab1-independent-0.jpg`、`15-tab2-retained600-0.jpg` |
| GSM-012 実際にpending中のtab切替・stale response | PASS（REUSE: 上記background/routing tests） | NOT RUN：More終了600行が切替前に既に視認。More→次tab操作7.862秒、pendingを観測していない。PASSに転用せず | 通常tab切替のlivenessはPASS。pending固有のnative軸は未観測 |
| 隣接selection/scroll/hide-show/query | PASS（REUSE: normal-render/focus関連契約） | PASS：double-clickで600を選択、wheel/scrollbar、preview hide/show、query normal入力で1結果とpreview追従 | PASS：実画面変化と応答。`16-body-scroll-0.jpg`、`20-text-selection-0.jpg`、`21-preview-hidden-0.jpg`、`22-preview-shown-0.jpg`、`23-query-responsive-0.jpg` |

初期selection直後には前fileの理由が映る画像があり、後の観測で現在fileのempty/permission-deniedへ更新された。入力の再試行やfixture変更で上書きしていない。初期画像と日時付き観測を保持し、capture/frame/workerのどこで遅れたかは未特定。確定したnative product FAILとしては扱っていない。本文dragでは選択を視認できず、distinctな通常double-click routeで選択を確認。drag route自体をPASSとは記録していない。

公開repoには画像・ACL・profile内容を送っていない。ローカル証跡locatorは`rust/target/gui-smoke/evidence/v0.30.2-windows-candidate-20261015/`。GUI所要観測は凍結sessionと`native-observations.jsonl`のUTC時刻（JST=UTC+9）に記録。画像は同rootのJPG、全raw subprocessは`subprocess-first-run.jsonl`。返却ZIPに画像・ACL・private profileは含めない。

## 正常close・cleanup・変更範囲

- task-owned PID 35724へAlt+F4。即時window inventoryは1件残ったが、hostでPID終了、後続native enumerationで0件を確認。kill/terminateは使っていない。GUI stdout/stderrは空。
- ReadData拒否は削除検査前に保存済み元SDDLへ正確に復元し、読み取り回復を確認。候補終了確認後に絶対pathがtask root内であることを検証し、owned fixture/profile/appのみcleanup。`cleanup.json`に最終fixture digestsと削除結果を保持。
- helper/追加CLIのTemporaryDirectoryも終了時cleanup。候補bundleとローカル証跡は返却・再検証用に保持。
- Worktree review preflight PASS、HEAD `8a452b3858c80f70a5f3064834c8add4d9b1d347`、master、開始/終了ともGit差分なし。他worktree・branchは操作していない。
- `python3 scripts/validate_change.py --base HEAD --plan`: changed paths/selected VMなし。candidateの検証は上記bindingとexact workflow evidenceに対して行った。ローカルHEADをcandidateとみなしていない。
- 製品実装変更がないためTDD/cargo再実行は対象外。安全なcandidate subprocess、raw出力整合確認、exact Windows CI契約と実native観測で検証。
- 実自己更新、live binary置換、外部Open/Reveal/Exec、clipboard、実cloud/UNC、IME、別DPIは未実行でNOT REQUIRED（選択範囲外）。install/security settings/repo settings変更、commit/push/PR/merge/tag/release、他chatへの送信は行っていない。

## macOS release ownerへの残件

1. dangling link nativeケース：現在sessionでは1314で作成不能。native実施には当該fixtureを安全に作成可能なsession/privilegeが必要。権限変更は実施していない。
2. pending More response中のnative tab切替：通常601行fixtureは切替前に完了。厳密なpending native確認は未実施で、exact CIの決定論的PASSと分離する。
3. 上記required NOT RUNを解消するか、release ownerがv0.30.2限定の明示的deviation判断へまとめる。現時点ではdeviation承認なし、Windows結果はBLOCKED。タグ・公開判断はWindows側で行わない。

返却パケットにはsanitized本文、binding、Windows helper結果、raw timings/fixture inventory、追加alias判定、PE/hash結果、負荷観測、cleanup結果を含める。ユーザを経由してmacOS release chatへ返却する。
