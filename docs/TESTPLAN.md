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
- App test module policy:
- app-level regression は owner/command seam ごとに module を分けて保守する。update は `rust/src/app/tests/update_commands.rs`、session restore は `rust/src/app/tests/session_restore.rs`、tab/background routing は `rust/src/app/tests/session_tabs.rs`、index/filelist lifecycle は `rust/src/app/tests/index_pipeline/*` を主対象にし、`app_core.rs` へ unrelated fixture regression を増やし続けない。
- routing / cleanup の確認は `rust/src/app/response_flow.rs`、`rust/src/app/result_reducer.rs`、`rust/src/app/index_coordinator.rs`、`rust/src/app/pipeline.rs`、`rust/src/app/tab_state.rs`、`rust/src/app/worker_bus.rs` を owner seam として扱い、background response の stale discard と tab close cleanup を `session_tabs.rs` / `index_pipeline/filelist_lifecycle.rs` へ寄せる。
- `FeatureStateBundle` / `TabSessionState` のような state bundle 導入後も、bundle 単位の ownership を直接確認したい回帰は既存 owner test module に寄せ、bundle 配置だけを検証するための横断 fixture を増やさない。
- stale response discard、cancel cleanup、pending/inflight 解放の契約は `update_commands.rs` と `index_pipeline/*` を優先対象にし、`app_core.rs` へ cross-cutting でない lifecycle regression を戻さない。
- filelist response の current/previous/stale-requested-root 分岐は `rust/src/app/tests/index_pipeline/filelist_lifecycle.rs` を owner test とし、request cleanup と post-settle routing を同じ module で固定する。
- 環境変数は validation 上 `user-facing`、`dev/test override`、`build/release` に分けて扱う。`README.md` に載せるのは user-facing のみとし、dev/test override は `TESTPLAN.md` と実装近傍 test に閉じる。
- GUI Manual:
- 起動、検索、選択、プレビュー、実行/オープン、再読込を手順化して検証。
- Perf/Sec:
- Perf: 10万件相当ダミー候補で検索時間計測。
- Perf: 軽量 PR gate は `perf_regression_filelist_stream_matches_v0123_reference_budget` とし、FileList stream budget を日常 validation に載せる。heavy suite は `perf_walker_classification_is_faster_than_eager_metadata_resolution` として分離する。
- Sec: コマンド引数を配列化しシェルインジェクションを回避。
- Sec: root 外パス実行拒否、履歴永続化無効化、CI の依存脆弱性検査を確認。
- Sec: Windows の一般 `.ps1` は既定で直接実行せず、既定アプリでオープンする。
- Sec: 自己更新は `SHA256SUMS.sig` の署名検証と checksum 検証を通過した asset のみを staged binary として採用する。

## Test cases
| TC ID | Level | Purpose | Related SP |
| --- | --- | --- | --- |
| TC-001 | unit | `FileList.txt` / `filelist.txt` の優先検出 | SP-001 |
| TC-002 | unit | リスト未存在時に walker 走査で候補生成 | SP-002 |
| TC-003 | unit | ファジー検索が関連度順・件数制限を満たす | SP-003 |
| TC-003A | unit | 通常語の各 token は subsequence-only よりリテラル一致を高順位にする | SP-003 |
| TC-004 | unit | 実行可能ファイルで execute 分岐 | SP-004 |
| TC-004A | unit | open / execute helper seam が open/execute handler を適切に呼び分ける | SP-004, SP-005 |
| TC-005 | unit | フォルダで open 分岐 | SP-005 |
| TC-006 | integration | CLI 契約（`--cli`/`--root`/`--limit`）を満たす | SP-006 |
| TC-006A | integration | CLI の `--limit` は 1000 件へ暗黙丸めせず、そのまま実効上限として扱う | SP-006 |
| TC-007 | perf | 10万件検索の遅延計測 | SP-007 |
| TC-008 | unit | 例外時に非ゼロ終了コードを返す | SP-008 |
| TC-009 | unit | モジュール分離により単体テスト可能 | SP-009 |
| TC-010 | manual+unit | GUI で検索/選択/実行/再読込が行える | SP-010 |
| TC-011 | manual | GUI 回帰手順に基づく再検証が可能 | SP-011 |
| TC-012 | unit | プレビューキャッシュが上限件数を超えない | SP-007 |
| TC-012A | unit | `.vbs` など非 `.txt` でも UTF-8 / UTF-16 / 主要レガシー文字コードなら本文プレビューでき、バイナリは unreadable を維持する | SP-010 |
| TC-013 | manual | インデックス中に空クエリ一覧が逐次更新され、操作が継続できる | SP-002, SP-007 |
| TC-014 | unit | FileList 作成の成功/失敗応答で GUI 状態遷移が正しい | SP-010, SP-008 |
| TC-015 | integration | CLI 非空クエリ出力形式と root 不正系を検証する | SP-006, SP-008 |
| TC-016 | unit | Root 変更時に旧 Root 由来の選択状態（current/pinned/preview）がクリアされる | SP-010 |
| TC-017 | unit | Root 変更時に旧 Root 向け FileList 上書き確認が破棄される | SP-010 |
| TC-103 | unit | Root 変更時に旧 Root 向け FileList の祖先追記確認と Walker 利用確認が破棄される | SP-001, SP-010 |
| TC-018 | unit | `Ctrl+Shift+C`（macOS では `Cmd+Shift+C`）は検索窓フォーカス時でも結果パスを優先コピーする | SP-010 |
| TC-019 | unit | 大規模 FileList 解析はストリーミングで中断可能（新 request で supersede） | SP-001, SP-007 |
| TC-020 | manual+unit | Windows: 異解像度モニタ跨ぎ時に復元サイズが破綻せず、IME `Space` / `Shift+Space` で最低限の半角スペースが検索窓へ反映される（`CompositionUpdate` 同フレームでも挿入される） | SP-010, SP-011 |
| TC-021 | unit | 検索窓フォーカス中でも `Ctrl+N` / `Ctrl+P` が current row を移動する | SP-010 |
| TC-022 | unit | 検索窓フォーカス中でも `Ctrl+G` / `Esc` で query clear + filter reset が実行される | SP-010 |
| TC-023 | unit | `Tab` / `Shift+Tab` はフォーカス非依存で PIN 固定/解除のみ実行し current row を維持する | SP-010 |
| TC-024 | unit | IME スペースフォールバックと composition commit fallback はカーソル位置へ挿入し、挿入後カーソルへ更新する | SP-010 |
| TC-025 | unit | FileList ストリーミング時に種別不明候補を先行表示し、種別解決後に FILE/DIR/LINK 表示とフィルタ状態を反映する | SP-001, SP-010, SP-007 |
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
| TC-104 | unit | render command 経由の tab close / reorder は active tab identity と queue cleanup を壊さない | SP-010 |
| TC-094 | unit | タブ accent color は保存・復元時に維持される | SP-010 |
| TC-095 | unit | tab accent palette は light/dark theme で DropSendTo の Slot Color と同じ RGB を返す | SP-010 |
| TC-047 | unit | FileList 作成時は祖先の既存 FileList へ子 FileList 参照を重複なく追記し、親 mtime を維持する | SP-001 |
| TC-048 | unit | 祖先探索は callback/失敗時に即停止し、それ以降の上位階層を処理しない | SP-001 |
| TC-049 | unit | FileList に root 外パスが含まれても候補表示時の追加フィルタを行わず、インデクシング経路の挙動を維持する | SP-007 |
| TC-050 | unit | root 外パスは結果一覧に存在しても execute/open 直前で拒否される | SP-004 |
| TC-051 | unit | UNC root 配下のパスは execute/open 直前の root 判定で許可される | SP-004 |
| TC-052 | unit | Create File List で祖先 FileList 追記がありうる場合は確認ダイアログを要求する | SP-001, SP-010 |
| TC-053 | unit | 祖先追記確認を拒否した場合、root 直下の FileList 作成だけを継続する | SP-001, SP-010 |
| TC-084 | unit | Source が FileList のタブで Create File List を確認すると、新規タブを開かずに同一タブの裏で Walker indexing を実行する | SP-001, SP-010 |
| TC-085 | unit | FileList source の Create File List 完了時、元タブが background 化していてもその元タブを再インデックスし、完了前に元タブ root が変わっていた場合は再インデックスしない | SP-001, SP-010 |
| TC-087 | unit | Create File List の保留状態は status panel または確認ダイアログ経由でキャンセルできる | SP-001, SP-010 |
| TC-088 | unit | Create File List 実行中のキャンセル要求は cancel flag を立て、`Canceled` 応答で状態と notice を解放する | SP-001, SP-010 |
| TC-089 | unit | Create File List の cancel 済み request は root 直下の既存 `FileList.txt` を置換しない | SP-001 |
| TC-082 | unit+perf | 回帰: FileList の `\` 区切り候補を filesystem existence probe なしでプラットフォーム優先解釈し、v0.12.3 の解析速度基準から 15% 以内に維持する | SP-001, SP-007 |
| TC-083 | unit+perf | Walker 初期インデクシングは通常ファイル/ディレクトリを `file_type` ベースで流し、eager metadata 解決より高速に `Finished` へ到達し、その後に遅延種別解決を自動開始する | SP-002, SP-007 |
| TC-054 | unit | `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` のとき query history を保存も復元も行わない | SP-010 |
| TC-055 | manual | README / release docs / release template に平文 history 保存、checksum 検証、notarization の暫定運用と `Security` / `Known issues` 記載前提が明記されている | SP-010, SP-012 |
| TC-056 | integration | CI は Linux/macOS/Windows を対象にし、`cargo audit` を実行する | SP-012 |
| TC-057 | unit | `Score` / `Name` / `Modified` / `Created` のソートモード遷移と `Score` 復帰を検証する | SP-013 |
| TC-058 | unit | query 変更や結果更新時にソートを破棄し、インデクシング経路へ属性取得を追加しない | SP-013 |
| TC-059 | unit | 日付ソートの未キャッシュ path は別 worker へ要求し、古い応答は request_id で破棄する | SP-013 |
| TC-060 | unit | sort metadata cache は上限を超えて増加せず、古い項目を破棄する | SP-013 |
| TC-061 | unit | `created()` 取得不可の項目は `Created` ソート時に末尾へ送る | SP-013 |
| TC-062 | manual | 結果ペインの `Sort` UI から各ソートを選択でき、`Modified` / `Created` 中も入力が継続できる | SP-010, SP-013 |
| TC-063 | perf | ソート機能追加後もインデクシング時の属性取得数を増やさず、既存の逐次表示挙動を維持する | SP-007, SP-013 |
| TC-090 | unit | 回帰: GNU Windows ビルドは `resource.o` を最終 `flistwalker` バイナリへ明示リンクし、Explorer アイコン欠落を防ぐ | SP-012 |
| TC-091 | unit | 回帰: GUI 終了時の worker join timeout は短時間に保たれ、close が不要に 2 秒近く遅延しない | SP-008, SP-010 |
| TC-092 | unit | 回帰: shared query module により search と highlight が exact / OR / anchor の query 解釈を一致させる | SP-003, SP-010 |
| TC-093 | unit | regex モードでも plain token / plain OR token は fuzzy 条件で一致し、regex 構文 token だけ regex として評価される | SP-003 |
| TC-068 | unit | 回帰: 検索結果更新で current row は行番号を維持し、結果数縮小時のみ末尾へ丸める | SP-010 |
| TC-069 | unit | 回帰: `Ignore Case` は既定で有効で、無効化時は検索結果とハイライトが case-sensitive になる | SP-010 |
| TC-070 | unit | 回帰: 起動直後と検索キャンセル後は候補がある場合に 1 行目が既定選択になる | SP-010 |
| TC-071 | unit | 大規模候補でも partial top-N の結果が full ranking の先頭と一致する | SP-003, SP-007 |
| TC-072 | unit | 並列検索の収集結果は逐次検索と同じ ranking を返す | SP-003, SP-007 |
| TC-073 | unit | 非アクティブタブの結果キャッシュ compact 後も、再表示時に current row と結果一覧を復元できる | SP-010 |
| TC-100 | unit | self-update candidate 解決は release asset 選択と support classification を分離し、manual-only fallback を契約として保持する | SP-014 |
| TC-101 | unit | update request / install transitions emit trace commands for supportability and retain request_id correlation | SP-014 |
| TC-102 | unit | Create File List の stale requested root completion は cleanup だけを行い、`use_filelist` 復帰や notice 更新を行わない | SP-001, SP-010 |
| TC-105 | unit | root dropdown selection は popup を閉じて選択 root を適用し、後続の root cleanup flow を開始できる | SP-010 |
| TC-106 | unit | background response routing は active tab を巻き戻さず、tab close 時は request routing cleanup だけを行う | SP-010 |
| TC-074 | unit | GitHub Releases の latest 応答から現在 platform の更新 asset と sidecar 文書（README / LICENSE / THIRD_PARTY_NOTICES）と `SHA256SUMS` を選択できる | SP-014 |
| TC-075 | unit | staged binary と sidecar 文書は `SHA256SUMS.sig` の署名検証と `SHA256SUMS` の checksum 検証を通過した場合のみ自己更新へ進む | SP-014 |
| TC-076 | unit | Windows の自己更新は補助 updater 経由で target EXE 置換コマンドを生成し、実行中 EXE の直接上書きを避ける | SP-014 |
| TC-077 | unit | macOS は新版検知時も自動置換へ進まず、手動更新案内に留める | SP-014 |
| TC-078 | unit | 更新確認/ダウンロード失敗は notice に反映されても GUI 操作を継続できる | SP-014 |
| TC-079 | manual+unit | 手動試験 override により同一 version でも更新ダイアログを表示できる | SP-014 |
| TC-080 | manual+unit | 手動試験 override により downgrade 候補でも更新ダイアログを表示できる | SP-014 |
| TC-081 | unit | 更新ダイアログで抑止した target version は起動間で保持され、より新しい version が出るまで再表示されない | SP-014 |
| TC-096 | unit | 起動時更新確認失敗は失敗ダイアログへ反映され、通常操作継続のため pending/in_progress を解放する | SP-014 |
| TC-097 | unit | 起動時更新確認失敗ダイアログの抑止設定は UI state に保存され、抑止中は失敗ダイアログを再表示しない | SP-014 |
| TC-098 | unit | `FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE` はネットワーク非依存で起動時更新確認失敗を強制し、失敗ダイアログ抑止中でも表示できる | SP-014 |
| TC-093 | unit | `FLISTWALKER_DISABLE_SELF_UPDATE=1` では起動時更新確認を送らず、更新チェックも `None` を返して自己更新を無効化する | SP-014 |
| TC-086 | unit | 実行バイナリと同一ディレクトリに `FLISTWALKER_DISABLE_SELF_UPDATE` ファイルがある場合も、自己更新を無効化する | SP-014 |
| TC-094 | unit | Windows の一般 `.ps1` は Execute ではなく Open 分岐を選ぶ | SP-004 |
| TC-095 | unit | `SHA256SUMS.sig` は改ざんされた manifest を検証失敗にする | SP-014 |
| TC-099 | manual | structural refactoring 後も GUI の主要操作が連続利用に耐える | SP-010 |
| TC-100 | manual | diagnostics trace smoke: `RUST_LOG` と `FLISTWALKER_WINDOW_TRACE` の責務分離を維持し、対象 flow が canonical event family と相関 field を出す | SP-010, SP-014 |
| TC-101 | unit | Windows regression: `copy_selected_paths` の notice 正規化は `app.shell.runtime.notice` を更新し、旧 shell 直参照の残骸を検出する | SP-010 |

## Regression Guard
- 発生条件: 検索結果の更新時に 100 行目へカーソルがある状態で結果数が 100 未満へ減る、または current row が未選択のまま再検索が走る。
- 期待動作: current row はユーザ操作なしで別の行へ移動せず、保持できる場合は同じ行番号を維持し、縮小した場合のみ末尾へ丸める。未選択状態は自動選択に変換しない。
- 非対象範囲: 手動の Arrow キー移動、Sort 切替、Root 変更による既存 selection 破棄。
- 関連テストID: TC-068.
- 発生条件: `copy_selected_paths` の Windows-only テストで、`FlistWalkerApp` の旧 `notice` 直参照が残る。
- 期待動作: notice は live runtime の `app.shell.runtime.notice` を参照し、`\\?\` 付きの extended prefix を正規化した結果だけを検証する。
- 非対象範囲: copy パス実装そのものの出力形式変更、Windows 以外の OS の path normalization。
- 関連テストID: TC-101.

## Runner and commands
- Runner: `cargo test`
- Runner: `cargo test`, `cargo audit`
- Validation Matrix:
| Change Type | Typical Targets | Required Validation | Optional / Follow-up |
| --- | --- | --- | --- |
| VM-001 Docs only | `docs/*.md`, `AGENTS.md`, release note text only | affected doc diff review, `rg` で ID/参照整合を確認 | Rust 実装に触れない限り `cargo test` は不要 |
| VM-002 App/UI orchestration | `rust/src/app/mod.rs`, `rust/src/app/*.rs` の state/render/input/session/update/filelist/tab_state/tabs/bootstrap/cache 変更 | `cd rust && cargo test` | dialog / focus / tab 操作を変えた場合は GUI 手動試験。検索結果描画や入力応答性を変えた場合は、非空 query で検索窓の左右移動・Backspace・結果スクロールの体感遅延を手動確認。タブ描画変更時は light/dark theme で active full-fill / inactive 下辺装飾 / 右クリック色変更を手動確認。routing / lifecycle を触った場合は `session_tabs.rs` と `index_pipeline/filelist_lifecycle.rs` の owner regression を追加確認する。window trace の observable output を変えた場合は TC-100 の focused smoke を追加実施する |
| VM-003 Indexing path | `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/walker.rs`, `rust/src/indexer/filelist_writer.rs`, `rust/src/app/index_worker.rs`, `rust/src/app/workers.rs`, `rust/src/app/mod.rs`, `rust/src/app/pipeline.rs` の index/filelist/walker 経路 | `cd rust && cargo test`; `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`; `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture` | 大規模 root で GUI 手動試験。worker/index trace の observable output を変えた場合は TC-100 の focused smoke を追加実施する |
| VM-004 Search/query contract | `rust/src/query.rs`, `rust/src/search/mod.rs`, `rust/src/search/cache.rs`, `rust/src/search/config.rs`, `rust/src/search/execute.rs`, `rust/src/search/rank.rs`, `rust/src/ui_model.rs`, highlight / sort 契約変更 | `cd rust && cargo test` | 主要 query (`'`, `!`, `^`, `$`, `|`) の GUI 手動試験 |
| VM-005 CLI / build / release / updater | `rust/src/main.rs`, `rust/build.rs`, `rust/src/updater.rs`, `scripts/build-rust-*.sh`, `.github/workflows/*`, `docs/RELEASE.md` | `cd rust && cargo test` | release/update 導線や platform 資産を変えた場合は該当 manual test と release doc review。workflow 変更時は tag workflow の preflight 条件、Windows native test、Windows GNU cross build、`cargo audit`、perf regression workflow の役割分担も確認する |
- 大規模 docs cleanup や plan 撤去のような docs-only 変更では、doc diff review と `rg` 参照整合確認を必須にする。Rust 実装に触れない限り `cargo test` は不要だが、変更対象が docs と `AGENTS.md` に限定されることを `git diff --stat` でも確認する。
- app architecture のような構造改善後も、恒久的な検証基準は VM-001 / VM-002 / VM-003 を直接適用する。temporary slice 固有の validation rule はこの文書へ持ち込まない。
- Commands:
- `cd rust`
- `source ~/.cargo/env`
- `cargo test`
- `cargo audit`
- heavy perf regression workflow: `.github/workflows/perf-regression.yml` の manual dispatch または schedule
- lightweight PR perf gate: `.github/workflows/ci-cross-platform.yml` の linux-native job で `perf_regression_filelist_stream_matches_v0123_reference_budget` を実行
- GUI 手動試験: `cargo run -- --root .. --limit 1000`
- GUI 手動試験: `cargo run -- --root .. --limit 1000` で新版検知ダイアログと更新承認導線を確認
- GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode SameVersion`
  Windows sandbox で同一 version の feed でも更新ダイアログ表示を確認する。helper は `SHA256SUMS.sig` を同時生成する。
- GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode Downgrade`
  Windows sandbox で旧 version feed を使った downgrade ダイアログ表示を確認する。helper は `SHA256SUMS.sig` を同時生成する。
- GUI 手動試験:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\manual-self-update-test.ps1 -Mode Custom -FeedVersion 0.12.1`
  Windows sandbox で任意 version のローカル feed を生成し、署名付き manifest を使った update 手順を再現する。
- CLI 動作確認: `cargo run -- --cli "main" --root .. --limit 20`

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
`FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE=1 cargo run -- --root .. --limit 1000` で起動し、ネットワーク状態に関係なく `Update Check Failed` ダイアログが表示されることを確認する。必要なら `FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE=\"debug message\"` で表示文言を固定する。

## Diagnostics Trace Smoke Test
1. worker-side trace:
`RUST_LOG=flist_walker::app::index_worker=info cargo test index_worker_trace_smoke_emits_canonical_fields --lib -- --nocapture` を実行し、Started/Finished 応答が通ることに加えて、出力ログに `flow=index`、`event=started|finished|completed`、`request_id=`、`source_kind=walker` が含まれることを確認する。
2. window trace:
window/session/input/update の observable output を変更した場合だけ、`FLISTWALKER_WINDOW_TRACE=1 FLISTWALKER_WINDOW_TRACE_PATH=<temp-path> cargo run -- --root .. --limit 1000` または対象 GUI 操作に相当する手順を実施し、変更対象 family の event が trace file に出ることを確認する。window trace に変更がない場合は `not needed` と記録する。

## Structural Refactoring GUI Smoke Test
Phase 9 の structural refactoring 完了判定では、この手順を別環境で 1 回通せば GUI smoke 合格とみなす。

1. 起動と初期表示
`cargo run -- --root .. --limit 1000` で起動し、一覧が表示されること。候補がある場合は 1 行目が選択され、入力欄へすぐ打鍵できることを確認する。
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
- 発生条件: `b8e3321` 以降、include_files/include_dirs 両有効の FileList ストリーム解析で各行に `exists()` 相当の確認が入り、Windows 由来 `\` 区切り FileList のインデクシングが目に見えて遅くなる。
- 期待動作: `\` / `/` の差異は候補順序だけで吸収し、初期ストリームは v0.12.3 相当の line-only 解析速度を維持する。
- 非対象範囲: include_files / include_dirs のいずれか片方のみ有効な場合の種別判定、root 外候補の実行拒否、階層 FileList の新旧判定。
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
- TC-074 -> SP-014 -> DES-014 -> FR-019
- TC-075 -> SP-014 -> DES-014 -> FR-020
- TC-076 -> SP-014 -> DES-014 -> FR-021
- TC-077 -> SP-014 -> DES-014, DES-007 -> FR-023
- TC-078 -> SP-014 -> DES-006, DES-014 -> NFR-007
- TC-081 -> SP-014 -> DES-014 -> FR-024
- TC-086 -> SP-014 -> DES-014 -> FR-019, FR-020
- TC-093 -> SP-014 -> DES-014 -> FR-019, FR-020
- TC-094 -> SP-004 -> DES-004 -> FR-022
- TC-095 -> SP-014 -> DES-014 -> FR-020, FR-023
- TC-100 -> SP-010, SP-014 -> DES-015, DES-014 -> FR-007, FR-019
