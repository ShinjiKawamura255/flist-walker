# Flows, Data Model, and API Contract

## Main flows
- Flow-001: 起動 -> （FileList 優先モード有効時）FileList 検出 -> 読み込み -> 検索 -> 選択 -> アクション。
- Flow-002: 起動 -> FileList なし -> walker 走査 -> 検索 -> 選択 -> アクション。
- Flow-003: アクション失敗 -> エラー整形 -> 表示 -> 非ゼロ終了（CLI）/エラー通知（GUI）。
- Flow-004: GUI 起動 -> 非同期インデックス -> 最新要求優先検索（古い要求を破棄） -> プレビュー -> 実行/オープン。
- Flow-005: GUI 起動 -> update worker が上限付きで GitHub Releases を確認 -> 新版あり -> 利用者承認 -> `SHA256SUMS` / `SHA256SUMS.sig` を先行取得 -> strict parse と署名検証 -> binary/sidecar を private create-new file へ上限付き streaming download/hash 検証 -> `VerifiedUpdateBundle` -> executable parent 内へ同一 directory 準備 -> durable parent/helper registration と acknowledgement -> 本体終了 -> sidecar 適用 -> binary-last atomic commit -> 再起動。precommit/restart failure は旧 bundle へ rollback し、中断は起動時 marker/hash recovery へ収束する。ignore list sample は別途起動時初期化で補完する。
  `FLISTWALKER_DISABLE_SELF_UPDATE=1`、または実行中バイナリと同一ディレクトリに `FLISTWALKER_DISABLE_SELF_UPDATE` ファイルがある場合は update flow を起動せず、通常起動のみ行う。
- Flow-006: batch CLI -> root selection -> index（`--progress` では開始と候補件数/時間を stderr）-> search/full-match sort -> limit（全一致件数・返却件数/時間を stderr）-> `BufWriter` による逐次 print、preauthorized action、または external command executor。external executor は全 target を canonical root 配下として事前認可し、OS の argv budget と optional path cap で stable greedy batch を作り、各 batch 直前の再認可後に direct argv で直列起動する。target 0 件は no-spawn、failure/cancel は残 batch を停止する。
- Flow-007: FileList create -> deterministic write plan/content precompute -> per-target cancellation check -> commit -> success report、または rollback attempt -> terminal settlement report。TUI は settlement 後にのみ pending output/root/exit intent を解決する。
- Flow-008: GUI/TUI frame -> `UiStatePatch + history_delta` enqueue -> persistence worker bounded lock -> latest JSON read/merge -> atomic write -> committed generations only clear。失敗は同一 coalesced payload を retry queue に残す。
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
- `SortMode`: score/name/modified/created/size の方向付き enum。shared sort は full match set と mode を受け、adapter がその後に limit を適用する。
- `AuthorizedActionRequest`: trusted root、current-row snapshot、request identity、cancellation token、action kind。
- `FileListWritePlan`: ordered root/ancestor targets、expected prior contents、new contents、consent scope。`FileListTransactionReport` は committed/failed/rolled-back/rollback-failed display paths と settlement reason を保持する。
- `UiStatePatch`: named non-history JSON leaf patch と ordered history deltas。worker generation は commit に含めた enqueue range を記録する。

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
- `sort_all_matches(matches, mode) -> sorted matches`
- `execute_authorized_action(request, backend) -> ActionReport`

## DES-020 Named-root and preset catalog
- `.flistwalker_search_catalog.json` は legacy `.flistwalker_roots.txt` と分離した version 1 document とし、named roots、pure search presets、unknown fields を保持する。
- mutation は sidecar lock 内で read/validate/mutate/atomic-write する。newer/malformed document は read-only failure とし、同時 writer の lost update と downgrade 時の legacy data loss を防ぐ。
- CLI adapter は明示 catalog option を先に処理し、`--preset` を effective search args へ変換して既存 batch/TUI pipeline へ渡す。
- GUI は `PresetManagerState` に worker から受理した catalog snapshot と ephemeral picker/preset editor/preset削除確認/named-root manager state を保持する。picker open ごとに catalog load request を送り、request-id freshness を満たす response だけで snapshot を置換する。name filter は UI memory 上で行い、適用は既存 root/index/search/sort/tab state transition を再利用する。新規preset draftは現在tabのpure-search stateから構築する。
- GUI picker/editor/Named Root manager は同じ modal 内で背景 input を遮断し、メイン panel へ control を追加しない。presetとNamed Rootのmutationは追加draft、元entry名付き編集draft、または削除対象名をworkerへ渡し、最新catalogをlock内で再読込してin-place mutation/atomic-writeする。named-root renameは参照presetを新名称へ追従させ、deleteは参照を解除してsnapshot fallbackを維持する。collisionやwrite failureではcatalogを上書きせずdraftまたは確認状態を維持し、保存時点のentry unknown fieldsとversion 1 documentのunknown fieldsを保持する。
- GUIのpreset適用はcontroller内の単一transitionでpreset所有stateを先に確定し、root/source/type変更時はindex refresh、それ以外はentry filter再構築と最新search requestのどちらか一方へ進む。sort scopeはtab所有stateとして維持する。picker/editor/Named Root managerの複数領域とworker/catalog境界を同時変更しなければ新機能をテストできない状態になった場合を、controller再分割の判断点とする。
- preset root と Named Root path の browse は既存 root browser の folder picker 境界と、現在のテキスト値から既存 ancestor を求める開始位置解決を再利用する。render は browse command だけを収集し、dispatch 後に dialog を開く。選択結果は表示用に正規化して対象 editor draft だけへ反映し、cancel は no-op、dialog failure は対象 editor の error として保持する。
- `plan_filelist_write(options, filesystem) -> FileListWritePlan`; `settle_filelist_plan(plan, cancel) -> FileListTransactionReport`
- `enqueue_persistence(patch, history_delta)`; `flush_persistence_bounded() -> CommitOutcome`

## DES-021 Field-scoped compiled query
- parserは各termを `QueryField + value` に正規化し、fieldなしと未知prefixを `Any` として既存matcherへ渡す。
- `PreparedCandidate` はbasename、visible path、parent directory、最終extensionの正規化viewとvisible文字offsetを検索request内で一度だけ構築する。known Entry kindがdirectoryならextension viewを空にする。
- compiled exact/include/exclude/regex/bonus matcherは同じfield selectorを共有し、highlightだけがfield-local positionをvisible path positionへ変換する。
- field queryはscopeが変わるprefix拡張を避けるためprefix cache対象外とする。非field queryのcache、matching、rankingは既存経路を維持する。
