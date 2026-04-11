# DESIGN

## Architecture overview
- DES-001 Index Source Resolver
- 役割: FileList 優先モード有効時に `FileList.txt`/`filelist.txt`（`filelist.txt` の大小違い含む）を検出して優先読み込み。
- 実装: `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/walker.rs`, `rust/src/indexer/filelist_writer.rs`

- DES-002 Walker Indexer
- 役割: FileList 未使用時の再帰走査。
- 実装: `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/walker.rs`

- DES-003 Fuzzy Search Engine
- 役割: クエリ解釈（`'` `!` `^` `$` `|`）とスコアリングを担う。query 分解と正規化は shared module へ集約し、非 regex の `^`/`$` は隣接文字制約付きファジーとして評価する。regex モードでも plain include token は regex へ昇格させず、regex 構文を含む token だけを regex matcher として扱う。
- 役割補足: 空白で分割した通常語 token は AND 条件で絞り込みつつ、score では token ごとのリテラル一致を subsequence 一致より優先して順位付けする。
- 役割補足: GUI/CLI の `Ignore Case` フラグを受け取り、search と highlight で同じ比較モードを使う。
- 役割補足: 検索クエリは要求単位で前処理し、候補ごとの path 文字列化・正規化を 1 回に抑える。大規模候補集合では search worker 内で並列評価しつつ、表示用には上位 `limit` 件だけを抽出する。
- 実装: `rust/src/query.rs`, `rust/src/search/mod.rs`, `rust/src/search/cache.rs`, `rust/src/search/config.rs`, `rust/src/search/execute.rs`, `rust/src/search/rank.rs`

- DES-004 Action Executor
- 役割: ファイル実行/オープン、フォルダオープンを OS 差分吸収して実行し、open/execute の testable seam を保つ。
- 実装: `rust/src/actions.rs`

- DES-005 CLI Adapter
- 役割: `clap` 引数を受け取り CLI 出力へ変換。
- 実装: `rust/src/main.rs`

- DES-009 GUI Adapter (egui/eframe)
- 役割: 検索入力、結果表示、プレビュー、複数選択と一括操作を提供。結果ハイライトは search と同じ query 解釈を shared module 経由で使用する。結果スナップショット更新時は current row を行番号ベースで維持し、結果数が減った場合のみ末尾へ丸める。
- 役割補足: 起動直後と `Ctrl+G` / `Esc` の検索キャンセル後は、候補が存在する場合に 1 行目を既定選択として復帰させる。
- 役割補足: 検索オプションの `Ignore Case` を既定有効で保持し、無効時は検索結果とハイライトを case-sensitive に切り替える。
- 役割補足: 非空 query 時の結果一覧は、不可視行の `LayoutJob` / highlight 組み立てを行わず、可視行だけに描画コストを寄せてカーソル移動や再描画時の UI 応答性を維持する。
- 役割補足: `app/mod.rs` は横断 orchestration と feature 間の結線だけを保持し、feature ごとの state transition は `app/filelist.rs`、`app/update.rs`、`app/render.rs`、`app/input.rs`、`app/session.rs`、`app/state.rs`、`app/tabs.rs`、`app/pipeline.rs`、`app/pipeline_owner.rs`、`app/cache.rs`、`app/result_reducer.rs`、`app/result_flow.rs`、`app/preview_flow.rs` へ分離する。status line / notice / update-cycle / root/path compare の純粋 helper は `app/coordinator.rs` へ寄せる。
- 役割補足: `app/mod.rs` の fixed point は `startup/bootstrap`、`frame update cycle`、`shutdown/persist`、`tab routing`、`filelist/update dialog dispatch`、`trace helper` の 6 区分を top-level で束ねることに限定し、各区分の state transition と policy 判定は owner module 側へ寄せる。process shutdown、window trace、egui font setup、root visibility/cache helper などの shell-local helper policy は `app/shell_support.rs` へ移した。
- 役割補足: `FlistWalkerApp` の field inventory は、`app-global shared state`、`active-tab-local state`、`persisted/background tab state`、`feature dialog/update state` の 4 束で追跡し、以後の state decomposition はこの分類を崩さない。
- 役割補足: `FlistWalkerApp` は `AppShellState` を最上位の ownership boundary とし、その内側に `AppRuntimeState`、`TabSessionState`、`FeatureStateBundle`、`RuntimeUiState`、`CacheStateBundle`、worker/state coordinators をまとめる。shell は `Deref` で runtime bundle を見せ、既存の coordinator 呼び出しを壊さずに outer shell を一枚かぶせる。
- 役割補足: `FlistWalkerApp` の app-global / active-result live fields は `AppRuntimeState` に束ね、coordinator 直下の direct field surface を runtime bundle へ寄せる。
- 役割補足: `AppTabState` は persisted/background tab の snapshot 契約として維持し、active tab 側の live state は別 bundle から owner module へ渡す方針を取る。
- 役割補足: `startup/bootstrap` では `new` / `from_launch` / `new_with_launch` が eframe entrypoint と app 初期化を束ねる一方、worker wiring と launch seed 構築は `app/bootstrap.rs`、restore/persist 契約は `app/session.rs` が owner を持つ。
- 役割補足: `shell_support.rs` は process shutdown、egui font setup、window trace、root visibility/cache helper などの shell-local helper policy を集約し、`app/mod.rs` から helper surface を外す。
- 役割補足: `frame update cycle` では `app/mod.rs` が egui frame ごとの orchestration と repaint 判断だけを持ち、index/search/poll の進行管理は `app/pipeline.rs`、active result refresh は `app/pipeline_owner.rs`、render command 生成は `app/render.rs` が担当する。
- 役割補足: `shutdown/persist` では `app/mod.rs` が eframe callback から shutdown seam を呼ぶだけに留め、UI state 永続化は `app/session.rs`、worker stop/join は `app/worker_runtime.rs` が担当する。
- 役割補足: `tab routing` では `app/mod.rs` が active tab と request routing の top-level context を持ち、tab switch/reorder/close、background response apply、restore/refresh は `app/tabs.rs` が owner を持つ。
- 役割補足: `filelist/update dialog dispatch` では `app/mod.rs` が dialog command dispatch と notice/status line 連携を束ね、FileList state transition は `app/filelist.rs`、update state transition は `app/update.rs` が担当する。
- 役割補足: `trace helper` では `app/mod.rs` が opt-in window trace の入口だけを保持し、worker trace は `app/workers.rs` と `app/worker_protocol.rs`、session/window 永続化まわりの補助 trace は `app/session.rs` など各 owner helper へ分離する。
- 役割補足: `app/session.rs` は UI state 永続化、saved roots、tab/session restore、window geometry の stabilize と restore を担当し、起動/終了まわりの永続化契約を一箇所へ集約する。
- 役割補足: `app/state.rs` は filelist/update dialog 状態、sort metadata、entry kind、tab drag など GUI 横断で共有される state 型を集約し、`FlistWalkerApp` 本体から型定義のノイズを外す。
- 役割補足: feature 単位の live state は `app/state.rs` の `FeatureStateBundle` (`root_browser`, `filelist`, `update`) に寄せ、`FlistWalkerApp` 直下から dialog/update/root browser field を外す。tab/session registry は `TabSessionState`、feature dialog/update は `FeatureStateBundle` として ownership を分離する。
- 役割補足: background tab snapshot は `app/tab_state.rs` の `TabQueryState`、`TabIndexState`、`TabResultState` へ分割し、tab capture/apply/restore で query/history/index/result の境界を明示する。
- 役割補足: `app/tabs.rs` は tab 初期化、tab snapshot capture/apply、tab switch/move/close、新規 tab 作成に加え、action/sort request routing の owner API と background tab 向け search/index response consume helper、activation 時の restore/refresh 入口を担当する。live 側の tab/session registry（`tabs`, `active_tab`, `next_tab_id`, `pending_restore_refresh`, request routing）は `app/state.rs` の `TabSessionState` に束ねる。
- 役割補足: root change、tab lifecycle、tab activation/background restore、tab close cleanup、tab reorder の state transition は専用 helper / command 境界へ寄せてあり、`app/mod.rs` には feature owner を呼び分ける coordinator だけを残す。
- 役割補足: `app/render.rs` は top action / FileList dialog / update dialog / tab bar interaction から `RenderCommand` を queue し、描画後に `dispatch_render_commands()` で state transition を消化する。root selector と query/history input は描画側の direct mutation を維持する。
- 役割補足: `app/mod.rs` の frame/update/exit orchestration は `poll_runtime_events()`, `run_update_cycle()`, `schedule_frame_repaint()`, `request_viewport_close_if_needed()`, `persist_state_and_shutdown()` といった helper seam を経由し、`update()` / `on_exit()` / `Drop` の open-coded sequence を最小化する。
- 役割補足: app 起動時の worker wiring と launch 由来の seed 構築は `app/bootstrap.rs` へ寄せ、`new_with_launch` は coordinator として初期化結果を束ねる。
- 役割補足: worker request/response channel は `app/worker_bus.rs` へ集約し、`FlistWalkerApp` 直下には worker bus 全体を 1 フィールドで保持する。
- 役割補足: runtime UI の一時状態は `app/ui_state.rs` の `RuntimeUiState` へ、query/history 系は `app/query_state.rs` の `QueryState` へ束ね、coordinator は state holder を介して feature 間を調停する。
- 役割補足: `app/pipeline.rs` は index queue、index response poll、dispatcher を担当し、active tab 向け search/result refresh と entry filter 再適用は `app/pipeline_owner.rs` の dedicated owner surface へ、background tab 向け search/index response apply は `app/tabs.rs` の background-flow helper へ委譲する。index request の採番・tracking・active/background refresh 開始・terminal cleanup の owner API は `app/index_coordinator.rs` 側へ寄せ、search worker の request/tab routing helper は `app/search_coordinator.rs` 側へ寄せる。
- 役割補足: `app/search_coordinator.rs` は search request_id 採番、active/background tab routing、stale response route 判定を担当し、active search の apply / rerun は `app/pipeline_owner.rs`、background search 応答の保持は `app/tabs.rs` が担当する。
- 役割補足: `app/filelist.rs` は overwrite/ancestor/use-walker/deferred-after-index の pending state、cancel flag 伝播、`Finished` / `Failed` / `Canceled` 応答後の cleanup と follow-up dispatch を owner として扱う。`app/state.rs` の `FileListManager` は request_id と requested root を相関して `CurrentRoot` / `PreviousRoot` / `StaleRequestedRoot` を分類し、`poll_filelist_response()` が stale requested root を無視したまま reducer 的に post-settle command を dispatch できるようにする。
- 役割補足: `app/update.rs` は startup check / install request の dispatch を担当し、request_id-correlated response apply、stale 応答吸収、prompt/failure/install_started 遷移の cleanup は `app/state.rs` の `UpdateManager` command surface と合わせて owner 境界を保つ。
- 役割補足: `app/index_worker.rs` は FileList / Walker streaming、kind classification、index worker 実装を担当し、`app/workers.rs` から indexing concern を切り離す。
- 役割補足: `app/worker_protocol.rs` は search/index/preview/action/sort/kind/filelist/update の request/response 型を集約し、worker protocol surface を worker 実装や bus wiring から独立して保守できるようにする。
- 役割補足: `app/worker_runtime.rs` は worker shutdown signal と join timeout の管理だけを持ち、個別 worker 実装から runtime orchestration を分離する。
- 役割補足: `app/cache.rs` は preview/highlight/sort metadata cache state と invalidation に専念し、bounded storage と scope/eviction を局所化する。preview/highlight/routing の orchestration は `app/preview_flow.rs` へ、sort の orchestration は `app/result_flow.rs` へ、実際の response apply と snapshot refresh は `app/result_reducer.rs` へ分離する。
- 役割補足: `app/worker_support.rs` は worker routing の共通 helper と action target helper を担当し、`workers.rs` から reusable helper を切り離す。
- 役割補足: search domain は `search/mod.rs` を public API と query compile の入口に保ちつつ、prefix cache は `search/cache.rs`、execution mode と parallel tuning は `search/config.rs`、candidate collect は `search/execute.rs`、ranking/materialization は `search/rank.rs` へ分割して保守する。
- 役割補足: indexer domain は `indexer/mod.rs` を build orchestration と nested FileList override の入口に保ちつつ、FileList read は `indexer/filelist_reader.rs`、walker は `indexer/walker.rs`、FileList write/ancestor propagation は `indexer/filelist_writer.rs` へ分割して保守する。
- 役割補足: 環境変数は `user-facing` (`FLISTWALKER_RESTORE_TABS`, `FLISTWALKER_DISABLE_HISTORY_PERSIST`)、`dev/test override` (`FLISTWALKER_SEARCH_*`, `FLISTWALKER_WALKER_*`, `FLISTWALKER_WINDOW_TRACE*`, update 手動試験 override)、`build/release` (`FLISTWALKER_UPDATE_*_HEX`, `FLISTWALKER_WINDOWS_*`, `FLISTWALKER_MACOS_SIGN_IDENTITY`) に分類し、公開 docs には user-facing だけを既定導線として載せる。
- 役割補足: candidate は `entry.rs` の `Entry { path, kind }` で app/index/search worker 境界をまたいで表現し、app 側の kind side-channel を持たない。
- 実装: `rust/src/app/mod.rs`, `rust/src/app/coordinator.rs`, `rust/src/app/filelist.rs`, `rust/src/app/update.rs`, `rust/src/app/render.rs`, `rust/src/app/input.rs`, `rust/src/app/session.rs`, `rust/src/app/state.rs`, `rust/src/app/tab_state.rs`, `rust/src/app/tabs.rs`, `rust/src/app/pipeline.rs`, `rust/src/app/pipeline_owner.rs`, `rust/src/app/bootstrap.rs`, `rust/src/app/cache.rs`, `rust/src/app/result_reducer.rs`, `rust/src/app/result_flow.rs`, `rust/src/app/preview_flow.rs`, `rust/src/app/worker_bus.rs`, `rust/src/app/worker_protocol.rs`, `rust/src/app/worker_runtime.rs`, `rust/src/app/worker_support.rs`, `rust/src/app/shell_support.rs`, `rust/src/app/ui_state.rs`, `rust/src/app/query_state.rs`, `rust/src/app/search_coordinator.rs`, `rust/src/app/index_coordinator.rs`, `rust/src/app/index_worker.rs`, `rust/src/app/workers.rs`, `rust/src/entry.rs`, `rust/src/ui_model.rs`, `rust/src/query.rs`, `rust/src/search/mod.rs`, `rust/src/search/cache.rs`, `rust/src/search/config.rs`, `rust/src/search/execute.rs`, `rust/src/search/rank.rs`, `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/walker.rs`, `rust/src/indexer/filelist_writer.rs`

- DES-010 GUI Test Artifacts
- 役割: GUI 回帰手順と結果を管理する。
- 実装: `docs/TESTPLAN.md` の GUI 手順節

- DES-012 CI / Release Hygiene
- 役割: release 対象 OS の CI 継続検証、依存脆弱性検査、notarization 運用の文書化。
- 実装: `.github/workflows/ci-cross-platform.yml`, `docs/RELEASE.md`, `.github/release-template.md`, `rust/build.rs`

- DES-013 Result Sort Controller
- 役割: 結果スナップショット限定のソート、日付属性の遅延取得、上限付き属性キャッシュを管理。
- 実装: `rust/src/app/mod.rs`, `rust/src/app/render.rs`, `rust/src/app/cache.rs`, `rust/src/app/workers.rs`, `rust/src/app/state.rs`, `rust/src/app/pipeline.rs`

- DES-014 Self Update Coordinator
- 役割: GitHub Releases の最新 version 確認、対象 asset と sidecar 文書 (`*.README.txt`, `*.LICENSE.txt`, `*.THIRD_PARTY_NOTICES.txt`) の選択、`SHA256SUMS.sig` と `SHA256SUMS` の検証、Windows/Linux 向け staged update と再起動を制御する。
- 実装: `rust/src/updater.rs`, `rust/src/app/update.rs`, `rust/src/app/render.rs`, `rust/src/app/workers.rs`, `rust/src/app/state.rs`
- 役割補足: `check_for_update()` は release fetch と candidate 解決を分け、candidate 解決側では release asset 選択と support classification を helper 化して contract を小さく保つ。
- 役割補足: update request / apply / failure の主要遷移は `AppendWindowTrace` と worker-side `tracing` の両方で残し、worker trace は `flow=update` / `event=*` / `request_id=*` を canonical field として support 時の request_id correlation を取りやすくする。
- 役割補足: request/response trace の details には request_id を必ず含め、update_check_failed や update_failed は error 内容も併記して support 時の切り分けをしやすくする。
- 役割補足: `UpdateState` と update worker request/response の lifecycle、stale 応答吸収、prompt/failure/install_started の遷移は `app/update.rs` の manager 境界へ集約する。
- 役割補足: `render.rs` は update dialog の描画と入力取得だけを担当し、永続化 (`session.rs`) と app close orchestration (`app/mod.rs`) は `FlistWalkerApp` 側に残したまま `UpdateAppCommand` で橋渡しする。
- 役割補足: top action button、FileList dialog、update dialog、tab bar reorder/close/switch は `RenderCommand` 境界を経由して owner helper (`filelist.rs`, `update.rs`, `tabs.rs`) へ渡す。描画コードは click/drag/dialog input の収集に寄せ、state transition は描画後 dispatcher で一段遅らせて実行する。

- DES-015 Diagnostics and Supportability Contract
- 役割: worker-side `tracing` と opt-in window trace の責務を分け、support/debug 用の canonical event family を維持する。
- 実装: `rust/src/app/workers.rs`, `rust/src/app/index_worker.rs`, `rust/src/app/mod.rs`, `rust/src/app/session.rs`, `rust/src/app/input.rs`, `rust/src/main.rs`
- 役割補足: worker-side async flow は `flow` / `event` / `request_id` を中心に記録し、request-scoped でない flow は `epoch` や `source_kind` など最小の補助 field だけを追加する。
- 役割補足: search / preview / filelist / action / sort metadata / update は started/finished/failed/receiver_closed 系の event family に寄せ、index は `flow=index` と `source_kind` で filelist/walker/none を切り分ける。
- 役割補足: GUI/session/input/update の opt-in trace は `FLISTWALKER_WINDOW_TRACE=1` のみで有効化し、window geometry、IME composition、query text change、startup/update dialog などの GUI diagnostics を `append_window_trace` へ集約する。
- 役割補足: diagnostics 強化で request routing や response acceptance を変えない。hot UI path へ重い同期 I/O や新しい汎用 logging framework を導入しない。

## Main flows
- Flow-001: 起動 -> （FileList 優先モード有効時）FileList 検出 -> 読み込み -> 検索 -> 選択 -> アクション。
- Flow-002: 起動 -> FileList なし -> walker 走査 -> 検索 -> 選択 -> アクション。
- Flow-003: アクション失敗 -> エラー整形 -> 表示 -> 非ゼロ終了（CLI）/エラー通知（GUI）。
- Flow-004: GUI 起動 -> 非同期インデックス -> 最新要求優先検索（古い要求を破棄） -> プレビュー -> 実行/オープン。
- Flow-005: GUI 起動 -> update worker が GitHub Releases を確認 -> 新版あり -> 利用者承認 -> asset と sidecar 文書 (`*.README.txt`, `*.LICENSE.txt`, `*.THIRD_PARTY_NOTICES.txt`) と `SHA256SUMS.sig` / `SHA256SUMS` を取得 -> 署名検証 -> checksum 検証 -> 補助 updater 起動 -> 本体終了 -> 置換後に新版本体と sidecar 文書を同一ディレクトリへ配置して再起動。
  `FLISTWALKER_DISABLE_SELF_UPDATE=1`、または実行中バイナリと同一ディレクトリに `FLISTWALKER_DISABLE_SELF_UPDATE` ファイルがある場合は update flow を起動せず、通常起動のみ行う。

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
- 非空クエリで indexing 中の自動再検索は、差分件数と時間の両閾値（既定: 2048件・1500ms）を満たす場合のみ実行し、indexing スループット低下を抑える。
- 検索は絞り込み後にスコア計算し、上位 `limit` を返す。
- prefix cache 用の候補集合は query 延長時だけ再利用し、cache 対象件数を超える大規模マッチでは全件ランキングを保存しない。
- 検索ランキングは score 降順 + 元の候補順序で決定し、`limit` が小さい場合は partial selection で top-N のみを整列する。
- 並列検索は候補数が閾値以上の場合だけ有効化し、`FLISTWALKER_SEARCH_THREADS` で worker 内の検索スレッド数を調整できる。
- 非アクティブタブは `results` と preview のような表示キャッシュを compact し、再表示時は `base_results` と現在の sort mode から復元する。`all_entries` / `entries` / index 本体は保持してタブ復帰で再インデックスを避ける。
- tab 保存状態は query/history、index 状態、result/selection を別 struct で保持し、tab 切替や復元時に必要な束だけ同期する。
- GUI の逐次反映は2系統とする: 空クエリはインデックス蓄積分を即時表示、非空クエリは一定件数/時間の閾値を満たしたときだけ検索用スナップショットを更新する。
- FileList 解析はストリーミングで処理し、`Started` を先行通知した後にバッチ反映する。大規模 FileList でも `Source: None` 固定を避け、新しい request_id で中断可能にする。
- FileList の `\` / `/` 混在は候補生成順でプラットフォーム優先の字句選択へ寄せ、初期ストリームで `exists()` / `try_exists()` を各行へ追加しない。v0.12.3 相当のスループットを基準として維持する。
- 非 Windows の dual-filter fast path では、Windows/WSL で生成された `\` 区切り FileList の実用性を優先して slash 正規化候補を先に流す。literal `\` を含む POSIX filename の厳密判定は初期ストリームでは扱わず、曖昧性解消のための per-line probe も入れない。
- Walker 解析は `jwalk` が返す `file_type` を通常ファイル/ディレクトリの即時分類に使い、リンクや Windows shortcut のような追加確認が必要な項目だけを後続の kind resolver へ遅延させる。初期 `Finished` は後処理完了を待たないが、`Finished`/`Truncated` 後は unknown kind を自動で kind resolver キューへ積み、バックグラウンドで収束させる。
- 階層 FileList 展開は全ディレクトリ走査ではなく、読み込み済み候補から `FileList.txt` / `filelist.txt` の完全一致エントリを抽出して判定する。
- 階層 FileList 展開で子 FileList を解析する経路も `should_cancel` を伝播し、supersede 時に中断できるようにする。
- include_files/include_dirs が両方有効な FileList 解析では、初期ロード時の `metadata` 依存を避けて候補パスの投入を優先し、FILE/DIR/LINK 表示種別は別ワーカーで遅延解決する。
- regex モードは include term をクエリ単位で事前コンパイルし、候補ごとの再コンパイルを禁止する。
- regex モードでも plain token は既存の literal/fuzzy matcher を流用し、regex 構文を含む token だけを事前コンパイル済み regex matcher へ振り分ける。
- プレビューキャッシュは固定上限（FIFO）で運用し、長時間セッションでのメモリ増加を抑制する。
- preview/highlight/sort metadata cache は app coordinator 直下の flat field ではなく専用 state struct へ束ね、root 変更や index refresh 開始時にまとめて破棄できるようにする。
- 結果ソートは `base_results` に検索エンジンの元順位を保持し、表示用 `results` だけを並び替えることで `Score` 復帰を O(n) で実現する。
- `Name` ソートは UI スレッド上で `base_results` の clone を即時ソートし、追加 I/O を行わない。
- `Modified` / `Created` は結果スナップショット中の未キャッシュ path だけを sort worker へ送り、属性解決後に表示リストを更新する。
- sort worker は index/search worker とは分離し、query 編集中や indexing 中でも UI フレームを塞がない。
- sort metadata cache は上限件数を持つ FIFO/LRU 風管理とし、root 変更や index refresh 開始時に破棄できるようにする。

- DES-007 Reliability / Error
- 失敗は `anyhow::Result` に集約し、CLI/GUI で表示責務を分離する。
- 外部コマンドは引数配列で起動し、シェル解釈を避ける。
- GUI 検索はワーカーからエラー文字列を受け取り、notice に反映する。
- GUI ワーカーは shutdown フラグを共有し、`Drop` 時に停止要求 + channel 切断 + `join` で終了待機する。
- `Drop` 時の worker `join` は短いタイムアウト付き（既定 250ms）で待機し、超過時はタイムアウト件数を記録して UI 終了の無限待ちと体感遅延を同時に避ける。
- OS シグナル（例: `Ctrl+C`）受信時は shutdown 要求を立て、GUI 側で window close を発行して終了処理へ収束させる。
- FileList 作成応答は request_id と要求 root を照合し、root 変更後に到着した旧 root の完了/失敗応答では再インデックスを行わず通知のみ行う。
- Create File List は app 側で pending confirmation / pending after index / in-flight request を 1 系列の状態として管理し、status panel の `Cancel Create File List` から共通キャンセル処理へ流す。
- God Object 解消の第一段として、Create File List は `app/filelist.rs` 内の FileList 専用 reducer/manager 境界へ寄せ、FileList worker request/response の lifecycle と stale/cancel 判定だけを manager 側で所有する。
- FileList 系の副作用は `UiCommand`、`WorkerCommand`、`FileListAppCommand` のようなカテゴリ化した戻り値で表現し、単一巨大 enum や `&mut FlistWalkerApp` への直接依存を増やさない。
- `pending_after_index`、tab/root 切替時の再インデックス判断、active/background tab への反映は orchestration として `FlistWalkerApp` 側に残し、manager は必要な app command を返すだけに留める。
- FileList 作成は OS 一時領域に出力してから最終配置へ移動する。クロスデバイスで `rename` 不可の場合は `copy` フォールバックし、最終配置のみを更新する。
- Create File List の worker request は cancel flag を持ち、テキスト生成、root 直下への最終置換、祖先 FileList 追記の各境界で中断確認する。キャンセル後は `Canceled` 応答を返し、UI は notice 更新だけ行う。
- FileList 作成後は root の親から filesystem root まで順に辿り、祖先ディレクトリ直下の既存 `FileList.txt` / `filelist.txt` へ子 FileList 参照を相対表現で追記する。既に同一参照がある場合は追記しない。
- 祖先 FileList を追記した場合でも、その FileList の mtime は更新前の値へ復元し、階層 FileList の新旧判定を崩さない。
- 祖先探索や親 FileList 読込/書込/mtime 復元で失敗した場合は、その時点で祖先追記だけを静かに打ち切り、root 自身の FileList 作成結果は成功扱いのまま維持する。
- Root 変更時は旧 root 由来の選択状態（current row / pinned / preview）を即時クリアし、旧パスの実行/コピー誤操作を防ぐ。
- Root 変更時は旧 root 向けの FileList 上書き確認ダイアログを破棄し、誤上書きを防ぐ。
- root 配下判定はアクション実行直前にのみ行い、FileList 読み込みや walker indexing のスループットへ影響させない。
- root 配下判定は `root == path` または `path.starts_with(root)` を基本とし、UNC root でも同一 share / 同一 root 配下なら許可する。
- root 外パスは候補表示を維持しつつ、Action worker が OS 起動要求を出す前に拒否して notice を返す。
- Create File List は root 直下の FileList 作成と祖先追記を分離し、祖先追記がありうる場合のみ GUI 側の確認ダイアログを通す。
- 利用者が祖先追記を拒否した場合、root 直下の FileList 作成は成功扱いのまま維持し、祖先追記経路だけをスキップする。
- Source が FileList のアクティブタブで Create File List を要求した場合は、新規タブを作らずに同一タブへ `use_filelist = false` の一時 index request を発行する。完了後はその Walker snapshot で FileList を作成し、FileList 作成完了応答で同一タブを通常の FileList 再インデックスへ戻す。
- FileList 作成完了時の再インデックス対象は request に紐づく tab_id/root で判定し、元タブが background 化していてもその tab へ再インデックス request を投げる。完了時点で元タブの root が変わっていた場合は旧 root 応答として notice のみ更新し、tab 状態は戻さない。
- `Ctrl+Shift+C`（macOS では `Cmd+Shift+C`）は TextEdit の既定コピー処理より後段で実行し、検索窓フォーカス中でも選択パスコピーを優先する。
- Windows のプレビュー抑止判定は属性ビットだけに依存せず、`FileAttributeTagInfo` と `CfGetPlaceholderStateFromAttributeTag` を使って Cloud Files API 準拠 placeholder を検出する。属性/タグ取得に失敗した場合のみ既存の属性ビット判定へフォールバックする。
- プレビューデコーダは拡張子を見ず、先頭 64KiB を対象に UTF-8、BOM 付き UTF-16、その後に主要レガシー文字コードを順に試す。候補ごとに decode error と制御文字比率を評価し、妥当なテキストだけを preview に採用する。
- query 履歴はアプリ共通 state として保持し、全タブから同じ履歴集合を参照できるようにする。
- query 履歴保存は入力経路から独立して管理し、TextEdit / IME フォールバック / Emacs 風編集のどの入力経路でも「一定時間の無入力」または `Results` 移動開始時に最終 query だけを記録する。
- IME 合成中は履歴確定を抑止し、`CompositionEnd` 後に反映された確定文字列のみが履歴候補になるようにする。
- `Ctrl+R` は履歴検索モードを開始し、同じ検索欄を履歴検索入力へ切り替える。履歴検索中は `Enter` / `Ctrl+J` / `Ctrl+M` で選択中履歴を query へ展開し、`Esc` / `Ctrl+G` で開始前 query を復元してキャンセルする。
- query 履歴は通常終了時の UI state に最大 100 件まで永続化し、次回起動時に後方互換を保って復元する。
- `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` のときは、UI state 読み書き時に query history フィールドを空として扱い、履歴の永続化だけを無効にする。
- 結果ソート状態はタブ単位で保持するが、query 変更や結果スナップショット更新時には `Score` へ戻し、保留中の sort request_id を無効化する。
- 結果ペイン上部に `Sort` ドロップダウンを配置し、`Score` / `Name (A-Z)` / `Name (Z-A)` / `Modified (New)` / `Modified (Old)` / `Created (New)` / `Created (Old)` を選択可能にする。
- `Created` 属性は取得失敗を正常系として扱い、notice ではなく並び順の末尾送りだけで吸収する。
- タブ復元は `FLISTWALKER_RESTORE_TABS=1` のときだけ有効化し、永続化対象は `root/query/use_filelist/use_regex/include_files/include_dirs/tab_accent/active_tab` に限定する。
- 起動時の優先順位は `--root` 明示 > 復元タブ（env 有効時） > 最後に使っていた root > `Set as default` > 通常 root とし、バージョン更新やバイナリ差し替えでも最後の root を維持する。
- `FLISTWALKER_RESTORE_TABS=1` が有効な間は root 行の `Set as default` ボタンを disabled 表示にし、ロジック側でも no-op + notice で排他を強制する。
- タブ復元時は active tab だけ即時 `request_index_refresh()` を行い、background tab は `pending_restore_refresh` を保持して初回 `switch_to_tab_index()` 時に lazy refresh する。
- background tab の search/index 応答は active tab の結果スナップショットへ直接触れず、tab-local state へ適用してから activation 時の restore 経路で前面へ戻す。
- タブ並び替えは `render_tab_bar` がドラッグ開始/ホバー/ドロップを扱い、実際の `Vec<AppTabState>` 更新は `move_tab(from, to)` に集約する。
- `move_tab` は並び替え前に `sync_active_tab_state()` を実行し、active tab の `tab.id` を基準に移動後 index を再解決することで UI 状態の取り違えを防ぐ。
- タブ accent は `DropSendTo` の Slot Color palette を参照し、light/dark theme ごとに background/border/foreground を切り替える。
- `render_tab_bar` は active tab の accent を full-fill、inactive tab の accent を下辺の glow/line 装飾として描画し、右クリックメニューから accent の設定/解除を行う。
- Root 変更時は query 自体を維持しつつ、履歴参照位置と draft query のみ破棄して root 跨ぎの戻り操作を防ぐ。
- 検索窓フォーカス中でも `ArrowUp` / `ArrowDown` / `Ctrl+I` / `Ctrl+J` / `Ctrl+M` はアプリ側ショートカットを優先処理し、結果移動・PIN トグル・実行を抑止しない。
- Windows の一般 `.ps1` は検索結果からの既定操作では直接実行せず、既定アプリでオープンする。自己更新用の内部 PowerShell script は updater モジュールからのみ起動する。
- 自己更新は `SHA256SUMS.sig` を埋め込み公開鍵で検証してから `SHA256SUMS` を信頼し、staged binary の checksum 検証へ進む。検証失敗時は既存バイナリと UI セッションを維持する。
- Windows の自己更新は実行中 EXE とは別実体の PowerShell スクリプトを一時配置して `powershell.exe -ExecutionPolicy Bypass -WindowStyle Hidden` で非表示起動し、`Copy-Item -LiteralPath` ベースで本体と `README.txt` / `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` を配置する。
- Linux の自己更新は一時 shell script を起動して本体終了を待ち、`cp` + `chmod` 後に新 binary を再起動する。sidecar 文書（`README.txt` / `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt`）も同一ディレクトリへ同期する。
- macOS は最新 version 検知のみ実施し、自動適用は非対応として release URL への案内に留める。

- DES-008 Testability
- indexer/search/actions/ui_model を独立モジュール化。
- query 解釈は `rust/src/query.rs` へ集約し、search と UI highlight で同じ token 分解・正規化を再利用する。
- OS 依存処理は抽象境界を薄くして単体テスト可能性を維持。
- app regression tests は monolithic な `FlistWalkerApp` fixture へ集約し続けず、owner/command seam ごとに module を分ける。update lifecycle は `rust/src/app/tests/update_commands.rs`、session restore/startup root は `rust/src/app/tests/session_restore.rs`、tab interaction/background routing は `rust/src/app/tests/session_tabs.rs`、index/filelist lifecycle は `rust/src/app/tests/index_pipeline/*` を基準に保守する。

- DES-011 Window/IME Stability (Windows)
- マルチディスプレイ跨ぎ時の一時的な巨大ウィンドウサイズを永続化しないよう、保存前に monitor 幅/高さでジオメトリをクランプする。
- 起動復元は保存済み座標/サイズを適用するが、monitor 情報がある場合は復元時にも同様の上限を適用して再発を防ぐ。
- Windows は起動時に System DPI Aware を有効化し、モニタ跨ぎ時の OS 側自動リサイズ揺れを低減する。
- IME 確定文字が TextEdit 側で落ちるフレーム向けに `CompositionEnd` 文字列のフォールバック反映を行う。
- `Space` / `Shift+Space` は IME/バックエンド差異があっても、TextEdit 側で空白未反映なら最低限の半角スペースをフォールバック挿入する。
- IME 関連の分岐は `FLISTWALKER_WINDOW_TRACE=1` で追跡し、`FLISTWALKER_WINDOW_TRACE_PATH` で出力先を明示指定できる。
- デバッグ用トレースは既定無効（`FLISTWALKER_WINDOW_TRACE=1` のときのみ有効）とし、通常運用への影響を避ける。

- DES-012 CI / Release Hygiene
- 通常 CI matrix に Linux を追加し、release 対象 OS と同じ観点で `cargo test --locked` を継続実行する。
- 依存脆弱性は `cargo audit` を CI で必須実行し、既知 CVE の流入を早期検知する。
- `x86_64-pc-windows-gnu` では `windres` が生成する `resource.o` を `flistwalker` バイナリへ直接リンクし、ライブラリターゲットだけに閉じた resource link で Explorer アイコンが欠落しないようにする。
- macOS notarization は現段階では手動ゲートとして維持し、draft release 作成後に docs / template で確認手順を明示する。

- DES-014 Self Update Coordinator
- GitHub Releases API の latest endpoint から `tag_name` / asset 名 / `browser_download_url` を取得し、現在 version と比較する。
- asset 選択は release asset 命名規則から current platform/arch と一致する standalone binary、対応する `*.README.txt` / `*.LICENSE.txt` / `*.THIRD_PARTY_NOTICES.txt`、`SHA256SUMS` / `SHA256SUMS.sig` を選択する。
- update worker は check/download を担当し、GUI 側は dialog 表示と再起動要求だけを扱う。
- 起動時の update check 失敗は worker からエラー文字列つきで返し、GUI 側は通常操作を継続したまま軽量ダイアログで理由を表示する。利用者が「今後この種の起動時エラーを表示しない」を選んだ場合は UI state へ永続化し、次回以降の startup check failure dialog を抑止する。
- 更新署名公開鍵はビルド時環境変数から埋め込み、未設定ビルドでは Windows/Linux でも update candidate を manual-only に落として自動更新不能を明示する。
- restart 時は現在 executable path を置換対象とし、起動引数は最小化して通常 GUI 起動へ戻す。セッション復元は既存 UI state に委譲する。
- update dialog は `skip until next version` のチェック状態を持ち、Later 選択時に current target version を UI state へ永続化する。
- 起動時の update 応答は保存済み `skipped_update_target_version` と semver 比較し、target version がそれ以下なら dialog を出さず、より新しい version のみ再通知する。
- `FLISTWALKER_DISABLE_SELF_UPDATE` が truthy、または実行中バイナリと同一ディレクトリに同名ファイルがある場合は GUI 側で起動時 update request 自体を送らず、update install 側でも同じ判定で最終ガードする。
- 手動試験用 override として `FLISTWALKER_UPDATE_FEED_URL`, `FLISTWALKER_UPDATE_ALLOW_SAME_VERSION=1`, `FLISTWALKER_UPDATE_ALLOW_DOWNGRADE=1`, `FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE` を読み取り、通常運用の GitHub latest 比較や startup failure dialog を内部検証用に再現できるようにする。

## Error handling / timeout / logging / metrics
- エラー戦略: ファイルアクセス失敗、実行失敗、正規表現不正を分類して表示。
- タイムアウト: 外部プロセス起動はブロッキング待機しない。
- ログ: 現状は標準出力/標準エラー中心。必要に応じて構造化ログへ拡張。
- ログ補足: worker-side supportability trace は `RUST_LOG` で opt-in し、canonical field (`flow`, `event`, `request_id`) を優先する。GUI/session/input diagnostics は `FLISTWALKER_WINDOW_TRACE=1` の file trace を使い分ける。
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
- DES-003 -> TC-092 (SP-003, SP-010)
- DES-004 -> TC-004, TC-005 (SP-004, SP-005)
- DES-005 -> TC-006 (SP-006)
- DES-006 -> TC-007 (SP-007)
- DES-007 -> TC-008 (SP-008)
- DES-008 -> TC-009 (SP-009)
- DES-009 -> TC-010 (SP-010)
- DES-009 -> TC-068 (SP-010)
- DES-009 -> TC-069 (SP-010)
- DES-010 -> TC-011 (SP-011)
- DES-011 -> TC-020 (SP-010, SP-011)
- DES-012 -> TC-056 (SP-012)
- DES-013 -> TC-057, TC-058, TC-059, TC-060 (SP-013)
- DES-014 -> TC-074, TC-075, TC-076, TC-077, TC-078, TC-081 (SP-014)
- DES-015 -> TC-100 (SP-010, SP-014)
