# ARCHITECTURE

## Overview
FlistWalker は Rust 製の GUI/CLI ハイブリッド検索ツールで、FileList 優先インデクシングと walker ベース再帰走査の両方を扱う。主要コンポーネントは search/index domain、GUI coordinator、OS integration、shared utility に分かれる。

## Top-Level Modules
- [main.rs](../rust/src/main.rs)
  - CLI entrypoint。引数解釈と GUI/CLI 起動分岐を担当する。
- [lib.rs](../rust/src/lib.rs)
  - 共有モジュール公開面。
- [entry.rs](../rust/src/entry.rs)
  - `Entry` / `EntryKind` を定義し、index/search/app 間の候補表現を統一する。
- [indexer/mod.rs](../rust/src/indexer/mod.rs)
  - index build orchestration と nested FileList override を担当する。
- [indexer/filelist_reader.rs](../rust/src/indexer/filelist_reader.rs)
  - FileList 検出、stream parse、hierarchy 読み込みを担当する。
- [indexer/filelist_writer.rs](../rust/src/indexer/filelist_writer.rs)
  - FileList text 生成、書き出し、ancestor 伝播、write 補助を担当する。
- [indexer/walker.rs](../rust/src/indexer/walker.rs)
  - walker ベースの file/dir 収集を担当する。
- [query.rs](../rust/src/query.rs)
  - fzf 互換 query tokenization と演算子解釈を担当する。
- [search/mod.rs](../rust/src/search/mod.rs)
  - query compile と public search API の入口を担当する。
- [search/cache.rs](../rust/src/search/cache.rs)
  - prefix cache と snapshot key を担当する。
- [search/config.rs](../rust/src/search/config.rs)
  - parallel threshold / thread count / execution mode 判定を担当する。
- [search/execute.rs](../rust/src/search/execute.rs)
  - sequential / parallel collect と candidate evaluation の execution path を担当する。
- [search/rank.rs](../rust/src/search/rank.rs)
  - ranking、result materialization、visible-result filter を担当する。
- [ui_model.rs](../rust/src/ui_model.rs)
  - highlight 判定、preview 文面、表示パス整形を担当する。
- [path_utils.rs](../rust/src/path_utils.rs)
  - Windows path normalization と path identity helper を担当する。
- [app/coordinator.rs](../rust/src/app/coordinator.rs)
  - status line、notice/status helper、frame/update glue、root/path compare の coordinator helper を担当する。
- [actions.rs](../rust/src/actions.rs)
  - open / execute の OS 差分吸収と testable seam を担当する。
- [updater.rs](../rust/src/updater.rs)
  - self-update 判定、asset 選択、staged update を担当する。
- [update_security.rs](../rust/src/update_security.rs)
  - update manifest 署名検証を担当する。
- [fs_atomic.rs](../rust/src/fs_atomic.rs)
  - atomic write helper。

## app Coordinator
[mod.rs](../rust/src/app/mod.rs) の `FlistWalkerApp` は egui/eframe の coordinator であり、feature 実装は `rust/src/app/` に分割されている。state holder は worker / UI / query の単位でも分離されている。

- `FlistWalkerApp` に残す fixed point は「egui/eframe entrypoint」「top-level orchestration」「cross-feature state holder」「owner API の呼び分け」に限定し、feature ごとの state transition と policy 判定は各 owner module へ寄せる。
- `FlistWalkerApp` の state inventory は少なくとも次の 4 束で追跡する。
  - `app-global shared state`
    - root、filter flag、index snapshot、worker-facing coordinator state のように active tab 以外でも参照される束。
  - `active-tab-local state`
    - query、results、selection、preview、notice のように現在前面 tab にだけ適用される束。
  - `persisted/background tab state`
    - `AppTabState` と restore/switch/reorder/request-routing に紐づく tab/session 束。
  - `feature dialog/update state`
    - FileList dialog、self-update dialog、window/root browser など feature 単位の補助束。live owner は `state.rs` の `FeatureStateBundle` に寄せる。
- `mod.rs` の責務棚卸しは少なくとも次の 6 区分で追跡する。
  - `startup/bootstrap`
    - 残置: `new` / `from_launch` / `new_with_launch` による app 初期化の入口。
    - owner: worker 起動と launch seed 構築は [bootstrap.rs](../rust/src/app/bootstrap.rs)、session restore は [session.rs](../rust/src/app/session.rs)。
  - `frame update cycle`
    - 残置: egui frame ごとの top-level update loop と repaint 判断。
    - owner: index/search/poll の lifecycle は [pipeline.rs](../rust/src/app/pipeline.rs)、active result refresh は [pipeline_owner.rs](../rust/src/app/pipeline_owner.rs)、render command は [render.rs](../rust/src/app/render.rs)。
  - `shutdown/persist`
    - 残置: eframe callback から shutdown seam を呼ぶ top-level exit orchestration。
    - owner: state 永続化は [session.rs](../rust/src/app/session.rs)、worker join/shutdown は [worker_runtime.rs](../rust/src/app/worker_runtime.rs)。
  - `tab routing`
    - 残置: active tab index と owner API 呼び分け。
    - owner: tab snapshot / switch / move / close / background response apply は [tabs.rs](../rust/src/app/tabs.rs)。
  - `filelist/update dialog dispatch`
    - 残置: dialog command の top-level dispatch と status/notice 連携。
    - owner: FileList flow は [filelist.rs](../rust/src/app/filelist.rs)、self-update lifecycle は [update.rs](../rust/src/app/update.rs)。
  - `trace helper`
    - 残置: opt-in 診断の入口と egui lifecycle に紐づく top-level trace 発火。
    - owner: worker protocol tracing は [workers.rs](../rust/src/app/workers.rs) と [worker_protocol.rs](../rust/src/app/worker_protocol.rs)、window/session diagnostics は [session.rs](../rust/src/app/session.rs) と各 owner helper。

- [bootstrap.rs](../rust/src/app/bootstrap.rs)
  - worker 起動と launch seed 構築。
- [session.rs](../rust/src/app/session.rs)
  - saved roots、UI state 永続化、window geometry restore、shutdown/persist owner。
- [tabs.rs](../rust/src/app/tabs.rs)
  - tab lifecycle、snapshot capture/apply、background tab 向け search/index response apply、activation 時の restore/refresh 入口、tab routing owner を担当する。live 側の tab/session orchestration は `state.rs` の `TabSessionState` を介して保持する。
- [pipeline.rs](../rust/src/app/pipeline.rs)
  - index queue、index response poll、dispatcher を担当し、active path は `PipelineOwner`、background path は `tabs.rs` の background-flow helper 群へ橋渡しする thin coordinator として振る舞う。
- [pipeline_owner.rs](../rust/src/app/pipeline_owner.rs)
  - active tab 向け search/result refresh、entry filter 再適用、incremental search refresh を担当する dedicated owner surface。
- [search_coordinator.rs](../rust/src/app/search_coordinator.rs)
  - search worker channel、request_id、tab routing に加え、active/background search request lifecycle helper を保持する。
- [index_coordinator.rs](../rust/src/app/index_coordinator.rs)
  - index worker channel、queue/inflight、incremental state、background tab state に加え、request id 採番、active/background refresh 開始、terminal cleanup など index request lifecycle の owner API を保持する。
- [index_worker.rs](../rust/src/app/index_worker.rs)
  - FileList / Walker streaming、kind classification、index worker thread 実装を担当する。
- [worker_protocol.rs](../rust/src/app/worker_protocol.rs)
  - search/index/preview/action/sort/kind/filelist/update の request/response 型を集約し、worker protocol surface を実装モジュールから分離する。
- [worker_bus.rs](../rust/src/app/worker_bus.rs)
  - preview/action/sort/kind/filelist/update worker channel を束ねる。
- [worker_runtime.rs](../rust/src/app/worker_runtime.rs)
  - worker shutdown signal と join timeout 管理を担当する。
- [worker_support.rs](../rust/src/app/worker_support.rs)
  - worker routing の共通 helper と action target helper を集約する。
- [ui_state.rs](../rust/src/app/ui_state.rs)
  - runtime UI focus、scroll、preview panel、tab drag などの一時状態を保持する。
- [query_state.rs](../rust/src/app/query_state.rs)
  - query、history、history search、kill buffer をまとめる。
- [cache.rs](../rust/src/app/cache.rs)
  - preview/highlight/sort metadata cache state と bounded invalidation を担当する。
- [result_flow.rs](../rust/src/app/result_flow.rs)
  - result sort の orchestration を担当する。
- [preview_flow.rs](../rust/src/app/preview_flow.rs)
  - preview request/response、highlight lookup、preview routing の orchestration を担当する。
- [render.rs](../rust/src/app/render.rs)
  - panel/dialog/results 描画と frame 後段の render command 生成。
- [input.rs](../rust/src/app/input.rs)
  - shortcut、IME、history search。
- [filelist.rs](../rust/src/app/filelist.rs)
  - FileList 作成フローと filelist dialog dispatch owner。
- [update.rs](../rust/src/app/update.rs)
  - self-update dialog と update state transition。request_id-correlated な update trace を supportability 用に橋渡しし、update dialog dispatch owner として振る舞う。
- [state.rs](../rust/src/app/state.rs)
  - GUI 横断 state 型。`FileListManager` / `UpdateManager` / `RootBrowserState` を束ねる `FeatureStateBundle`、live tab/session registry を束ねる `TabSessionState`、cache/request routing など bundle inventory の受け皿として扱う。`AppRuntimeState` は `FlistWalkerApp` の app-global / active-result live state を束ねる runtime bundle として扱う。
- [tab_state.rs](../rust/src/app/tab_state.rs)
  - tab snapshot 用 state 型。`AppTabState` は persisted/background tab state の canonical snapshot とし、active tab 側の live state とは区別して追跡する。
- [workers.rs](../rust/src/app/workers.rs)
  - search/preview/action/sort/update/filelist/kind worker の spawn 実装を担当する。

## App Test Boundaries
- `rust/src/app/tests/update_commands.rs`
  - update dialog / manager / worker-response lifecycle を command 境界で検証する。
- `rust/src/app/tests/session_restore.rs`
  - startup root 選択、saved tab sanitize、restore 時の active/background tab 初期化を restore owner 境界で検証する。
- `rust/src/app/tests/session_tabs.rs`
  - tab switch/reorder/close と background tab 応答 routing を tab owner 境界で検証する。
- `rust/src/app/tests/index_pipeline/*`
  - filelist、index inflight、kind resolution、search refresh の pipeline lifecycle を command 単位で検証する。
- `rust/src/app/tests/app_core.rs`
  - app coordinator の汎用 state transition と cross-cutting regression だけを保持し、update/restore の専用責務は各 owner-aligned module へ寄せる。

## Threading Model
- UI thread:
  - egui frame ごとに request enqueue と response poll を行う。
- Search worker:
  - query と entry snapshot を受けて結果を返す。
- Index worker:
  - FileList / walker を使って candidate を生成する。
- Preview worker:
  - テキスト preview を生成する。
- Action / sort / kind / filelist / update workers:
  - 補助的な非同期処理を担当する。
- Tracing:
  - `RUST_LOG` 指定時のみ worker-side `tracing` が有効になり、async worker flow は canonical な `flow` / `event` / `request_id`（request-scoped flow のみ）を構造化 field として記録する。
  - search / preview / filelist / action / sort metadata / update worker は `started` / `finished` / `failed` / `receiver_closed` 系の event family を使い、support 時に request 単位で追跡できるようにする。
  - index worker は `flow=index` を共有しつつ `source_kind=filelist|walker|none` を併記して started/finished/completed/superseded/failed を記録する。
  - `FLISTWALKER_WINDOW_TRACE=1` は GUI/session/input/update の opt-in 診断面であり、`append_window_trace` 経由の event family を `RUST_LOG` 側へ混在させない。

request_id によって最新応答だけを反映し、古い応答による UI 巻き戻りを防ぐ。

## Main Data Flow
1. launch settings と root を決定する。
2. index request を発行する。
3. index response から `all_entries` / `entries` を更新する。
4. query が空なら一覧表示、非空なら search request を発行する。
5. current row に応じて preview request を発行する。
6. action または update request を必要に応じて発行する。

## Shared Utility Policy
OS ごとの差異や表示用正規化のような cross-cutting helper は app 内に複製せず shared utility に寄せる。

- [path_utils.rs](../rust/src/path_utils.rs)
  - Windows パス正規化、path identity、display/shell 変換。
- [ui_model.rs](../rust/src/ui_model.rs)
  - preview/highlight 計算。
- [fs_atomic.rs](../rust/src/fs_atomic.rs)
  - 原子的ファイル書き込み。

## Related Docs
- [REQUIREMENTS.md](./REQUIREMENTS.md)
- [SPEC.md](./SPEC.md)
- [DESIGN.md](./DESIGN.md)
- [TESTPLAN.md](./TESTPLAN.md)
- [RELEASE.md](./RELEASE.md)
- [TASKS.md](./TASKS.md)

## Document Roles
- `REQUIREMENTS.md` / `SPEC.md`
  - プロダクト契約と振る舞いの規範面を保持する。
- `ARCHITECTURE.md` / `DESIGN.md`
  - steady-state の構造、責務境界、運用上の設計判断を保持する。
- `TESTPLAN.md`
  - 継続運用する validation matrix、manual test、regression guard を保持する。
- `RELEASE.md`
  - release 運用と配布手順を保持する。
- `TASKS.md`
  - change program の履歴、closure record、恒久的に残す進捗サマリを保持する。

## Regression Guards

### Kind Resolution Arc Clone Freeze (v0.16.0)

- Scenario: 50万件のエントリを持つ Walker インデクシングの途中で、UI が `KindResolverWorker` から解決済みのメタデータ（`EntryKind`）バッチを 512 個受け取る。このとき `apply_entry_kind_updates` 内で `Arc::make_mut(&mut self.entries)` などを呼び出すと、検索スレッドが別のスナップショットをホールドしているため毎フレーム50万要素の巨大な `Arc<Vec<Entry>>` クローン（数百万回の `PathBuf` アロケーション）が発生し、UI が完全にフリーズする。
- Expected Behavior: UI は巨大リストに対する O(N) のクローンをせず、`self.cache.entry_kind` へ解決された Kind を安全に書き込み、それを参照することでメタデータ解決の恩恵（アイコン表示やフィルタ反映）を O(1) で得る。
- Non-goals: `WalkDir` 自身の遅延解決以外の部分（メタデータ取得そのものの OS I/O 遅延）は Background スレッドに閉じるため本件の責務ではない。
- Related Tests: `poll_kind_response_does_not_clone_arc_shared_entries_regression` (in `kind_resolution.rs`)
- Notes for Future Changes: `app.entries` および他のエントリリスト(`all_entries`, `incremental_filtered_entries`, `index.entries`) に対して、チャンク処理中のループ内で一括更新を目的に `mut` 参照を要求・上書きしてはいけない。常に Cache (辞書) を更新し、描画やフィルタリング時は Cache を `Entry` より優先して参照すること。
