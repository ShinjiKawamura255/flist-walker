# Flows, Data Model, and API Contract

## Main flows
- Flow-001: 起動 -> （FileList 優先モード有効時）FileList 検出 -> 読み込み -> 検索 -> 選択 -> アクション。
- Flow-002: 起動 -> FileList なし -> walker 走査 -> 検索 -> 選択 -> アクション。
- Flow-003: アクション失敗 -> エラー整形 -> 表示 -> 非ゼロ終了（CLI）/エラー通知（GUI）。
- Flow-004: GUI 起動 -> 非同期インデックス -> 最新要求優先検索（古い要求を破棄） -> プレビュー -> 実行/オープン。
- Flow-005: GUI 起動 -> update worker が上限付きで GitHub Releases を確認 -> 新版あり -> 利用者承認 -> `SHA256SUMS` / `SHA256SUMS.sig` を先行取得 -> strict parse と署名検証 -> binary/sidecar を private create-new file へ上限付き streaming download/hash 検証 -> `VerifiedUpdateBundle` -> executable parent 内へ同一 directory 準備 -> durable parent/helper registration と acknowledgement -> 本体終了 -> sidecar 適用 -> binary-last atomic commit -> bounded restart。Windows の hidden helper/restart command は stdio を `NUL` へ固定し、helper と restart は最大3ラウンド・100ms間隔、verbatim path時の非verbatim代替を適用する。更新後processはmode別internal flagでterminal handoffを先行し、helper終了後のmarker/hash再検証を終えてからGUI続行またはHeadless終了へ進む。GUI restart はchildの500ms startup graceも適用する。precommit/restart failure は旧 bundle へ rollback し、旧GUIも同じbounded restart契約で起動する。新版・旧版の両起動失敗は両診断を保持し、中断は起動時 marker/hash recovery へ収束する。ignore list sample は別途起動時初期化で補完する。
  `FLISTWALKER_DISABLE_SELF_UPDATE=1`、または実行中バイナリと同一ディレクトリに `FLISTWALKER_DISABLE_SELF_UPDATE` ファイルがある場合は update flow を起動せず、通常起動のみ行う。
- Flow-006: batch CLI -> root selection -> type alias normalization -> index（`--progress` では開始と候補件数/時間を stderr）-> search/full-match name/path/metadata sort -> limit（`--progress` では全一致件数・返却件数/時間と、切り詰め時のwarningを stderr）-> `BufWriter` による逐次 print、preauthorized action、または external command executor。external executor は全 target を lexical root または canonical root 配下として事前認可し、OS の argv budget と optional path cap で stable greedy batch を作り、各 batch 直前の再認可後に direct argv で直列起動する。target 0 件は no-spawn、failure/cancel は残 batch を停止する。
- Flow-007: FileList create -> deterministic write plan/content precompute -> 下位祖先から上位祖先への replacement（既存 permissions と必要な時刻 metadata を一時ファイルへ先行適用）-> root を最後に commit -> success report、または通常の failure/cancel/panic では rollback attempt -> terminal settlement report。replace 後の directory sync failure は current target も committed/rollback 対象へ入れ、durability error を報告する。root commit 前の process abort では旧 root と単調な祖先参照を残し、再実行時の重複除去で完了状態へ収束する。TUI は settlement 後にのみ pending output/root/exit intent を解決する。
- Flow-008: GUI/TUI frame -> `UiStatePatch + history_delta` enqueue -> persistence worker bounded lock -> latest JSON read/merge -> atomic write -> committed generations only clear。利用者が確定する saved-root/default-root は `request_id + SettingsCommitRequest` として同 worker へ送り、canonicalize と UI state / saved roots write を完了してから結果を UI へ返す。observed commit の pending 中は通常 UI-state snapshot を dirty のまま保留し、旧 live state の snapshot が commit 後に新設定を上書きしないようにする。失敗は live state を更新せず draft を保持し、複数ファイル commit の途中失敗は変更済みファイルを rollback する。履歴の失敗は同一 coalesced payload を retry queue に残す。
- Flow-009: interactive CLI -> startup root selector 解決 -> index worker で root metadata 検証 -> immutable candidate batch を event loop へ送信 -> 1 iteration 最大 64 応答を append -> batch snapshot を search worker と共有。root error は `IndexFailed`、backlog があれば次 iteration の poll timeout を 0 にする。
- Flow-009A: interactive selection -> FileList settlement / worker join -> terminal guard の逆順復旧 -> selected path/root を main adapter へ返却 -> print または Flow-006 と同じ external command executor。terminal 復旧前に stdout write または child spawn を行わない。

## Data model
- Candidate
- `path: PathBuf` 正規化済み絶対パス
- `display: String` 画面表示用パス
- `CandidateBatches`: `Arc<Vec<Arc<[PathBuf]>>>` と総件数。増分 append は batch reference のみを copy-on-write し、既存 path allocation を共有する。
- SearchResult
- `candidate: Candidate`
- `score: f64`
- `SortMode`: score/name/path/modified/created/size の方向付き enum。shared sort は full match set と mode を受け、adapter がその後に limit を適用する。
- `AuthorizedActionRequest`: trusted root、current-row snapshot、request identity、cancellation token、action kind。GUI adapter は同じ request identity/root を request-scoped freshness registry へ enqueue 前に登録し、root/tab lifecycle と worker terminal の双方で冪等に失効させる。現在 tab の precheck rejection はその tab の routing/freshness/state だけを消去し、別 tab の background request を維持する。worker disconnect のみ全 action request を消去する。shared report の raw backend detail は内部 trace 専用とし、CLI/GUI の利用者向け通知へ転記しない。
- `FileListWritePlan`: root commit point より前に下位から上位へ並ぶ ancestor targets と最後の root target、expected prior logical contents/metadata、new contents、consent scope。`FileListTransactionReport` は committed/failed/rolled-back/rollback-failed display paths と settlement reason を保持する。
- `UiStatePatch`: named non-history JSON leaf patch と ordered history deltas。worker generation は commit に含めた enqueue range を記録する。
- `MaxDepth`: 無制限または 1 以上の上限。Walker queue は root からの depth を path と一緒に保持し、FileList は filesystem I/O を行わない root 相対 lexical depth helper を共有する。

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
- Universal CLI: Linux/macOS の `flistwalker --cli [query] [--root PATH] [--limit N]`（Windows は direct-process best-effort）
- Dedicated CLI: `fw [query] [--root PATH] [--limit N]`
- `sort_all_matches(matches, mode) -> sorted matches`
- `execute_authorized_action(request, backend) -> ActionReport`

## DES-020 Named-root and preset catalog
- `.flistwalker_search_catalog.json` は legacy `.flistwalker_roots.txt` と分離した version 1 document とし、named roots、pure search presets、unknown fields を保持する。
- mutation は sidecar lock 内で read/validate/mutate/atomic-write する。newer/malformed document は read-only failure とし、同時 writer の lost update と downgrade 時の legacy data loss を防ぐ。
- CLI adapter は明示 catalog option を先に処理し、`--preset` を effective search args へ変換して既存 batch/TUI pipeline へ渡す。
- GUI は `PresetManagerState` に worker から受理した catalog snapshot と ephemeral picker/preset editor/preset削除確認/named-root manager state を保持する。picker open ごとに catalog load request を送り、request-id freshness を満たす response だけで snapshot を置換する。name filter は UI memory 上で行い、適用は既存 root/index/search/sort/tab state transition を再利用する。新規preset draftは現在tabのpure-search stateから構築する。
- GUI の`Presets...` top actionと`Primary+Shift+P`は同じpicker open transitionを呼ぶ。picker/editor/Named Root manager は同じ modal 内で背景 input を遮断し、メイン panel にはlauncher以外のpreset controlを追加しない。presetとNamed Rootのmutationは追加draft、元entry名付き編集draft、または削除対象名をworkerへ渡し、最新catalogをlock内で再読込してin-place mutation/atomic-writeする。named-root renameは参照presetを新名称へ追従させ、deleteは参照を解除してsnapshot fallbackを維持する。collisionやwrite failureではcatalogを上書きせずdraftまたは確認状態を維持し、保存時点のentry unknown fieldsとversion 1 documentのunknown fieldsを保持する。
- GUIのpreset適用はcontroller内の単一transitionでpreset所有stateを先に確定し、root/source/type変更時はindex refresh、それ以外はentry filter再構築と最新search requestのどちらか一方へ進む。sort scopeはtab所有stateとして維持する。picker/editor/Named Root managerの複数領域とworker/catalog境界を同時変更しなければ新機能をテストできない状態になった場合を、controller再分割の判断点とする。
- preset root と Named Root path の browse は既存 root browser の folder picker 境界と、現在のテキスト値から既存 ancestor を求める開始位置解決を再利用する。render は browse command だけを収集し、dispatch 後に dialog を開く。選択結果は表示用に正規化して対象 editor draft だけへ反映し、cancel は no-op、dialog failure は対象 editor の error として保持する。
- `plan_filelist_write(options, filesystem) -> FileListWritePlan`; `settle_filelist_plan(plan, cancel) -> FileListTransactionReport`
- `enqueue_persistence(patch, history_delta)`; `flush_persistence_bounded() -> CommitOutcome`

## DES-021 Field-scoped compiled query
- parserは各termを `QueryField + value` に正規化し、fieldなしと未知prefixを `Any` として既存matcherへ渡す。
- `PreparedCandidate` はbasename、root相対field path、parent directory、最終extensionの正規化viewとvisible文字offsetを検索request内で一度だけ構築する。known Entry kindがdirectoryならextension viewを空にする。
- 絶対表示時はroot相対fieldのbyte/character開始offsetを保持し、matchingにroot prefixを含めずhighlightを表示位置へ写像する。path/dirのliteral値は候補側と同じseparator正規化を行う。
- compiled exact/include/exclude/regex/bonus matcherは同じfield selectorを共有し、highlightだけがfield-local positionをvisible path positionへ変換する。
- field queryはscopeが変わるprefix拡張を避けるためprefix cache対象外とする。非field queryのcache、matching、rankingは既存経路を維持する。

## DES-022 Shared max-depth indexing scope
- Windows depth comparison removes drive/UNC verbatim namespaces lexically and losslessly before root-prefix comparison; it never probes each candidate on the filesystem.
- `MaxDepth` は index domain が所有し、既存 public index/FileList/Walker API は無制限 wrapper を維持しつつ、depth-aware API を batch、GUI、TUI adapter へ提供する。
- recursive walker と adaptive walker は directory queue/再帰 frame に depth を保持し、depth上限のdirectoryをemitした後は子directoryをqueueへ入れない。FileList parser と nested hierarchy は resolved lexical candidate を callbackまたはoverride discoveryへ渡す前に同じ `MaxDepth` で除外する。
- GUI `AppRuntimeState`、`AppTabState`、`SavedTabState`、GUI/TUI `IndexRequest`、`SearchPreset` は同じ値を伝播する。active tab preset transition は max depth を他の preset-owned state と同時に確定し、値が変われば1回だけ reindexする。
- main panel は `Folders` と `Preview` の間に現在値buttonを描画し、popup draft と live値を分離する。`Apply` は live値更新、active-tab同期、session dirty化、request-id付きreindexを順に行い、`Cancel` は no-op とする。
- preset catalog と saved tab は欠落 field を無制限として deserializeし、既存versionを維持する。新規tabは無制限とし、FileList作成workerは表示indexのmax depthを再利用しない。

## DES-024 Dual executable entrypoints and one-shot fast path
- `src/main.rs` と `src/bin/fw.rs` は最小 entrypoint とし、両方が `process_entry::initialize_process_entry` で optional tracing と hidden updater restart を argument parsing より先に処理する。Windows universal だけは compile-time GUI subsystem とし、runtime console detach は行わない。universal は GUI/internal restart と共有 CLI dispatch 実装、`fw` は console subsystem から `cli::run_dedicated` へ進む。
- `cli::run_dedicated` は `BinaryVariant::Cli` を process-global entry context へ設定し、`args::parse_dedicated_args` が先頭へ `--cli` を注入して共有 Clap command を `fw` 名で解析する。実行・TUI・query・action の実装は複製しない。
- batch indexing は `build_index_with_metadata_cancellable_and_max_depth` の `Entry` を直接 filter/search へ渡す。一回検索用 `rank_search_results_uncached` は共有 ranking evaluator を使い、prefix cache の lookup/store と cache population 用の clone/full-sort だけを省く。
- updater asset resolver は `BinaryVariant` を platform target へ明示し、binary stem だけを `FlistWalker-*` / `fw-*` に分ける。remote sidecar stem と署名済み checksum は共有し、対象 variant asset の欠落を別 variant で補完しない。transaction marker は実行ファイル名から推測せず CLI variant に `fw.` sidecar prefix を記録し、version 付き/rename 済み executable と helper process をまたいで universal sidecar と競合しないローカル target を復元する。旧 marker は prefix 欠落を universal として読む。
- build/resource/release scripts は両 executable を生成する。既存 archive と `.app` は universal のみ、`fw` は standalone のみとし、release bundle validator が 28 asset / 26 checksum entry を fail closed で確認する。
## DES-025 GUI settings persistence boundary

- `settings_dialog` は保存済み設定の snapshot、草稿、数値入力、reload 確認、状態（closed/loading/editing/saving/failed）を所有する。画面の操作は worker request を発行し、既存 `config_open` service だけを JSON open に再利用する。
- `runtime_config` は raw JSON bytes を snapshot として読み、保存時に sidecar lock を取得して最新 bytes と比較し、6つの利用者キーだけを patch して atomic replace する。未知キーは残し、実効 process config は変更しない。
- `config_settings` worker は active 1、queued request 1、response 1 に制限し、modal generation で応答を照合する。UI frame は read、lock、write、opener を実行しない。成功は次回起動反映の通知をstatus lineの先頭へ置いてからモーダルを閉じ、フッターが省略されても通知を優先する。失敗は草稿を保持して再試行を許す。

## DES-026 段階的プレビューの所有権と色分け
- `ui_model::paged_preview` は確定済み本文、行範囲、固定復号方式、raw offset、最大4 KiBの先読みbuffer、行途中状態、取得可能なファイルID・size・mtimeを文書として所有する。追加ページは旧文書を複製したworker内の暫定文書へ構築し、復号・binary・identity確認後だけ新しい `Arc` を公開する。失敗ページの本文とcursorは採用しない。
- `PreviewWorkerBus` は全タブ共通で実行中1件と最新待機1件を所有し、より新しいrequest IDをatomic freshnessとしてworkerへ伝える。UI dispatchは待機要求を置き換え、前の待機要求のroutingを解放する。terminal受信後に次の待機要求を送る。UI threadはファイルI/Oを行わない。
- active文書はcommitted tab payloadへ格納し、background応答はrequest-tab routingと選択pathを照合する。既存のinactive/closed heavy snapshot LRUは文書容量もweightへ算入する。選択変更、再読込、tab restoreでは旧requestの本文を採用しない。
- `ui_model::syntax_preview` は対応拡張子を限定した軽量字句分類器とページ継続状態を持つ。workerが増分でspanを作り、65,536 span/1 MiBを超えたらspanを破棄しプレーン表示へ戻す。元の本文を変更しない。GUIは可視行だけ `LayoutJob` を構築し、行番号を選択できない独立ラベルにする。
- 旧文書の退役は既存のbounded tab reclaimerへ渡し、満杯時は旧所有者または単一保留応答に保持して新規採用を遅延する。active/inactive/closed/待機参照の共有 `Arc` は一意に計上し、resident 32 MiBを越える際は非active文書をLRU順で退役する。worker構築8 MiB、応答8 MiB、退役40 MiB、描画一時8 MiBと合わせたaccounted上限は96 MiBとする。背景追加失敗の理由はタブ単位で保持し、本文を残す。
- raw本文は1 MiB、復号後本文は4 MiB、行表は5,000件、単一文書のcapacity合計は8 MiB以内とする。UIの色切替はセッション状態であり設定JSONへ保存しない。追加依存・grammar assetは用いない。
- TUIは共有readerの20行head policyを利用し、従来のmetadata表示とエラー表示を維持する。rollbackはGUIの新reader/highlighter接続を戻し、永続形式は変更しない。
