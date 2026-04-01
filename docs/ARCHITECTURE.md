# ARCHITECTURE

## Overview
FlistWalker は Rust 製の GUI/CLI ハイブリッド検索ツールで、FileList 優先インデクシングと walker ベース再帰走査の両方を扱う。主要コンポーネントは search/index domain、GUI coordinator、OS integration、shared utility に分かれる。

## Top-Level Modules
- [main.rs](/mnt/d/work/flistwalker/rust/src/main.rs)
  - CLI entrypoint。引数解釈と GUI/CLI 起動分岐を担当する。
- [lib.rs](/mnt/d/work/flistwalker/rust/src/lib.rs)
  - 共有モジュール公開面。
- [indexer.rs](/mnt/d/work/flistwalker/rust/src/indexer.rs)
  - FileList 優先読み込み、walker 走査、FileList 書き出しを担当する。
- [query.rs](/mnt/d/work/flistwalker/rust/src/query.rs)
  - fzf 互換 query tokenization と演算子解釈を担当する。
- [search.rs](/mnt/d/work/flistwalker/rust/src/search.rs)
  - candidate scoring と top-N 抽出を担当する。
- [ui_model.rs](/mnt/d/work/flistwalker/rust/src/ui_model.rs)
  - highlight 判定、preview 文面、表示パス整形を担当する。
- [actions.rs](/mnt/d/work/flistwalker/rust/src/actions.rs)
  - open / execute の OS 差分吸収を担当する。
- [updater.rs](/mnt/d/work/flistwalker/rust/src/updater.rs)
  - self-update 判定、asset 選択、staged update を担当する。
- [update_security.rs](/mnt/d/work/flistwalker/rust/src/update_security.rs)
  - update manifest 署名検証を担当する。
- [fs_atomic.rs](/mnt/d/work/flistwalker/rust/src/fs_atomic.rs)
  - atomic write helper。
- [path_utils.rs](/mnt/d/work/flistwalker/rust/src/path_utils.rs)
  - Windows extended path prefix 除去と display/shell 用 path 正規化を担当する。

## app Coordinator
[app.rs](/mnt/d/work/flistwalker/rust/src/app.rs) の `FlistWalkerApp` は egui/eframe の coordinator であり、feature 実装は `rust/src/app/` に分割されている。

- [bootstrap.rs](/mnt/d/work/flistwalker/rust/src/app/bootstrap.rs)
  - worker 起動と launch seed 構築。
- [session.rs](/mnt/d/work/flistwalker/rust/src/app/session.rs)
  - saved roots、UI state 永続化、window geometry restore。
- [tabs.rs](/mnt/d/work/flistwalker/rust/src/app/tabs.rs)
  - tab lifecycle、snapshot capture/apply。
- [pipeline.rs](/mnt/d/work/flistwalker/rust/src/app/pipeline.rs)
  - index/search queue、response poll、incremental refresh。
- [cache.rs](/mnt/d/work/flistwalker/rust/src/app/cache.rs)
  - preview/highlight cache、preview request/response。
- [render.rs](/mnt/d/work/flistwalker/rust/src/app/render.rs)
  - panel/dialog/results 描画。
- [input.rs](/mnt/d/work/flistwalker/rust/src/app/input.rs)
  - shortcut、IME、history search。
- [filelist.rs](/mnt/d/work/flistwalker/rust/src/app/filelist.rs)
  - FileList 作成フロー。
- [update.rs](/mnt/d/work/flistwalker/rust/src/app/update.rs)
  - self-update dialog と update state transition。
- [state.rs](/mnt/d/work/flistwalker/rust/src/app/state.rs)
  - GUI 横断 state 型。
- [tab_state.rs](/mnt/d/work/flistwalker/rust/src/app/tab_state.rs)
  - tab snapshot 用 state 型。
- [workers.rs](/mnt/d/work/flistwalker/rust/src/app/workers.rs)
  - worker request/response 型と worker 実装。

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

- [path_utils.rs](/mnt/d/work/flistwalker/rust/src/path_utils.rs)
  - Windows パス正規化と display/shell 変換。
- [ui_model.rs](/mnt/d/work/flistwalker/rust/src/ui_model.rs)
  - preview/highlight 計算。
- [fs_atomic.rs](/mnt/d/work/flistwalker/rust/src/fs_atomic.rs)
  - 原子的ファイル書き込み。

## Related Docs
- [REQUIREMENTS.md](/mnt/d/work/flistwalker/docs/REQUIREMENTS.md)
- [SPEC.md](/mnt/d/work/flistwalker/docs/SPEC.md)
- [DESIGN.md](/mnt/d/work/flistwalker/docs/DESIGN.md)
- [TESTPLAN.md](/mnt/d/work/flistwalker/docs/TESTPLAN.md)
- [RELEASE.md](/mnt/d/work/flistwalker/docs/RELEASE.md)
