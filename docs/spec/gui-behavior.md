# GUI Behavior Specification

## SP-010 GUI 操作仕様
### Requirements
- MUST: 検索入力、結果リスト、プレビューペイン、実行/オープンを提供する。
- MUST: Source（FileList/Walker）と Root を画面表示する。
- MUST: フッター右端に現在 version を常時表示する。
- MUST: 非マッチは非表示とし、一致箇所ハイライトを提供する。
- MUST: ハイライトは search と同じ query interpretation を用い、検索結果と表示が一致するようにする。
- MUST: 検索結果の再適用時は current row の行番号を維持し、結果数が減った場合のみ末尾へ丸める。未選択状態は自動選択に変換しない。
- MUST: GUI 起動直後および `Ctrl+G` / `Esc` による検索キャンセル後は、候補が存在する場合 current row を 1 行目へ既定化して表示する。
- MUST: 複数選択と一括アクションを提供する。
- MUST: Windows では on-demand placeholder と判定できるファイルの本文プレビューを行わず、取得系 I/O による意図しないダウンロードを避ける。
- MUST: 本文プレビューは拡張子で制限せず、UTF-8、BOM 付き UTF-16、および主要 OS で一般的なレガシー文字コードを順に解釈して、テキストとして安全に復号できた内容を表示する。
- SHOULD: FileList 読み込み直後の未解決候補は背景解決により FILE/DIR/LINK 表示を後追い更新できる。
- MUST: runtime config の `emacs_keybindings_enabled` が `true` のとき、`Ctrl+N` / `Ctrl+P` / `Ctrl+G` / `Esc` は検索窓フォーカス中でも有効である。
- MUST: runtime config の `emacs_keybindings_enabled` が `false` のとき、Emacs 風の `Ctrl+N` / `Ctrl+P` / `Ctrl+V` / `Alt+V` / `Ctrl+G` / `Ctrl+R` / `Ctrl+I` / `Ctrl+J` / `Ctrl+M` および検索欄編集用 `Ctrl+A` / `Ctrl+E` / `Ctrl+B` / `Ctrl+F` / `Ctrl+H` / `Ctrl+D` / `Ctrl+W` / `Ctrl+K` / `Ctrl+Y` / `Ctrl+U` はアプリ側ショートカットとして消費してはならない。
- MUST: 選択パスコピーは Windows/Linux では `Ctrl+Shift+C`、macOS では `Cmd+Shift+C` を受理する。GUI backend がこの chord を `Event::Copy` として通知し、`Key::C` が来ない場合も同じ選択パスコピーとして扱う。
- MUST: query 履歴は全タブ共通で最大 100 件まで保持し、空文字と連続重複 query は履歴保存しない。
- MUST: query 履歴はセッション復元ファイルへ永続化し、後方互換を保ったまま復元できる。
- MUST: runtime config の `history_persist_disabled` が有効なとき、query 履歴は読み込み・保存の両方を行わない。
- MUST: `Ctrl+R` で履歴検索モードを開始し、同じ検索欄で query history をファジー検索できる。
- MUST: 履歴検索モード中は履歴検索中であることがわかる表記を行い、結果一覧は履歴候補一覧へ切り替える。
- MUST: 履歴検索モード中は `Enter` / `Ctrl+J` / `Ctrl+M` で選択中の履歴を検索欄へ展開し、`Esc` / `Ctrl+G` でキャンセルして開始前 query へ戻す。
- MUST: 検索オプションに `Ignore Case` チェックボックスを表示し、既定で有効にする。無効化時は検索結果とハイライトの両方を case-sensitive に切り替える。
- SHOULD: query 履歴は打鍵ごとではなく、一定時間の無入力または結果移動開始を契機に確定する。
- SHOULD: IME 合成中の未確定文字列は query 履歴へ保存せず、変換確定後の query のみ履歴対象とする。
- MUST: 検索窓フォーカス中でも `ArrowUp` / `ArrowDown` で `Results` の current row を移動できる。
- MUST: runtime config の `emacs_keybindings_enabled` が `true` のとき、`Ctrl+J` / `Ctrl+M` は検索窓フォーカス有無に関わらず `Enter` と同等に実行/オープンを起動する。
- MUST: `Tab` / `Shift+Tab` はフォーカス位置に依存せず現在行の PIN 固定/解除のみを実行し、選択行移動は行わない。
- MUST: runtime config の `emacs_keybindings_enabled` が `true` のとき、`Ctrl+I` は検索窓フォーカス有無に関わらず `Tab` と同等に現在行の PIN 固定/解除を実行する。
- MUST: search / index の非同期応答は、active request_id または request-tab routing で結び付いた background tab に対してのみ適用し、stale 応答で現在の root / tab / result state を巻き戻してはならない。
- MUST: supersede または cancel された非同期 flow は、pending / in_progress / deferred action 状態を解放し、現在の UI state を壊さずに継続操作可能でなければならない。
- MUST: indexing の `Finished` 応答時に未反映の index entries が残っている場合、GUI はそれらを単一フレームで全件吸収してはならない。frame budget 内で分割反映し、全件反映後に terminal state へ遷移しなければならない。
- MUST: `Finished` 応答後の内部後処理 drain は、探索中の表示更新より小さい件数上限を用い、完了速度より入力応答性を優先しなければならない。
- MUST: Walker が上限打ち切り（`Truncated`）に到達した場合でも、GUI は終端直前の大きな batch backlog を過小な固定件数で長時間 drain し続けてはならない。frame budget を応答性の上限として維持しつつ、`Indexing...` の終端尾を短く保てる件数を 1 frame 内で吸収しなければならない。
- MUST: indexing 中の空クエリ・フィルタなし表示では、表示更新のたびに全候補の表示用スナップショットを複製してはならない。表示に必要な上位件数だけを更新し、全件 snapshot は terminal state で確定させなければならない。
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
- MUST: タブ復元が無効、または `--root` / 起動時 query が明示された場合は、従来どおり `Set as default` の root 選択を優先する。
- MUST: runtime config の `restore_tabs_enabled` が有効な間は `Set as default` 操作を UI で無効化し、起動 root と競合する永続設定を追加できないようにする。
- SHOULD: タブ復元時は active tab のみ起動直後に再インデックスし、background tab は初回 activate 時に遅延 reindex する。
- MUST: 現在のプロセス中に閉じた GUI タブは in-memory の直近順スタックとして保持し、`Ctrl+Shift+T`（macOS では `Cmd+Shift+T`）で最後に閉じたタブから active tab として復元できなければならない。
- MUST: 閉じたタブを復元する際は、閉じた時点の tab index を優先し、現在の tab 数を超える場合のみ末尾へ復元しなければならない。
- MUST: 閉じたタブ復元スタックは直近 25 件を上限とし、上限を超えた場合は最も古い閉じたタブから破棄しなければならない。
- MUST: 閉じたタブ復元は前回セッションの閉じたタブ履歴を読み書きしてはならず、復元対象がない場合はタブ構成を変更してはならない。
- MUST: 閉じたタブを復元する際は新しい tab id を割り当て、閉じる前の pending search/index/preview/action/sort request_id を復元してはならない。
- MUST: タブの close ボタンにマウスが重なった場合、close ボタンの押下領域をタブ本体の押下領域と視覚的に区別できなければならない。
- MUST: タブバーはドラッグアンドドロップで並び替え可能でなければならず、ドロップ先は既存タブ領域内に限定する。
- MUST: タブ並び替え時は active tab を index ではなく同一タブ実体として維持し、root/query/filter/進行中状態を他タブへ取り違えてはならない。
- SHOULD: 入力デバウンスで連続打鍵時の再描画負荷を抑える。
- MUST: 結果ペインは `Sort` セレクタを持ち、`Score` / `Name (A-Z)` / `Name (Z-A)` / `Modified (New)` / `Modified (Old)` / `Created (New)` / `Created (Old)` を選択できる。

### Preconditions / Postconditions
- Preconditions: GUI モードで起動しインデックス構築可能。
- Postconditions: 利用者がプレビュー確認後に安全に実行/オープンできる。

### Regression Guard
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
- MUST: ソートは現在の検索結果スナップショットにのみ適用し、インデックス構築や FileList 解析の経路へ属性取得を追加してはならない。
- MUST: `Score` は検索エンジンが返した元の順位へ戻せる。
- MUST: `Name` ソートはファイル/ディレクトリ名を主キー、正規化済みフルパスを副キーとして即時に並び替える。
- MUST: `Modified` / `Created` ソートは結果スナップショットに含まれる path だけを対象に、別ワーカーで `metadata` を遅延取得して適用する。
- MUST: `Modified` / `Created` の取得中も UI 入力と一覧操作を維持する。
- MUST: query が 1 文字でも変化した場合、適用済みソートと保留中ソート要求を破棄し、表示順を `Score` に戻す。
- MUST: root 変更、index refresh、filter 変更、tab 切替で結果スナップショットが変化した場合も、保留中ソート要求は破棄できる。
- MUST: 属性キャッシュは上限付きで保持し、上限超過時は古い項目から破棄する。
- MUST: `created()` が取得できない OS/ファイルは `None` として扱い、`Created` ソート時は末尾へ送る。
- SHOULD: 既にキャッシュ済みの属性だけで並び替え可能な場合、日付ソートも同期的に完了できる。

### Preconditions / Postconditions
- Preconditions: 検索結果が 1 件以上ある。
- Postconditions: `Score` は元の検索順位、他ソートは選択したキー順で結果が表示される。

### Edge / Error
- 検索結果が空のときはソート要求を無視する。
- path が削除済み・権限不足などで属性取得に失敗した場合はその項目だけ `None` 扱いで継続する。
- 保留中ソート応答が古い query / root / tab に属する場合は破棄する。
