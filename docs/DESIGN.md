# DESIGN

## Architecture overview
- DES-001 Index Source Resolver
- 役割: FileList 優先モード有効時に `FileList.txt`/`filelist.txt`（`filelist.txt` の大小違い含む）を検出して優先読み込み。
- 実装: `rust/src/indexer.rs`

- DES-002 Walker Indexer
- 役割: FileList 未使用時の再帰走査。
- 実装: `rust/src/indexer.rs`

- DES-003 Fuzzy Search Engine
- 役割: クエリ解釈（`'` `!` `^` `$` `|`）とスコアリングを担う。query 分解と正規化は shared module へ集約し、非 regex の `^`/`$` は隣接文字制約付きファジーとして評価する。
- 役割補足: 空白で分割した通常語 token は AND 条件で絞り込みつつ、score では token ごとのリテラル一致を subsequence 一致より優先して順位付けする。
- 役割補足: GUI/CLI の `Ignore Case` フラグを受け取り、search と highlight で同じ比較モードを使う。
- 役割補足: 検索クエリは要求単位で前処理し、候補ごとの path 文字列化・正規化を 1 回に抑える。大規模候補集合では search worker 内で並列評価しつつ、表示用には上位 `limit` 件だけを抽出する。
- 実装: `rust/src/query.rs`, `rust/src/search.rs`

- DES-004 Action Executor
- 役割: ファイル実行/オープン、フォルダオープンを OS 差分吸収して実行。
- 実装: `rust/src/actions.rs`

- DES-005 CLI Adapter
- 役割: `clap` 引数を受け取り CLI 出力へ変換。
- 実装: `rust/src/main.rs`

- DES-009 GUI Adapter (egui/eframe)
- 役割: 検索入力、結果表示、プレビュー、複数選択と一括操作を提供。結果ハイライトは search と同じ query 解釈を shared module 経由で使用する。結果スナップショット更新時は current row を行番号ベースで維持し、結果数が減った場合のみ末尾へ丸める。
- 役割補足: 起動直後と `Ctrl+G` / `Esc` の検索キャンセル後は、候補が存在する場合に 1 行目を既定選択として復帰させる。
- 役割補足: 検索オプションの `Ignore Case` を既定有効で保持し、無効時は検索結果とハイライトを case-sensitive に切り替える。
- 実装: `rust/src/app.rs`, `rust/src/ui_model.rs`, `rust/src/query.rs`

- DES-010 GUI Test Artifacts
- 役割: GUI 回帰手順と結果を管理する。
- 実装: `docs/TESTPLAN.md` の GUI 手順節

- DES-012 CI / Release Hygiene
- 役割: release 対象 OS の CI 継続検証、依存脆弱性検査、notarization 運用の文書化。
- 実装: `.github/workflows/ci-cross-platform.yml`, `docs/RELEASE.md`, `.github/release-template.md`, `rust/build.rs`

- DES-013 Result Sort Controller
- 役割: 結果スナップショット限定のソート、日付属性の遅延取得、上限付き属性キャッシュを管理。
- 実装: `rust/src/app.rs`, `rust/src/app/render.rs`, `rust/src/app/workers.rs`

- DES-014 Self Update Coordinator
- 役割: GitHub Releases の最新 version 確認、対象 asset 選択、`SHA256SUMS.sig` と `SHA256SUMS` の検証、Windows/Linux 向け staged update と再起動を制御する。
- 実装: `rust/src/updater.rs`, `rust/src/app.rs`, `rust/src/app/render.rs`, `rust/src/app/workers.rs`

## Main flows
- Flow-001: 起動 -> （FileList 優先モード有効時）FileList 検出 -> 読み込み -> 検索 -> 選択 -> アクション。
- Flow-002: 起動 -> FileList なし -> walker 走査 -> 検索 -> 選択 -> アクション。
- Flow-003: アクション失敗 -> エラー整形 -> 表示 -> 非ゼロ終了（CLI）/エラー通知（GUI）。
- Flow-004: GUI 起動 -> 非同期インデックス -> 最新要求優先検索（古い要求を破棄） -> プレビュー -> 実行/オープン。
- Flow-005: GUI 起動 -> update worker が GitHub Releases を確認 -> 新版あり -> 利用者承認 -> asset と `SHA256SUMS.sig` / `SHA256SUMS` を取得 -> 署名検証 -> checksum 検証 -> 補助 updater 起動 -> 本体終了 -> 置換後に新版本体を再起動。
  `FLISTWALKER_DISABLE_SELF_UPDATE=1` の場合は update flow を起動せず、通常起動のみ行う。

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
- GUI の逐次反映は2系統とする: 空クエリはインデックス蓄積分を即時表示、非空クエリは一定件数/時間の閾値を満たしたときだけ検索用スナップショットを更新する。
- FileList 解析はストリーミングで処理し、`Started` を先行通知した後にバッチ反映する。大規模 FileList でも `Source: None` 固定を避け、新しい request_id で中断可能にする。
- 階層 FileList 展開は全ディレクトリ走査ではなく、読み込み済み候補から `FileList.txt` / `filelist.txt` の完全一致エントリを抽出して判定する。
- 階層 FileList 展開で子 FileList を解析する経路も `should_cancel` を伝播し、supersede 時に中断できるようにする。
- include_files/include_dirs が両方有効な FileList 解析では、初期ロード時の `metadata` 依存を避けて候補パスの投入を優先し、FILE/DIR/LINK 表示種別は別ワーカーで遅延解決する。
- regex モードは include term をクエリ単位で事前コンパイルし、候補ごとの再コンパイルを禁止する。
- プレビューキャッシュは固定上限（FIFO）で運用し、長時間セッションでのメモリ増加を抑制する。
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
- `Ctrl+Shift+C`（macOS では `Cmd+Shift+C`）は TextEdit の既定コピー処理より後段で実行し、検索窓フォーカス中でも選択パスコピーを優先する。
- Windows のプレビュー抑止判定は属性ビットだけに依存せず、`FileAttributeTagInfo` と `CfGetPlaceholderStateFromAttributeTag` を使って Cloud Files API 準拠 placeholder を検出する。属性/タグ取得に失敗した場合のみ既存の属性ビット判定へフォールバックする。
- query 履歴はアプリ共通 state として保持し、全タブから同じ履歴集合を参照できるようにする。
- query 履歴保存は入力経路から独立して管理し、TextEdit / IME フォールバック / Emacs 風編集のどの入力経路でも「一定時間の無入力」または `Results` 移動開始時に最終 query だけを記録する。
- IME 合成中は履歴確定を抑止し、`CompositionEnd` 後に反映された確定文字列のみが履歴候補になるようにする。
- `Ctrl+R` は履歴検索モードを開始し、同じ検索欄を履歴検索入力へ切り替える。履歴検索中は `Enter` / `Ctrl+J` / `Ctrl+M` で選択中履歴を query へ展開し、`Esc` / `Ctrl+G` で開始前 query を復元してキャンセルする。
- query 履歴は通常終了時の UI state に最大 100 件まで永続化し、次回起動時に後方互換を保って復元する。
- `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` のときは、UI state 読み書き時に query history フィールドを空として扱い、履歴の永続化だけを無効にする。
- 結果ソート状態はタブ単位で保持するが、query 変更や結果スナップショット更新時には `Score` へ戻し、保留中の sort request_id を無効化する。
- 結果ペイン上部に `Sort` ドロップダウンを配置し、`Score` / `Name (A-Z)` / `Name (Z-A)` / `Modified (New)` / `Modified (Old)` / `Created (New)` / `Created (Old)` を選択可能にする。
- `Created` 属性は取得失敗を正常系として扱い、notice ではなく並び順の末尾送りだけで吸収する。
- タブ復元は `FLISTWALKER_RESTORE_TABS=1` のときだけ有効化し、永続化対象は `root/query/use_filelist/use_regex/include_files/include_dirs/active_tab` に限定する。
- 起動時の優先順位は `--root` 明示 > 復元タブ（env 有効時） > 最後に使っていた root > `Set as default` > 通常 root とし、バージョン更新やバイナリ差し替えでも最後の root を維持する。
- `FLISTWALKER_RESTORE_TABS=1` が有効な間は root 行の `Set as default` ボタンを disabled 表示にし、ロジック側でも no-op + notice で排他を強制する。
- タブ復元時は active tab だけ即時 `request_index_refresh()` を行い、background tab は `pending_restore_refresh` を保持して初回 `switch_to_tab_index()` 時に lazy refresh する。
- タブ並び替えは `render_tab_bar` がドラッグ開始/ホバー/ドロップを扱い、実際の `Vec<AppTabState>` 更新は `move_tab(from, to)` に集約する。
- `move_tab` は並び替え前に `sync_active_tab_state()` を実行し、active tab の `tab.id` を基準に移動後 index を再解決することで UI 状態の取り違えを防ぐ。
- Root 変更時は query 自体を維持しつつ、履歴参照位置と draft query のみ破棄して root 跨ぎの戻り操作を防ぐ。
- 検索窓フォーカス中でも `ArrowUp` / `ArrowDown` / `Ctrl+I` / `Ctrl+J` / `Ctrl+M` はアプリ側ショートカットを優先処理し、結果移動・PIN トグル・実行を抑止しない。
- Windows の一般 `.ps1` は検索結果からの既定操作では直接実行せず、既定アプリでオープンする。自己更新用の内部 PowerShell script は updater モジュールからのみ起動する。
- 自己更新は `SHA256SUMS.sig` を埋め込み公開鍵で検証してから `SHA256SUMS` を信頼し、staged binary の checksum 検証へ進む。検証失敗時は既存バイナリと UI セッションを維持する。
- Windows の自己更新は実行中 EXE とは別実体の PowerShell スクリプトを一時配置して `powershell.exe -ExecutionPolicy Bypass -WindowStyle Hidden` で非表示起動し、`Copy-Item -LiteralPath` ベースで置換する。
- Linux の自己更新は一時 shell script を起動して本体終了を待ち、`cp` + `chmod` 後に新 binary を再起動する。
- macOS は最新 version 検知のみ実施し、自動適用は非対応として release URL への案内に留める。

- DES-008 Testability
- indexer/search/actions/ui_model を独立モジュール化。
- query 解釈は `rust/src/query.rs` へ集約し、search と UI highlight で同じ token 分解・正規化を再利用する。
- OS 依存処理は抽象境界を薄くして単体テスト可能性を維持。

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
- asset 選択は release asset 命名規則から current platform/arch と一致する standalone binary と `SHA256SUMS` / `SHA256SUMS.sig` を選択する。
- update worker は check/download を担当し、GUI 側は dialog 表示と再起動要求だけを扱う。
- 更新署名公開鍵はビルド時環境変数から埋め込み、未設定ビルドでは Windows/Linux でも update candidate を manual-only に落として自動更新不能を明示する。
- restart 時は現在 executable path を置換対象とし、起動引数は最小化して通常 GUI 起動へ戻す。セッション復元は既存 UI state に委譲する。
- update dialog は `skip until next version` のチェック状態を持ち、Later 選択時に current target version を UI state へ永続化する。
- 起動時の update 応答は保存済み `skipped_update_target_version` と semver 比較し、target version がそれ以下なら dialog を出さず、より新しい version のみ再通知する。
- `FLISTWALKER_DISABLE_SELF_UPDATE` が truthy な場合は GUI 側で起動時 update request 自体を送らず、update install 側でも同じ環境変数で最終ガードする。
- 手動試験用 override として `FLISTWALKER_UPDATE_FEED_URL`, `FLISTWALKER_UPDATE_ALLOW_SAME_VERSION=1`, `FLISTWALKER_UPDATE_ALLOW_DOWNGRADE=1` を読み取り、通常運用の GitHub latest 比較を一時的に差し替えられるようにする。

## Error handling / timeout / logging / metrics
- エラー戦略: ファイルアクセス失敗、実行失敗、正規表現不正を分類して表示。
- タイムアウト: 外部プロセス起動はブロッキング待機しない。
- ログ: 現状は標準出力/標準エラー中心。必要に応じて構造化ログへ拡張。
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
- DES-003 -> TC-067 (SP-003, SP-010)
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
