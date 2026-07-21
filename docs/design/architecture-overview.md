# Architecture Overview

## Architecture overview
- DES-001 Index Source Resolver
- 役割: FileList 優先モード有効時に `FileList.txt`/`filelist.txt`（`filelist.txt` の大小違い含む）を検出して優先読み込み。
- 実装: `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/filelist_hierarchy.rs`, `rust/src/indexer/walker.rs`, `rust/src/indexer/filelist_writer.rs`

- DES-002 Walker Indexer
- 役割: FileList 未使用時の再帰走査。
- 実装: `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/filelist_hierarchy.rs`, `rust/src/indexer/walker.rs`

- DES-003 Fuzzy Search Engine
- 役割: query domain の `CompiledQuery` / `PreparedCandidate` / `QueryEvaluation` がクエリ解釈（`'` `!` `^` `$` `|`）、候補文字列正規化、match/visibility、既存 score/bonus、任意の文字 index highlight span を一元的に担う。非 regex の `^`/`$` は隣接文字制約付きファジーとして評価し、regex モードでも plain include token は regex へ昇格させず、regex 構文を含む token だけを regex matcher として扱う。
- 役割補足: 空白で分割した通常語 token は AND 条件で絞り込みつつ、score では token ごとのリテラル一致を subsequence 一致より優先して順位付けする。
- 役割補足: 同じ unanchored 完全一致 token が複数回指定された場合は query compile 時に必要出現回数として保持し、search と visible highlight 判定の両方で同じ回数条件を適用する。
- 役割補足: GUI/CLI の `Ignore Case` フラグを受け取り、search と highlight で同じ比較モードを使う。
- 役割補足: 検索クエリは要求単位で1回 compile し、候補ごとの path 文字列化・正規化を `PreparedCandidate` で1回に抑える。rank-only 評価は span を割り当てず、大規模候補集合では search worker 内で並列評価しつつ表示用の上位 `limit` 件だけを抽出する。search は shared evaluation 後に別の visible 判定を行わない。
- 役割補足: `CompiledIgnoreTerms` は CLI filter operation と GUI ignore filter scope/pass で1回生成して候補 loop へ再利用する。prefix cache は search options/root と live snapshot identity を含む完全な意味論 key で分離する。
- 実装: `rust/src/query.rs`, `rust/src/search/mod.rs`, `rust/src/search/match_eval.rs`, `rust/src/search/cache.rs`, `rust/src/search/config.rs`, `rust/src/search/execute.rs`, `rust/src/search/rank.rs`, `rust/src/ui_model/highlight.rs`, `rust/src/app/cache.rs`, `rust/src/app/preview_flow.rs`

- DES-004 Action Executor
- 役割: UI の action intent、worker の root confinement、OS 固有の open/execute leaf を分離し、認可済み path だけが OS 境界へ到達する testable seam を保つ。
- 役割補足: UI は同期 filesystem I/O を行わない `Reject` / `Defer` の字句的 precheck と trusted root を含む request 構築だけを担当する。`Defer` は許可を意味せず、worker の権威的な判定へ必ず送る。
- 役割補足: action authorization module は action mode から raw effective target と display path を導出し、解決済み root に対する component containment、解決済み execution path の deduplication、execution/display path の分離を担当する。
- 役割補足: Action worker は全 target の fail-closed な事前認可、各 OS 呼び出し直前の再認可、実行順序、途中失敗時の残件停止と partial-completion 通知を担当する。OS leaf は認可済みの解決済み path を実行し、root policy を判断しない。
- 実装: `rust/src/app/coordinator.rs`, `rust/src/app/input/actions.rs`, `rust/src/app/shell_support.rs`, `rust/src/app/action_authorization.rs`, `rust/src/app/worker_protocol.rs`, `rust/src/app/worker_tasks.rs`, `rust/src/app/worker_support.rs`, `rust/src/actions.rs`

- DES-005 CLI Adapter
- 役割: `clap` 引数を受け取り CLI 出力へ変換。
- 実装: `rust/src/main.rs`

- DES-009 GUI Adapter (egui/eframe)
- 役割: 検索入力、結果表示、プレビュー、複数選択と一括操作を提供。結果ハイライトは search と同じ query 解釈を shared module 経由で使用する。結果スナップショット更新時は current row を行番号ベースで維持し、結果数が減った場合のみ末尾へ丸める。
- 役割補足: Preview worker は File/Directory の header に取得可能な size、`Created` / `Updated`、条件付き属性、シンボリックリンクの `Target` を付加する。symlink では size/日時を `Target ...` としてリンク先、属性をリンク自身へ分ける。日時は依存追加なしの UTC 表示とし、metadata の取得失敗は該当行を省略して本文表示を継続する。
- 役割補足: 起動直後と `Ctrl+G` / `Esc` の検索キャンセル後は、候補が存在する場合に 1 行目を既定選択として復帰させる。
- 役割補足: 検索オプションの `Ignore Case` を既定有効で保持し、無効時は検索結果とハイライトを case-sensitive に切り替える。
- 役割補足: 非空 query 時の結果一覧は、不可視行の `LayoutJob` / highlight 組み立てを行わず、可視行だけに描画コストを寄せてカーソル移動や再描画時の UI 応答性を維持する。
- 役割補足: `app/mod.rs` は横断 orchestration と feature 間の結線だけを保持し、feature ごとの state transition は `app/filelist/mod.rs`、`app/update.rs`、`app/render.rs`、`app/input/mod.rs`、`app/session.rs`、`app/state.rs`、`app/tabs.rs`、`app/pipeline.rs`、`app/pipeline_owner.rs`、`app/cache.rs`、`app/result_reducer.rs`、`app/result_flow.rs`、`app/preview_flow.rs`、`app/response_flow.rs`、`app/root_browser.rs` へ分離する。status line / notice / update-cycle / root/path compare の純粋 helper は `app/coordinator.rs` へ寄せる。
- 役割補足: saved roots の `Manage list` は `RootListManagerState` に保存済み list とは分離した draft roots/default root、通常時の単一選択、編集中の index/path、削除モードと複数選択を保持する。`app/render_dialogs.rs` は通常・編集・削除モードを描画し、検証、正規化、重複排除、default root 追従、Apply/Cancel 境界は `app/root_browser.rs` が担当する。
- 役割補足: `app/mod.rs` の fixed point は `startup/bootstrap`、`frame update cycle`、`shutdown/persist`、`tab routing`、`filelist/update dialog dispatch`、`trace helper` の 6 区分を top-level で束ねることに限定し、各区分の state transition と policy 判定は owner module 側へ寄せる。process shutdown、window trace、egui font setup、root visibility/cache helper などの shell-local helper policy は `app/shell_support.rs` へ移した。
- 役割補足: `FlistWalkerApp` の field inventory は、`app-global shared state`、`active-tab-local state`、`persisted/background tab state`、`feature dialog/update state` の 4 束で追跡し、以後の state decomposition はこの分類を崩さない。
  - 役割補足: `FlistWalkerApp` は `AppShellState` を最上位の ownership boundary とし、その内側に `AppRuntimeState`、`TabSessionState`、`FeatureStateBundle`、`RuntimeUiState`、`CacheStateBundle`、worker/state coordinators をまとめる。shell は runtime bundle を透明に露出せず、coordinator は `shell` バンドル経由で state に明示的にアクセスする。
- 役割補足: `FlistWalkerApp` の app-global / active-result live fields は `AppRuntimeState` に束ね、coordinator 直下の direct field surface を runtime bundle へ寄せる。
- 役割補足: active tab の高容量 live payload（`TabIndexState`、`TabQueryState`、`TabResultState`、entry-kind cache、notice、pending request と進行状態）は `AppRuntimeState` / coordinator / live cache 側を唯一の authoritative owner とし、inactive tab は `AppTabState` が authoritative owner になる。active slot は `id` / accent と root/filter の小さな mirror を保持するが、その payload は scratch であり active-path reader/writer は参照しない。
- 役割補足: tab activation は `TabSessionState` から slot を一時的に remove して owned value とし、outgoing live payload を slot へ store、active index 更新、incoming root/filter の適用と payload の shell への load、scratch slot の reinsert、post-activation の順で行う。`&AppTabState` を保持したまま `&mut FlistWalkerApp` を更新せず、index/pending/kind/incremental/result container と entry-kind cache は move/swap し、activation 後の全件 cache rebuild は行わない。
- 役割補足: reorder は active shell payload を再ロードせず identity/mirror だけを更新する。create/active close/restore/session restore は同じ store/load 境界を使い、inactive close は active shell を変更しない。session persistence は active shell と inactive slot から persisted fields だけを射影し、`AppTabState` 全体を clone しない。
- 役割補足: `startup/bootstrap` では `new` / `from_launch` / `new_with_launch` が eframe entrypoint と app 初期化を束ねる一方、worker wiring と launch seed 構築は `app/bootstrap.rs`、restore/persist 契約は `app/session.rs` が owner を持つ。
- 役割補足: `shell_support.rs` は process shutdown、egui font setup、window trace、root visibility/cache helper などの shell-local helper policy を集約し、`app/mod.rs` から helper surface を外す。
- 役割補足: `frame update cycle` では `app/mod.rs` が egui frame ごとの orchestration と repaint 判断だけを持ち、index/search/poll の進行管理は `app/pipeline.rs`、active result refresh は `app/pipeline_owner.rs`、render command 生成は `app/render.rs` が担当する。
- 役割補足: `shutdown/persist` では `app/mod.rs` が eframe callback から shutdown seam を呼ぶだけに留め、UI state 永続化は `app/session.rs`、worker stop/join は `app/worker_runtime.rs` が担当する。
- 役割補足: `tab routing` では `app/mod.rs` が active tab と request routing の top-level context を持ち、tab switch/reorder/close と restore/refresh は `app/tabs.rs`、background response polling / apply は `app/response_flow.rs`、root selector lifecycle は `app/root_browser.rs` が owner を持つ。background reducer は routing 先 index が current active index の場合に応答を破棄し、active slot の scratch payload を更新しない。
- 役割補足: `filelist/update dialog dispatch` では `app/mod.rs` が dialog command dispatch と notice/status line 連携を束ね、FileList state transition は `app/filelist/mod.rs`、update state transition は `app/update.rs` が担当する。
- 役割補足: `trace helper` では `app/mod.rs` が opt-in window trace の入口だけを保持し、worker trace は `app/workers.rs` の registry shim、`app/worker_tasks.rs` の実装 body、`app/worker_protocol.rs`、session/window 永続化まわりの補助 trace は `app/session.rs` など各 owner helper へ分離する。
- 役割補足: `app/session.rs` は UI state 永続化、saved roots、tab/session restore、window geometry の stabilize と restore を担当し、起動/終了まわりの永続化契約を一箇所へ集約する。
- 役割補足: `app/state.rs` は filelist/update dialog 状態、sort metadata、entry kind、tab drag など GUI 横断で共有される state 型を集約し、`FlistWalkerApp` 本体から型定義のノイズを外す。
- 役割補足: feature 単位の live state は `app/state.rs` の `FeatureStateBundle` (`root_browser`, `filelist`, `update`) に寄せ、`FlistWalkerApp` 直下から dialog/update/root browser field を外す。tab/session registry は `TabSessionState`、feature dialog/update は `FeatureStateBundle` として ownership を分離する。
  - `FileListManager` / `UpdateManager` は内部 bundle を透明に露出せず、`workflow` / `state` を明示参照する前提で扱う。
- 役割補足: background tab snapshot は `app/tab_state.rs` の `TabQueryState`、`TabIndexState`、`TabResultState` へ分割し、tab capture/apply/restore で query/history/index/result の境界を明示する。
- 役割補足: `app/tabs.rs` は tab 初期化、tab snapshot capture/apply、tab switch/move/close、新規 tab 作成に加え、action/sort request routing の owner API と background tab 向け search/index response consume helper、activation 時の restore/refresh 入口を担当する。live 側の tab/session registry（`tabs`, `active_tab`, `next_tab_id`, `pending_restore_refresh`, request routing）は `app/state.rs` の `TabSessionState` に束ねる。`TabSessionState` は Vec そのものを公開せず、明示的な collection API と active tab / tab id / routing / restore helpers を通じてのみ更新する。
- 役割補足: root change、tab lifecycle、tab activation/background restore、tab close cleanup、tab reorder の state transition は専用 helper / command 境界へ寄せてあり、`app/mod.rs` には feature owner を呼び分ける coordinator だけを残す。
- 役割補足: `app/render.rs` は `run_ui_frame()`、`RenderCommand`、`dispatch_render_commands()`、小さな facade wrapper を保持する。panel/dialog/results/tab の実描画は `app/render_panels.rs`、`app/render_dialogs.rs`、`app/render_tabs.rs`、`app/render_snapshot.rs`、`app/render_theme.rs` が担当し、top action / FileList dialog / update dialog / tab bar interaction からの state transition は描画後 dispatcher で一段遅らせて実行する。
- 役割補足: `app/mod.rs` の frame/update/exit orchestration は `poll_runtime_events()`, `run_update_cycle()`, `schedule_frame_repaint()`, `request_viewport_close_if_needed()`, `persist_state_and_shutdown()` といった helper seam を経由し、`update()` / `on_exit()` / `Drop` の open-coded sequence を最小化する。
- 役割補足: app 起動時の worker wiring と launch 由来の seed 構築は `app/bootstrap.rs` へ寄せ、`new_with_launch` は coordinator として初期化結果を束ねる。
- 役割補足: worker request/response channel は `app/worker_bus.rs` へ集約し、`FlistWalkerApp` 直下には worker bus 全体を 1 フィールドで保持する。
- 役割補足: action、kind、index の dispatch owner は bounded sender の `try_send` 結果を state transition へ変換する。action は受理後だけ request routing と in-progress state を commit し、kind/index は `Full` の要求を owner の bounded retry state へ戻す。`Disconnected`、stale、cancel は owner が terminal response と同じ cleanup reducer へ流し、pending/routing を残さない。
- 役割補足: index coordinator は dispatch 済み要求を最大 2 件、app pending scheduler は全体最大 4 件かつ tab ごとに最新 1 件だけ所有する。kind owner は `Full` で取り出した path を queue 先頭へ戻し、worker からの `kind=None` terminal response で pending/in-flight set を解放する。
- 役割補足: runtime UI の一時状態は `app/ui_state.rs` の `RuntimeUiState` へ、query/history 系は `app/query_state.rs` の `QueryState` へ束ね、coordinator は state holder を介して feature 間を調停する。
- 役割補足: `app/pipeline.rs` は index queue、index response poll、dispatcher を担当し、active tab 向け search/result refresh と entry filter 再適用は `app/pipeline_owner.rs` の dedicated owner surface へ、background tab 向け search/index response apply は `app/tabs.rs` の background-flow helper へ委譲する。index request の採番・tracking・active/background refresh 開始・terminal cleanup の owner API は `app/index_coordinator.rs` 側へ寄せ、search worker の request/tab routing helper は `app/search_coordinator.rs` 側へ寄せる。
- 役割補足: `app/search_coordinator.rs` は search request_id 採番、active/background tab routing、stale response route 判定を担当し、active search の apply / rerun は `app/pipeline_owner.rs`、background search 応答の保持は `app/tabs.rs` が担当する。
- 役割補足: `app/filelist/mod.rs` は overwrite/ancestor/use-walker/deferred-after-index の pending state、cancel flag 伝播、`Finished` / `Failed` / `Canceled` 応答後の cleanup と follow-up dispatch を owner として扱う。`app/state.rs` の `FileListManager` は request_id と requested root を相関して `CurrentRoot` / `PreviousRoot` / `StaleRequestedRoot` を分類し、`poll_filelist_response()` が stale requested root を無視したまま reducer 的に post-settle command を dispatch できるようにする。
- 役割補足: `app/update.rs` は startup check / install request の dispatch を担当し、request_id-correlated response apply、stale 応答吸収、prompt/failure/install_started 遷移の cleanup は `app/state.rs` の `UpdateManager` command surface と合わせて owner 境界を保つ。
- 役割補足: `app/index_worker.rs` は FileList / Walker streaming、kind classification、index worker 実装を担当し、`app/workers.rs` の registry shim から indexing concern を切り離す。worker body は `app/worker_tasks.rs` に集約する。
- 役割補足: `app/worker_protocol.rs` は search/index/preview/action/sort/kind/filelist/update の request/response 型を集約し、worker protocol surface を worker 実装や bus wiring から独立して保守できるようにする。
- 役割補足: `app/worker_runtime.rs` は worker shutdown signal と join timeout の管理だけを持ち、個別 worker 実装から runtime orchestration を分離する。
- 役割補足: `app/worker_runtime.rs` は action 2、kind 1、index 2 の固定 worker handle を名前付きで直接所有し、stop accepting -> queued request settle -> join -> response close を全体 250ms の終了予算で調停する。
- 役割補足: `app/cache.rs` は preview/highlight/sort metadata/entry-kind cache state と invalidation に専念し、bounded storage と scope/eviction を局所化する。entry-kind cache は tab-local payload として activation 時に move/swap し、それ以外の cache は app-global とする。preview/highlight/routing の orchestration は `app/preview_flow.rs` へ、sort の orchestration は `app/result_flow.rs` へ、実際の response apply と snapshot refresh は `app/result_reducer.rs` へ分離する。
- 役割補足: `app/response_flow.rs` は preview/action/sort を中心に worker response の polling と routing を集約し、background tab 応答も owner ごとに dispatch できるようにする。
- 役割補足: `app/root_browser.rs` は root selector dialog の state と root change cleanup を担当し、root 変更時の一時 UI を tab/session state から分離する。
- 役割補足: `app/worker_support.rs` は worker routing の共通 helper と action target helper を担当し、`workers.rs` の registry shim から reusable helper を切り離す。
- 役割補足: search domain は `search/mod.rs` を public API と high-level orchestration の入口に保ちつつ、query compile / literal/regex match / searchable entry materialization / candidate score evaluation は `search/match_eval.rs`、prefix cache は `search/cache.rs`、execution mode と parallel tuning は `search/config.rs`、candidate collect は `search/execute.rs`、ranking/materialization は `search/rank.rs` へ分割して保守する。
- 役割補足: indexer domain は `indexer/mod.rs` を public API、型、FileList-vs-walker build orchestration の入口に保ちつつ、nested FileList override は `indexer/filelist_hierarchy.rs`、FileList read は `indexer/filelist_reader.rs`、walker は `indexer/walker.rs`、FileList write/ancestor propagation は `indexer/filelist_writer.rs` へ分割して保守する。
- 役割補足: active indexing の terminal response は `PendingActiveIndexFinish` として保留でき、未反映 `pending_entries` を frame budget 内で吸収し終えてから finalization する。既定の files+folders 両有効では、終端時に全件 filter / kind cache rebuild を再実行せず、incremental ingestion 済みの state を source of truth へ昇格する。
- 役割補足: runtime settings は Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` へ集約し、`FLISTWALKER_*` は初回 seed としてのみ使う。初回 seed には一般利用者向けの `walker_max_entries`、`history_persist_disabled`、`restore_tabs_enabled`、`emacs_keybindings_enabled`、`tab_pin_moves_to_next_row` を既定値で含め、既存 config に欠けている場合も読み込み時に同じ項目を補完する。build/release と dev/test override は従来どおり env のまま保持し、公開 docs には config file の場所と seed-only 挙動を明記する。
- 役割補足: candidate は `entry.rs` の `Entry { path, kind }` で app/index/search worker 境界をまたいで表現し、app 側の kind side-channel を持たない。
- 実装: `rust/src/app/mod.rs`, `rust/src/app/coordinator.rs`, `rust/src/app/filelist/mod.rs`, `rust/src/app/update.rs`, `rust/src/app/render.rs`, `rust/src/app/input/mod.rs`, `rust/src/app/session.rs`, `rust/src/app/state.rs`, `rust/src/app/tab_state.rs`, `rust/src/app/tabs.rs`, `rust/src/app/pipeline.rs`, `rust/src/app/pipeline_owner.rs`, `rust/src/app/bootstrap.rs`, `rust/src/app/cache.rs`, `rust/src/app/result_reducer.rs`, `rust/src/app/result_flow.rs`, `rust/src/app/preview_flow.rs`, `rust/src/app/worker_bus.rs`, `rust/src/app/worker_protocol.rs`, `rust/src/app/worker_runtime.rs`, `rust/src/app/worker_support.rs`, `rust/src/app/shell_support.rs`, `rust/src/app/ui_state.rs`, `rust/src/app/query_state.rs`, `rust/src/app/search_coordinator.rs`, `rust/src/app/index_coordinator.rs`, `rust/src/app/index_worker.rs`, `rust/src/app/workers.rs`, `rust/src/app/worker_tasks.rs`, `rust/src/entry.rs`, `rust/src/ui_model/mod.rs`, `rust/src/query.rs`, `rust/src/search/mod.rs`, `rust/src/search/cache.rs`, `rust/src/search/config.rs`, `rust/src/search/execute.rs`, `rust/src/search/rank.rs`, `rust/src/ignore_list.rs`, `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/walker.rs`, `rust/src/indexer/filelist_writer.rs`
- 実装: `rust/src/app/mod.rs`, `rust/src/app/coordinator.rs`, `rust/src/app/filelist/mod.rs`, `rust/src/app/update.rs`, `rust/src/app/render.rs`, `rust/src/app/input/mod.rs`, `rust/src/app/session.rs`, `rust/src/app/state.rs`, `rust/src/app/tab_state.rs`, `rust/src/app/tabs.rs`, `rust/src/app/pipeline.rs`, `rust/src/app/pipeline_owner.rs`, `rust/src/app/bootstrap.rs`, `rust/src/app/cache.rs`, `rust/src/app/result_reducer.rs`, `rust/src/app/result_flow.rs`, `rust/src/app/preview_flow.rs`, `rust/src/app/worker_bus.rs`, `rust/src/app/worker_protocol.rs`, `rust/src/app/worker_runtime.rs`, `rust/src/app/worker_support.rs`, `rust/src/app/shell_support.rs`, `rust/src/app/ui_state.rs`, `rust/src/app/query_state.rs`, `rust/src/app/search_coordinator.rs`, `rust/src/app/index_coordinator.rs`, `rust/src/app/index_worker.rs`, `rust/src/app/workers.rs`, `rust/src/app/worker_tasks.rs`, `rust/src/entry.rs`, `rust/src/ui_model/mod.rs`, `rust/src/query.rs`, `rust/src/runtime_config.rs`, `rust/src/search/mod.rs`, `rust/src/search/cache.rs`, `rust/src/search/config.rs`, `rust/src/search/execute.rs`, `rust/src/search/rank.rs`, `rust/src/ignore_list.rs`, `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/walker.rs`, `rust/src/indexer/filelist_writer.rs`

- DES-010 GUI Test Artifacts
- 役割: GUI 回帰手順と結果を管理する。
- 実装: `docs/TESTPLAN.md` の GUI 手順節

- DES-012 CI / Release Hygiene
- 役割: release 対象 OS の CI 継続検証、依存脆弱性検査、notarization 運用の文書化。
- 実装: `.github/workflows/ci-cross-platform.yml`, `docs/RELEASE.md`, `.github/release-template.md`, `rust/build.rs`

- DES-013 Result Sort Controller
- 役割: 結果スナップショット限定のソート、日付属性の遅延取得、上限付き属性キャッシュを管理。
- 実装: `rust/src/app/mod.rs`, `rust/src/app/render.rs`, `rust/src/app/cache.rs`, `rust/src/app/workers.rs`, `rust/src/app/worker_tasks.rs`, `rust/src/app/state.rs`, `rust/src/app/pipeline.rs`

- DES-014 Self Update Coordinator
- 役割: GitHub Releases の最新 version 確認、対象 asset と sidecar 文書 (`*.README.txt`, `*.LICENSE.txt`, `*.THIRD_PARTY_NOTICES.txt`) の選択、`SHA256SUMS.sig` と `SHA256SUMS` の検証、Windows/Linux 向け staged update と再起動を制御する。
- 実装: `rust/src/updater.rs`, `rust/src/updater/release.rs`, `rust/src/updater/staging.rs`, `rust/src/updater/manifest.rs`, `rust/src/updater/apply.rs`, `rust/src/app/update.rs`, `rust/src/app/render.rs`, `rust/src/app/workers.rs`, `rust/src/app/worker_tasks.rs`, `rust/src/app/state.rs`
- 役割補足: `check_for_update()` は release fetch と candidate 解決を分け、candidate 解決側では release asset 選択と support classification を helper 化して contract を小さく保つ。
- 役割補足: self-update apply helper は署名検証と全 staged asset の checksum 検証を通過した private verified bundle だけを受け取り、未検証 staged path を platform apply へ渡さない。
- 役割補足: staged asset と helper script は no-overwrite file creation primitive を経由し、既存 path の暗黙上書きを避ける。
- 役割補足: update request / apply / failure の主要遷移は `AppendWindowTrace` と worker-side `tracing` の両方で残し、worker trace は `flow=update` / `event=*` / `request_id=*` を canonical field として support 時の request_id correlation を取りやすくする。
- 役割補足: request/response trace の details には request_id を必ず含め、update_check_failed や update_failed は error 内容も併記して support 時の切り分けをしやすくする。
- 役割補足: `UpdateState` と update worker request/response の lifecycle、stale 応答吸収、prompt/failure/install_started の遷移は `app/update.rs` の manager 境界へ集約する。
- 役割補足: `render_dialogs.rs` は update dialog の描画と入力取得を担当し、`render.rs` は `RenderUpdateDialogCommand` の queue/dispatch 境界を保持する。永続化 (`session.rs`) と app close orchestration (`app/mod.rs`) は `FlistWalkerApp` 側に残したまま `UpdateAppCommand` で橋渡しする。
- 役割補足: top action button、FileList dialog、update dialog、tab bar reorder/close/switch は `RenderCommand` 境界を経由して owner helper (`filelist.rs`, `update.rs`, `tabs.rs`) へ渡す。描画コードは click/drag/dialog input の収集に寄せ、state transition は描画後 dispatcher で一段遅らせて実行する。

- DES-015 Diagnostics and Supportability Contract
- 役割: worker-side `tracing` と opt-in window trace の責務を分け、support/debug 用の canonical event family を維持する。
- 実装: `rust/src/app/workers.rs`, `rust/src/app/worker_tasks.rs`, `rust/src/app/index_worker.rs`, `rust/src/app/mod.rs`, `rust/src/app/session.rs`, `rust/src/app/input/mod.rs`, `rust/src/main.rs`
- 役割補足: worker-side async flow は `flow` / `event` / `request_id` を中心に記録し、request-scoped でない flow は `epoch` や `source_kind` など最小の補助 field だけを追加する。
- 役割補足: search / preview / filelist / action / sort metadata / update は started/finished/failed/receiver_closed 系の event family に寄せ、index は `flow=index` と `source_kind` で filelist/walker/none を切り分ける。
- 役割補足: GUI/session/input/update の opt-in trace は `FLISTWALKER_WINDOW_TRACE=1` のみで有効化し、window geometry、IME composition、query text change、startup/update dialog などの GUI diagnostics を `append_window_trace` へ集約する。
- 役割補足: diagnostics 強化で request routing や response acceptance を変えない。hot UI path へ重い同期 I/O や新しい汎用 logging framework を導入しない。

- DES-016 Ignore List Filter
- 役割: 実行中 binary と同じフォルダにある ignore list を読み取り、検索候補と空クエリ表示から除外する。
- 実装: `rust/src/ignore_list.rs`, `rust/src/query.rs`, `rust/src/app/session.rs`, `rust/src/app/ui_state.rs`, `rust/src/app/bootstrap.rs`, `rust/src/app/shell_support.rs`, `rust/src/app/render.rs`, `rust/src/app/render_panels.rs`, `rust/src/main.rs`
- 役割補足: ignore list の各ルールは query の `!` 除外と同じ非 fuzzy の literal / anchor 比較で評価し、既定では GUI の `Use Ignore List` チェックボックスが有効な状態で候補集合へ反映する。
- 役割補足: ignore list ファイルの読込失敗や未存在は空ルールとして扱い、検索/GUI/CLI の通常操作を止めない。

- DES-017 Runtime Config Bootstrap
- 役割: Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` を runtime settings の保存先として扱い、起動初回のみ current env を seed に自動生成する。
- 実装: `rust/src/runtime_config.rs`, `rust/src/main.rs`, `rust/src/app/session.rs`, `rust/src/app/shell_support.rs`, `rust/src/search/config.rs`, `rust/src/app/index_worker.rs`, `rust/src/updater.rs`
- 役割補足: runtime config file が存在する場合は読み込み結果を process env に反映して既存の env 駆動経路へ伝播し、存在しない場合だけ current env を取り込んでファイルを生成する。UI state、saved roots、window trace も同じ base directory 解決規則へ揃える。
- 役割補足: Windows の旧 exe-side / home-directory 配置ファイルと Linux/macOS の旧 home-root 配置ファイルは、新しい保存先が未作成のときだけ初回起動で移行し、既存の新配置ファイルを上書きしない。transition migration は v0.20.0 までの一時対応として扱う。
- 役割補足: GUI の設定ボタンは render command 経由で `shell_support` の config open 処理を呼び、`runtime_config_file_path` を生成済みにしたうえで `actions` の既定アプリ open を試す。既定アプリが失敗した場合は、Windows では `notepad.exe`、macOS では `open -t`、Linux では `VISUAL` / `EDITOR` または一般的な GUI editor へフォールバックする。
- 役割補足: build-time 公開鍵や release signing secret は runtime config file に含めず、既存の build / release / dev-test secret 経路に残す。

- DES-018 Release Sample Ignore List
- 役割: release asset と self-update helper が ignore list サンプルを同梱・配置し、初回利用時の導線を提供する。
- 実装: `scripts/prepare-release.sh`, `scripts/prepare-release-linux.sh`, `scripts/prepare-release.ps1`, `scripts/prepare-release-macos.sh`, `rust/src/updater.rs`, `docs/RELEASE.md`
- 役割補足: ignore list sample は binary に埋め込まれ、起動時に `flistwalker.ignore.txt.example` が無ければ local 実体として生成される。
- 役割補足: 既存 `flistwalker.ignore.txt` を上書きせず、sample 生成失敗は本体起動や自己更新を妨げない。

- DES-019 PowerShell Windows GNU Build Coordinator
- 役割: Windows PowerShell での依存検出、導入承認、process PATH 再構成、GNU tool 解決、release/clean build、artifact 同期を一貫した契約で提供する。
- 実装: `scripts/common-win-gnu.ps1`, `scripts/build-rust-win.ps1`, `scripts/build-rust-win-clean.ps1`, `scripts/dev-check-windows.ps1`, `scripts/test-build-rust-win.ps1`, `scripts/test-windows-build-artifact.ps1`, `rust/build.rs`, `rust/windows_resource_build.rs`
- 役割補足: shared helper は検出、承認、install、PATH 再構成、tool/env export、checked command 実行だけを担当し、build/clean/test の操作順は各 entrypoint が所有する。
- 役割補足: install mode は `CheckOnly` / `NoInstall` / `InstallMissing` / interactive を明示し、非対話 interactive は `NoInstall` へ縮退する。machine state の変更は winget/pacman/rustup の利用者承認後だけ行い、script 自身は永続 PATH を変更しない。
- 役割補足: Windows-host GNU `build.rs` は resolved `windres` / `ar` で resource を生成し、GNU target の GUI binary へ `resource.o` を明示リンクする。
- 役割補足: build entrypoint は source/destination の正規化比較で自己 copy を避け、strip 済み実体を 2 名の artifact へ同期する。
