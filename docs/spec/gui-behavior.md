# GUI Behavior Specification

## SP-010 GUI 操作仕様
### Requirements
- MUST: 検索入力、結果リスト、プレビューペイン、実行/オープンを提供する。
- MUST: Source（FileList/Walker）と Root を画面表示する。
- MUST: フッター右端に現在 version を常時表示する。
- MUST: 非マッチは非表示とし、一致箇所ハイライトを提供する。
- MUST: ハイライトは search と同じ query interpretation を用い、検索結果と表示が一致するようにする。
- MUST: 表示中の Results が空なら current row は `None`、1件以上なら常に範囲内の `Some(row)` とする。検索・sort・filter・preset・tab/session restore・非同期応答による結果再適用では、従来の行番号を結果末尾へ丸め、従来行がなければ0行目を選択する。非active tabで結果配列をcompactionした間だけ、再表示用のbase resultsに対するselection保持を許す。
- MUST: GUI 起動直後および `Ctrl+G` / `Esc` による検索キャンセル後も、上記 Results/current row invariantを次の描画までに満たす。
- MUST: 複数選択と一括アクションを提供する。
- MUST: Windows では on-demand placeholder と判定できるファイルの本文プレビューを行わず、取得系 I/O による意図しないダウンロードを避ける。
- MUST: 本文プレビューは拡張子で制限せず、UTF-8、BOM 付き UTF-16、および主要 OS で一般的なレガシー文字コードを順に解釈して、テキストとして安全に復号できた内容を表示する。
- MUST: ファイルの本文プレビューには byte size を `Size:` として人間可読単位で表示する。サイズ取得を含む preview I/O は worker で実行し、UI thread をブロックしてはならない。フォルダサイズの再帰計算は行わない。
- MUST: directory previewは直接の子の先頭4096項目と最大1項目のlookaheadに列挙を制限し、sample内を名前順にして最大24行を表示する。上限超過は件数の下限とlisting truncatedを表示し、読取errorはpartial listingとして空directoryと区別する。GUI/TUIの新しいpreview要求と終了要求はfilesystem呼出し間で古い処理を中断し、古い本文を公開しない。GUIの取消・drain破棄は要求ID付きterminalでownerのbusy/routingを解放し、非active tabは必要なpreviewを復帰時に再要求する。
- MUST: Preview は取得できる場合に `Updated:`（filesystem の `modified()`）を表示し、`Created:` は filesystem の `created()` が利用できる場合に表示する。日時は `YYYY-MM-DD HH:mm UTC` 形式とする。
- SHOULD: Preview は `Read-only` / Windows の `Hidden` 属性を `Attributes:` に表示し、シンボリックリンクにはリンク先を `Target:` として表示する。属性またはリンク先を取得できない場合も本文プレビュー全体を失敗させてはならない。
- MUST: シンボリックリンクでは、本文・`Target Size:`・`Target Created:`・`Target Updated:` はリンク先を対象とし、`Attributes:` はリンク自身を対象とする。リンク先 metadata を取得できない行は省略する。
- SHOULD: FileList 読み込み直後の未解決候補は背景解決により FILE/DIR/LINK 表示を後追い更新できる。
- MUST: runtime config の `emacs_keybindings_enabled` が `true` のとき、`Ctrl+N` / `Ctrl+P` / `Ctrl+G` / `Esc` は検索窓フォーカス中でも有効である。
- MUST: `emacs_keybindings_enabled=true` のとき、GUI application command の `次へ` / `前へ` / `確定` / `キャンセル` は共有 semantic mapping でそれぞれ `Ctrl+N` / `Ctrl+P` / `Ctrl+J` または `Ctrl+M` / `Ctrl+G` を受理しなければならない。この mapping はメイン結果一覧、履歴、root dropdown、preset picker、Named Root manager、および通常の `Enter` / `Esc` を持つ確認・通知 modal に適用し、新しい modal/overlay が feature ごとの Emacs 判定を追加しなければ利用できない構成にしてはならない。対話面が対応する通常 command を提供しない場合は、新しい command を Emacs chord だけに追加してはならない。
- MUST: `emacs_keybindings_enabled=true` のとき、GUI が所有する全単一行入力は共有 text-editing adapter を使用し、`Ctrl+A` / `Ctrl+E` の先頭・末尾移動、`Ctrl+B` / `Ctrl+F` の1文字移動、`Ctrl+H` / `Ctrl+D` の前方・後方1文字削除、`Ctrl+K` / `Ctrl+U` の末尾・先頭までのkill、`Ctrl+Y` のyankを Unicode character index で処理しなければならない。対象には通常検索、履歴フィルター、preset filter、preset editor の name/root/query、Named Root editor の name/path、保存 root 管理の追加・編集欄を含む。`Ctrl+H` が backend で Backspace に変換済みの場合は二重削除してはならず、IME composition 中は独自 reducer を適用してはならない。`Ctrl+W` は `ctrl_w_deletes_word_in_query` の既存契約に従う。
- MUST: runtime config の `emacs_keybindings_enabled` が `false` のとき、Emacs 風の `Ctrl+N` / `Ctrl+P` / `Ctrl+V` / `Alt+V` / `Ctrl+G` / `Ctrl+R` / `Ctrl+I` / `Ctrl+J` / `Ctrl+M` および検索欄編集用 `Ctrl+A` / `Ctrl+E` / `Ctrl+B` / `Ctrl+F` / `Ctrl+H` / `Ctrl+D` / `Ctrl+W` / `Ctrl+K` / `Ctrl+Y` / `Ctrl+U` はアプリ固有 command / reducer を起動してはならず、GUI backend が同 chord を既定編集として持つ場合も application-owned input を変化させてはならない。
- MUST: runtime config の `ctrl_w_deletes_word_in_query` は既定 `false` とする。GUI は `emacs_keybindings_enabled=true` かつ同設定が `true` で、通常検索欄または履歴検索フィルターへフォーカス中の場合、IME 合成中を除いて `Ctrl+W` を Unicode character index に基づく直前単語の削除として描画前に一度だけ処理し、タブ終了へ流してはならない。IME 合成中の `Ctrl+W` は単語削除にも Windows/Linux のタブ終了にも流してはならない。検索欄外、Emacs 無効時、または同設定が `false` の場合は Windows/Linux の `Ctrl+W` と macOS の `Cmd+W` による従来のタブ終了を維持する。macOS では設定有効時も `Cmd+W` をタブ終了として維持する。TUI の `Ctrl+W` 単語削除はタブ競合がないため同設定へ依存させてはならない。
- MUST: GUI は top action の `Help` と `F1` の両方から、キーボードショートカットと query syntax を示すモーダルヘルプを開き、`F1` / `Esc` / `Close`、および Emacs 設定有効時の `Ctrl+G` で閉じられなければならない。
- MUST: GUI ヘルプは macOS では primary modifier を `Cmd`、その他の OS では `Ctrl` と表示し、runtime config の `emacs_keybindings_enabled` に応じて Emacs 風ショートカット一覧または無効状態を表示しなければならない。query syntax は非field term、`name:`、`path:`、`dir:`、`ext:` の対象と、fieldへ適用できる `'`、`!`、`^`、`$`、`|` および複合例を表示しなければならない。
- MUST: GUI ヘルプ表示中は背後の検索入力、選択、PIN、コピー、実行などを起動するキーイベントを消費し、現在の検索状態を変更してはならない。
- MUST: 選択パスコピーは Windows/Linux では `Ctrl+Shift+C`、macOS では `Cmd+Shift+C` を受理する。GUI backend がこの chord を `Event::Copy` として通知し、`Key::C` が来ない場合も同じ選択パスコピーとして扱う。
- MUST: query 履歴は全タブ共通で最大 100 件まで保持し、空文字は保存しない。同一 query を再登録すると既存項目を除去して最新位置へ移動する。
- MUST: query 履歴はセッション復元ファイルへ永続化し、後方互換を保ったまま復元できる。
- MUST: runtime config の `history_persist_disabled` が有効なとき、query 履歴は読み込み・保存の両方を行わない。
- MUST: `Ctrl+R` で履歴検索モードを開始し、同じ検索欄で query history をファジー検索できる。
- MUST: 履歴検索モード中は履歴検索中であることがわかる表記を行い、結果一覧は履歴候補一覧へ切り替える。
- MUST: 履歴検索モード中は `Enter` / `Ctrl+J` / `Ctrl+M` で選択中の履歴を検索欄へ展開し、`Esc` / `Ctrl+G` でキャンセルして開始前 query へ戻す。
- MUST: preset適用、履歴検索の確定・キャンセルなどがquery文字列をprogrammaticに置換した場合、検索欄へfocusを戻し、TextEditのcursorを置換後query末尾へ移動する。
- MUST: Windows/Linux の `Ctrl+L` と macOS の `Cmd+L` は、通常画面で検索欄の focus を toggle する。同じ shortcut を繰り返すと unfocused → focused → unfocused と遷移しなければならない。
- MUST: 検索オプションに `Ignore Case` チェックボックスを表示し、既定で有効にする。無効化時は検索結果とハイライトの両方を case-sensitive に切り替える。
- SHOULD: query 履歴は打鍵ごとではなく、一定時間の無入力または結果移動開始を契機に確定する。
- SHOULD: IME 合成中の未確定文字列は query 履歴へ保存せず、変換確定後の query のみ履歴対象とする。
- MUST: 検索窓フォーカス中でも `ArrowUp` / `ArrowDown` で `Results` の current row を移動できる。
- MUST: `ArrowUp` / `ArrowDown` および有効な `Ctrl+P` / `Ctrl+N` による current row 移動では、移動先が現在の viewport 内にある限り Results の scroll offset を維持する。移動先の行全体が viewport の上端または下端を越える場合だけ、その行全体が見える最小量を scroll する。下移動から上移動への反転時も上端までは offset を維持し、current row を毎回 viewport の先頭または末尾へ固定してはならない。この契約は key repeat と、手動 scroll や resize 後の keyboard 移動にも適用する。
- MUST: runtime config の `emacs_keybindings_enabled` が `true` のとき、`Ctrl+J` / `Ctrl+M` は検索窓フォーカス有無に関わらず `Enter` と同等に実行/オープンを起動する。
- MUST: `Tab` / `Shift+Tab` はフォーカス位置に依存せず現在行の PIN 固定/解除を実行する。runtime config の `tab_pin_moves_to_next_row` が `false` または未指定のときは選択行移動を行わず、`true` のときは PIN 固定/解除後に選択行を次行へ進める。
- MUST: runtime config の `emacs_keybindings_enabled` が `true` のとき、`Ctrl+I` は検索窓フォーカス有無に関わらず `Tab` と同等に現在行の PIN 固定/解除を実行する。
- MUST: search / index の非同期応答は、active request_id または request-tab routing で結び付いた background tab に対してのみ適用し、stale 応答で現在の root / tab / result state を巻き戻してはならない。
- MUST: background tab の search/sort 応答で選択中 path が変わった場合は、`None`から`Some(path)`への遷移も含め、その tab の旧 preview と pending preview routing を無効化する。非compactionのinactive化で完了済みpreview本文を破棄する場合もtab scoped reload pendingを記録する。tab activation時はcompaction復元または明示reload pendingの場合だけ、新しいcurrent pathのpreviewを既存の非同期/cache worker経路へ要求し、通常activationで無条件要求してはならず、無効化前の遅延`PreviewResponse`も採用してはならない。session restoreのlazy index refreshとreloadが重なる場合は、preview request ownershipをrefreshで消さないようrefreshを先にdispatchし、必要ならkind resolutionを経てpreviewを要求する。
- MUST: tab は `Dormant` / `Loading` / `Ready` / `Refreshing` / `Failed` / `Evicted` の lifecycle と committed snapshot の有無を別々に所有する。request_id は tab generation として扱い、generation が一致する `Finished` だけが building snapshot を committed snapshot へ ownership swap できる。stale terminal は自身の job/mailbox tracking だけを解放し、後続 generation を settle してはならない。
- MUST: 通常・session restore・closed-tab restore のタブ遷移では、active tab の committed/building index、kind resolution collections、base results、results、entry-kind cache を要素単位で複製、同期破棄、capacity 縮小、または全件再構築してはならない。Ready tab の activation は保持 snapshot を即時表示し無条件 reindex してはならない。Refreshing または Failed tab は last-good Results を保持し、新 generation が成功した時だけ置換する。
- MUST: active から open-inactive へ遷移する tab は、その active tenure が2秒以上、またはその tenure 中に query、programmatic query replacement、root、source/filter、depth、result selection/PIN、sort、明示 index refresh のいずれかを変更した場合に限り Recent Inactive 候補になれる。2秒未満かつ該当操作のない瞬間遷移は、既存の Recent Inactive を置換してはならない。候補と直前の active engagement はタブ遷移の単一 transaction として保持し、切替成功時だけ候補を確定する。hard 上限を通常候補だけで満たせない場合は、reclaimer の必要枠を先に予約して既存 Recent Inactive と未確定候補の必要分を単一 batch に退避し、遷移成功時だけ一括 transfer する。予約または transfer に失敗した場合は batch、既存 Recent Inactive、active engagement を一括 rollback する。active tab close の成功時は候補を確定せず、既存 Recent Inactive が hard 回収されなかった場合は維持する。
- MUST: committed snapshot を持つ Recent Inactive は最大1件とし、最終 active 時点から30秒間は通常の inactive cache の soft 上限（count 2、deterministic weight 1,000,000 entry-unit）による eviction 対象外にする。猶予満了は timer 起因の即時 eviction を要求せず、次の cache pressure 判定から通常候補へ戻す。active と sole Warm を除く全 heavy snapshot の hard 上限は count 3、weight 4,000,000 entry-unit とし、超過時は猶予中の Recent Inactive も回収できる。Recent Inactive は scheduler の Warm role と同一視してはならない。
- MUST: Dormant/Evicted tab を activate した場合、Query の空/非空および FileList/Walker 設定にかかわらず、同じ tab transition 内で `Loading` と `Indexing...` を設定して最新 generation を scheduler へ登録しなければならない。root 解決、FileList discovery、結果再構築、allocator compaction を UI thread 上で待ってはならない。既存 Warm generation の activation は同じ request を Active へ promote し、重複 request を作ってはならない。
- MUST: index 中に閉じた tab は closed-tab restore 時に新しい index request を確立しなければならない。search/sort も中断していた場合は、保持済み entries/results から同種 request を新しい tab ID で直ちに再発行し、replacement index 成功後の最終 snapshot に保持した sort mode/scope を再適用しなければならない。replacement index が失敗しても先行する result request を旧 tab ID へ戻してはならない。preview request ownership を破棄した場合は選択中 path の preview reload を再発行し、旧 request ID と response routing は復元後の tab ID へ引き継いではならない。
- MUST: index scheduler は Active 1件と Warm 最大1件を役割・世代・選択 sequence で決定論的に追跡する。A と B が worker を使用中に C を activate した場合、選択対象または直前 Active を Warm として残し、最古/低優先の Warm 1件だけを cancel する。全 background job の一括 preempt、`HashSet` 列挙順による victim 選択、cancel 済み generation の再 activation は禁止する。scheduler cancel は committed snapshot があれば Ready、なければ Dormant へ戻す。
- MUST: 各 index request は data 8 batch 上限の request-scoped mailbox と固定 `Started` / `Truncated` / terminal control slot を持つ。全 publication は generation と別の mailbox-local sequence を持ち、coordinator は `Started`、data、`Truncated`、terminal の順序を維持する。data Full の producer は latest/cancel/mailbox close/shutdown を再確認しながら non-blocking retry し、terminal は data capacity に依存せず最終 sequence と preceding-data count を固定する。Active mailbox を先に drain し、Warm は固定小 quota とし、shared response queue や UI-owned unbounded deferred queue を設けてはならない。
- MUST: active tab の root、検索オプション、query/history、result/selection/preview、notice、pending request、index/search/sort/action/preview の進行状態は、切替、並べ替え、新規作成、close、closed-tab restore、session restore の前後で tab identity と対応し続けなければならない。新規タブ生成前に未確定の共有 query history を確定し、active request の応答は live state、background request の応答は対応する inactive tab state だけを更新しなければならない。request routing が active tab ID を返しても live pending request_id と一致しない応答を active slot の scratch payload へ適用してはならない。
- MUST: session 保存は active tab の live persisted fields と inactive tab の persisted fields だけを射影し、保存のために tab の index/result payload 全体を複製してはならない。
- MUST: supersede または cancel された非同期 flow は、pending / in_progress / deferred action 状態を解放し、現在の UI state を壊さずに継続操作可能でなければならない。
- MUST: action、kind resolution、indexing の worker dispatch は上限付き channel への non-blocking `try_send` 相当で行い、UI thread は空き待ちしてはならない。`Full` と `Disconnected` は通常の制御結果として処理し、要求と UI state を未確定のまま残してはならない。
- MUST: action request は request_id を採番して dispatch を試み、受理された場合にだけ pending action、request-tab routing、in-progress 表示をその request_id へ更新しなければならない。`Full` の場合は直前に受理済みの action state を変更せず、今回の要求を未受理として通知する。`Disconnected` の場合は今回の要求を失敗として終端し、pending/routing/in-progress を新設してはならない。
- MUST: kind resolution request が `Full` になった場合は対象を pending queue の先頭へ戻して後続 frame で再試行し、重複する in-flight 状態を作ってはならない。worker は filesystem metadata I/O より前に tab identity と epoch の最新性を判定し、stale、tab 消失、共有状態の poison のいずれでも filesystem I/O を行わず `kind=None` の terminal response を返して pending/in-flight を解放しなければならない。`Disconnected` でも同じ terminal settlement を行う。
- MUST: index request が `Full` になった場合は UI 側の bounded pending scheduler へ戻し、tab ごとの最新要求だけを再試行対象として保持しなければならない。worker は root の canonicalize、FileList 読み込み、walker 開始より前に supersede/cancel を判定し、stale request を `Canceled` response で終端しなければならない。`Disconnected` でも request routing と pending/in-progress を失敗状態へ収束させなければならない。
- MUST: worker shutdown は新規要求の受理停止、保留要求の drain または cancel、固定 worker の join、response endpoint の close の順で行い、UI 終了を無期限に待たせてはならない。
- MUST: indexing の `Finished` 応答時に未反映の index entries が残っている場合、GUI はそれらを単一フレームで全件吸収してはならない。frame budget 内で分割反映し、全件反映後に terminal state へ遷移しなければならない。
- MUST: `Finished` 応答後の内部後処理 drain は、探索中の表示更新より小さい件数上限を用い、完了速度より入力応答性を優先しなければならない。
- MUST: Walker が上限打ち切り（`Truncated`）に到達した場合でも、GUI は終端直前の大きな batch backlog を過小な固定件数で長時間 drain し続けてはならない。frame budget を応答性の上限として維持しつつ、`Indexing...` の終端尾を短く保てる件数を 1 frame 内で吸収しなければならない。
- MUST: index worker が GUI の反映速度を上回る場合、request mailbox の 8 batch 上限で producer に backpressure を掛けなければならない。GUI は mailbox 外へ未反映 batch を無制限に移さず、active pending entry の 32,768 entry high-water mark と frame budget を併用する。FileList hierarchy replacement も 1 batch 1,024 entry 以下へ分割する。
- MUST: indexing 中の空クエリ・フィルタなし表示では、表示更新のたびに全候補の表示用スナップショットを複製してはならない。表示に必要な上位件数だけを更新し、全件 snapshot は terminal state で確定させなければならない。
- MUST: Results描画はviewport内の行だけをwidget化し、offscreen pathのclone、highlight/layout、widget allocationを行ってはならない。offscreen current rowへのscroll、click/double-clickの絶対index、横方向layout、highlight、preview要求は維持する。描画回帰計測はprocess-global stateを使わずthread-localかつ1描画呼出し単位で隔離し、外部actionを起動せず絶対indexだけを記録する。
- MUST: active indexing 中に空クエリ・フィルタなし状態へ戻す場合、表示更新のために蓄積済み index entries を `runtime.entries` へ全件 clone してはならない。
- MUST: indexing 中にフィルタ適用済みの増分 snapshot を保持している場合、`Finished` 後の terminal state 確定はその snapshot を再利用し、全候補を UI thread で再フィルタしてはならない。
- MUST: kind filter 用の unknown path queue 構築は、対象 entry 全件の `PathBuf` 中間配列を UI thread 上で作ってはならない。entry 自体または cache で kind が既知の path と、既存 queue / in-flight の path を除外しながら直接 queue へ積まなければならない。
- MUST: Walker の `Finished` 応答を受信した後、GUI は残り entry drain や terminal snapshot 確定を内部後処理として継続してよいが、status line の `Indexing...` 表示は停止しなければならない。
- MUST: terminal 後処理の完了時に、大規模 indexing で使用した pending entry queue の容量解放を UI thread 上で同期的に実行してはならない。
- MUST: notice の更新/消去や通常の status line 再計算は、OS メモリ使用量の同期サンプリングを伴ってはならない。メモリ表示更新は専用の定期サンプリング経路で行わなければならない。
- MUST: Root 変更時は旧 Root 由来の選択状態（current row / pinned / preview）を破棄し、誤操作を防止する。
- MUST: Root 変更時は旧 Root 向けに保留中の FileList 上書き確認、祖先追記確認、Walker 利用確認、deferred-after-index を破棄する。
- MUST: Root 変更時は query 履歴の参照位置のみリセットし、履歴本体は保持する。
- MUST: IME のスペース/変換確定フォールバック挿入はクエリ末尾固定ではなくカーソル位置へ挿入し、カーソル位置を挿入後位置へ更新する。
- SHOULD: runtime config の `restore_tabs_enabled` が `true` のときのみ、前回終了時のタブ状態（root/query/filter active tab）を起動時に復元できる。
- SHOULD: 保存済み `last_root` / `default_root` / tab root は native window 初期表示前に存在確認しない。存在しない root は初回 index refresh の失敗/空結果として UI 上で扱い、切断済みドライブや遅いパスで window 表示を遅延させない。
- MUST: 保存済み window 位置が現在の表示範囲外にある場合、GUI 起動時の初期位置は現在の表示範囲内へ補正しなければならない。現在の仮想ディスプレイ内に収まる負座標は有効な配置として維持しなければならない。
- SHOULD: タブは任意の accent color を持てる。
- MUST: 非 active tab に accent color が設定されている場合、タブ下部にその色の装飾を表示する。
- MUST: active tab に accent color が設定されている場合、タブ全面をその色で装飾する。
- MUST: タブ accent color は保存対象のタブ状態に含め、タブ復元時に保持する。
- MUST: タブ復元が無効、または `--root` / 起動時 query / 起動時 depth が明示された場合は saved session 全体を復元しない。明示 root がなければ `Set as default`、`last_root`、通常 root の順で選び、saved-session root を default より優先してはならない。Restore Tabs が有効で明示値がなく有効な saved session がある場合だけ active saved-tab root を優先し、有効な session がない場合は `last_root`、default、通常 root の順とする。
- MUST: runtime config の `restore_tabs_enabled` が有効な間は `Set as default` 操作を UI で無効化し、起動 root と競合する永続設定を追加できないようにする。
- MUST: 保存済み root list の追加・編集・削除は `Manage list` へ統合し、メインウィンドウへ埋め込まれない独立した非ブロッキング native 管理ウィンドウで操作できなければならない。
- MUST: `Manage list` は FlistWalker メインウィンドウの外枠を基準に中央配置し、メインウィンドウが負座標を含む別ディスプレイ上にある場合も同じ仮想デスクトップ座標系で配置しなければならない。メインウィンドウの geometry を取得できない場合は OS の既定配置へフォールバックしてよい。
- MUST: `Manage list` では直接入力したフォルダパス、または `Browse...` で選択したフォルダを draft list へ追加できなければならない。
- MUST: `Manage list` の追加・編集時の folder 種別判定、canonicalize、draft snapshot との重複判定は専用 worker で実行し、UI frame 上で filesystem I/O を行ってはならない。要求は request_id、dialog generation、Add/Edit intent、edit index を相関し、close/reopen または後続操作より古い success/error を draft へ反映してはならない。
- MUST: root 検証中は処理中であることを表示し、worker 切断は対象入力欄のエラーとして終端させなければならない。検証結果は入力時点の状態であり、その後も folder が存在し続けることは保証しない。
- MUST: `Manage list` の通常状態ではチェックボックスを表示せず、単一の draft list 項目を行選択し、`Edit` でその行だけをテキスト編集へ切り替えられなければならない。
- MUST: `Manage list` の通常状態にある draft list 項目は、パス文字列部分だけでなく一覧の利用可能な横幅全体をクリックして行選択できなければならない。
- MUST: `Manage list` で編集中の入力が元のパスから変更されていない場合、別の行をクリックすると現在の編集を解除してその行を選択し、別の行をダブルクリックすると編集対象をその行へ切り替えなければならない。入力が変更済みの場合は、`Save` または `Cancel` が実行されるまで別の行へ切り替えてはならず、必要な操作を通知しなければならない。
- MUST: 編集値は空文字、フォルダではないパス、または自分以外の draft 項目と重複するパスの場合は反映せず、編集状態を維持して理由を通知しなければならない。
- MUST: `Manage list` の追加または編集に失敗した場合、対象入力欄を赤系の枠と背景でエラー表示し、操作名を含む理由を入力欄の直下へ表示しなければならない。失敗後は対象入力欄へフォーカスを戻して入力値を全選択し、入力値が変更された時点でその欄のエラー表示を解除しなければならない。
- MUST: default root に対応する draft 項目を編集した場合、`Apply` または `OK` による確定時に default root も編集後のパスへ追従しなければならない。
- MUST: `Manage list` は `Remove...` により明示的な削除モードへ移行し、削除モード中だけ複数選択用チェックボックスと `Remove selected` / `Cancel` を表示しなければならない。
- MUST: 削除モードの `Cancel` は選択状態だけを破棄して通常状態へ戻り、draft list を変更してはならない。
- MUST: `Manage list` の draft 変更は `Apply` または `OK` を押したタイミングでのみ保存済み root list へ反映し、`Cancel` またはウィンドウ close では反映してはならない。
- SHOULD: タブ復元時は active tab のみ起動直後に Loading とし、background tab は Dormant のまま初回 activate 時に遅延 load する。
- MUST: 起動時復元された background tab は初回 activate まで index/search 候補を構築しない。初回 activate した generation は Active mailbox として Warm より先に処理し、Warm の Full data lane や stale terminal によって開始・応答適用を待たせてはならない。
- MUST: 現在のプロセス中に閉じた GUI タブは in-memory の直近順スタックとして保持し、`Ctrl+Shift+T`（macOS では `Cmd+Shift+T`）で最後に閉じたタブから active tab として復元できなければならない。
- MUST: 閉じたタブを復元する際は、閉じた時点の tab index を優先し、現在の tab 数を超える場合のみ末尾へ復元しなければならない。
- MUST: 閉じたタブ復元スタックは直近 25 件の lightweight intent（root/query/history、filter/source/depth、accent、元位置、sort mode/scope、PIN、選択 path）を保持する。closed heavy snapshot と Recent Inactive 以外の open-inactive heavy snapshot は共通の LRU soft cache（count 2、deterministic weight 1,000,000 entry-unit）内だけで保持し、active、sole Warm、猶予中の Recent Inactive は soft cache 外とする。hard 上限はこれらを含む inactive/closed owner 全体へ適用する。heavy snapshot が Evicted の closed tab は intent を即時表示して Loading を開始し、旧 request/routing を引き継いではならない。
- MUST: heavy snapshot の最終 drop は `WorkerRuntime` が所有する固定 reclaimer worker で行う。queue は4件を上限とし、Full 時の対象は cache count/weight 内で eviction-pending のまま保持する。retirement が必要な terminal commit は固定 terminal-pending slot で ownership swap を待ち、同 tab の後続 generation と新しい heavy cache admission を止める。close/restore は未上限 side queue を作らず、UI は操作可能なまま reclaim pending status を表示する。
- MUST: background Finished の partial/continuation 結合、file/dir filter、Walker の未確定 kind queue、Create File List 用 path snapshot は request-owned finalization として frame budget 内で増分処理し、候補件数に比例する全件 clone/scan/drop を単一 UI frame で行ってはならない。finalization は index worker 数と同じ最大2件に固定し、terminal publish 時点で worker inflight slot を解放して active request を優先する。finalization Full 時の terminal は request mailbox の固定 terminal slot に留め、background state を tab-local side queue へ移してはならない。その tab の activation は finalizer admission まで latest-only で保留し、activation 後に terminal を受けた demoted generation も background state を結合する finalization barrier を通す。最終化中に Ignore List、Ignore Case、Files、Folders が変わった場合は旧 filter/kind 出力を reclaimer へ ownership transfer してから最新 policy で増分 filter をやり直し、reclaimer Full では旧 policy/output を rollback して再試行する。close/root change/shutdown は未完了 finalization も同じ request resource として回収する。
- MUST: 閉じたタブ復元は前回セッションの閉じたタブ履歴を読み書きしてはならず、復元対象がない場合はタブ構成を変更してはならない。
- MUST: 閉じたタブを復元する際は新しい tab id を割り当て、閉じる前の pending search/index/preview/action/sort request_id を復元してはならない。
- MUST: タブの close ボタンにマウスが重なった場合、close ボタンの押下領域をタブ本体の押下領域と視覚的に区別できなければならない。
- MUST: タブバーはドラッグアンドドロップで並び替え可能でなければならず、ドロップ先は既存タブ領域内に限定する。
- MUST: タブ並び替え時は active tab を index ではなく同一タブ実体として維持し、root/query/filter/進行中状態を他タブへ取り違えてはならない。
- SHOULD: 入力デバウンスで連続打鍵時の再描画負荷を抑える。
- MUST: 結果ペインは `Sort` セレクタを持ち、`Score` / `Name (A-Z)` / `Name (Z-A)` / `Modified (New)` / `Modified (Old)` / `Created (New)` / `Created (Old)` / `Size (Large)` / `Size (Small)` を選択できる。
- MUST: 結果ペインは表示件数と limit 前の全マッチ件数を区別できる表示を持ち、limit により一部だけを表示している場合は `shown of total` 相当の情報を示す。
- MUST: 結果ペインは sort scope として `Shown results` / `All matches` を選択でき、既定は `Shown results` とする。

### Preconditions / Postconditions
- Preconditions: GUI モードで起動しインデックス構築可能。
- Postconditions: 利用者がプレビュー確認後に安全に実行/オープンできる。

### Regression Guard
- 発生条件: restore tab の体感改善を shared response queue の先送り、全 background request の一括 preempt、activation ごとの unconditional reindex、または closed tab の full payload 常時保持で実現する。
- 期待動作: saved session は intent だけを復元し、active だけ Loading、background は Dormant で開始する。scheduler は Active + sole Warm、response は順序付き bounded request mailbox、snapshot は committed/building 分離、inactive/closed は共通 LRU、最終 drop は bounded reclaimer とする。Refresh failure は last-good を保持し、Ready activation は同じ snapshot を即時表示する。
- 非対象範囲: 単一 active FileList snapshot 自体の hard byte cap、process restart をまたぐ full snapshot 永続化、検索意味論の変更。
- 関連テストID: TC-203, TC-204, TC-205, TC-206, TC-207, TC-208。
- 発生条件: 新しい picker/modal が `ArrowDown` / `ArrowUp` / `Enter` / `Escape` を feature 内で直接処理し、共有 Emacs command mapping を通さないため、その画面だけ `Ctrl+N` / `Ctrl+P` / `Ctrl+J` / `Ctrl+M` / `Ctrl+G` が無効になる。
- 期待動作: Emacs 設定が有効なら preset picker を含む全対応対話面で通常キーと同じ application command が動作し、全 application-owned 単一行入力で同じ Emacs text-editing reducer が動作する。無効なら chord をアプリ独自操作として処理しない。
- 非対象範囲: 対応する通常 command が存在しない画面への新規操作追加、OS/IME が所有する text composition、`ctrl_w_deletes_word_in_query` が別途制御する `Ctrl+W`。
- 関連テストID: TC-201。
- 発生条件: GUI が application shortcut を TextEdit より先に処理する構成で、検索欄フォーカス中の `Ctrl+W` が Emacs 単語削除へ到達する前にタブ終了として消費される。または同じイベントを TextEdit と独自 reducer の双方が処理して2語削除する。
- 期待動作: `emacs_keybindings_enabled=true` かつ `ctrl_w_deletes_word_in_query=true` の検索欄・履歴検索フィルターでは `Ctrl+W` を一度だけ直前単語削除として処理し、タブ数を変えない。設定無効、Emacs 無効、検索欄外では従来どおりタブを閉じ、IME 合成中はどちらも起動しない。
- 非対象範囲: TUI の既存 `Ctrl+W`、`Ctrl+K` など他の Emacs 編集 chord、タブ close ボタン、macOS の `Cmd+W`。
- 関連テストID: TC-199。
- 発生条件: GUI backend が `Ctrl+Shift+C` / `Cmd+Shift+C` を通常の `Key::C` ではなく `Event::Copy` に変換し、検索窓フォーカス中に TextEdit 側のコピー経路だけが動く。
- 期待動作: Shift 付き primary copy chord は選択中または PIN 済み path のコピーを優先し、Shift なしの通常コピーは path copy shortcut として扱わない。
- 非対象範囲: TextEdit 内の通常 `Ctrl+C` / `Cmd+C` による query text コピー。
- 関連テストID: TC-018.

## SP-011 GUI 回帰テスト計画
### Requirements
- MUST: GUI の主要フロー（起動、検索、選択、実行/オープン、再読込）を手順化する。
- MUST: 仕様IDに対応するテストIDで結果を記録する。
- SHOULD: 主要OS差分（Windows/macOS/Linux）で確認観点を持つ。

### Preconditions / Postconditions
- Preconditions: 対象ビルドが実行可能。
- Postconditions: 回帰実施可否を判定できる記録が残る。
## SP-013 検索結果ソート
### Requirements
- MUST: 既定の `Shown results` scope では、ソートは現在の検索結果スナップショットにのみ適用し、インデックス構築や FileList 解析の経路へ属性取得を追加してはならない。
- MUST: 検索応答は表示上限適用前の全マッチ件数を返し、GUI は表示中件数と全マッチ件数を区別して扱わなければならない。
- MUST: `All matches` scope では、現在の query / File・Folder filter / Ignore List / regex / case-sensitivity 条件を満たす全マッチ集合から選択 sort key の上位 `limit` 件を作り直さなければならない。
- MUST: batch CLI と TUI も同じ sort mode vocabulary を使用し、non-score sort は full match set への sort を limit より先に適用しなければならない。score sort は既存 ranking/tie behavior を維持する。
- MUST: GUI の候補収集深度 control、tab-local ownership、preset 表示と適用後の持続性は SP-021 に従う。
- MUST: `All matches` scope であっても、GUI は全マッチを一覧へ全件描画せず、表示対象は `limit` 件以内に抑えなければならない。
- MUST: `Score` は検索エンジンが返した元の順位へ戻せる。All matchesの非Score結果から戻す場合も、全候補中のScore上位集合を復元する。mode/scope切替中の旧応答は破棄し、保留queryは最新設定で完了させる。
- MUST: `Name` ソートはファイル/ディレクトリ名を主キー、正規化済みフルパスを副キーとして即時に並び替える。
- MUST: `Modified` / `Created` / `Size` ソートは結果スナップショットに含まれる path だけを対象に、別ワーカーで `metadata` を遅延取得して適用する。
- MUST: `All matches` scope の非 `Score` ソートは UI thread ではなく worker で実行し、検索応答の request_id / tab routing により古い応答を破棄できなければならない。
- MUST: `Modified` / `Created` / `Size` の取得中も UI 入力と一覧操作を維持する。
- MUST: query が 1 文字でも変化した場合、適用済みソートと保留中ソート要求を破棄し、表示順を `Score` に戻す。
- MUST: root 変更、index refresh、filter 変更、tab 切替で結果スナップショットが変化した場合も、保留中ソート要求は破棄できる。
- MUST: 属性キャッシュは上限付きで保持し、上限超過時は古い項目から破棄する。
- MUST: `created()` が取得できない OS/ファイルは `None` として扱い、`Created` ソート時は末尾へ送る。
- MUST: `Size` ソートは通常ファイルの byte size を使う。フォルダ、または size を取得できない項目は `None` として扱い、昇順・降順のどちらでも末尾へ送る。フォルダサイズを再帰計算してはならない。
- SHOULD: 既にキャッシュ済みの属性だけで並び替え可能な場合、日付ソートも同期的に完了できる。

### Preconditions / Postconditions
- Preconditions: 検索結果が 1 件以上ある。
- Postconditions: `Score` は元の検索順位、他ソートは選択したキー順で結果が表示される。

### Edge / Error
- 検索結果が空のときはソート要求を無視する。
- path が削除済み・権限不足などで属性取得に失敗した場合はその項目だけ `None` 扱いで継続する。
- 保留中ソート応答が古い query / root / tab に属する場合は破棄する。

## SP-022 Stateful endurance 検証仕様
### Requirements
- MUST: app-level endurance test は同じ seed と profile から同じ論理 event 列を生成し、wall-clock sleep や host 固有 path 表現を順序決定へ使用してはならない。
- MUST: event 列は tab create/close/restore/switch/reorder、query/root 変更、index/search dispatch、batch/replace/finish/fail/cancel、stale または out-of-order completion を含められなければならない。
- MUST: 各 event 後に tab が1件以上、tab ID が一意、active index が範囲内、request routing が live tab だけを参照、`results.len() <= total_match_count`、current row が結果範囲内または空結果の既定位置、index pending queue が4以下、coordinator in-flight が2以下であることを検証しなければならない。
- MUST: closed tab または stale request の応答は、active tab と別の live tab の root/query/result/noticeを巻き戻してはならない。
- MUST: quiescence phase は既知 request を terminal response へ進め、規定 step 以内に search/index/preview/action/sort/FileList の pending、routing、in-flight、progress state が解放されることを検証しなければならない。
- MUST: required PR profile は固定 regression corpus と複数の固定 seed を通常の `cargo test` で3 OS継続実行し、失敗時に seed、step、event、state digest と再生コマンドを表示しなければならない。
- SHOULD: non-required scheduled profile は大きな seed/event budget と一時 root 上の実 worker soak を実行し、ログを artifact として保持する。
