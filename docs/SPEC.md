# SPEC

## SP-001 FileList 優先読み込み
### Requirements
- MUST: FileList 優先モード有効時のみ、検索ルート直下で `FileList.txt` と `filelist.txt` を検出する。
- MUST: 両方ある場合は `FileList.txt` を優先する。
- MUST: 上記2名が無い場合、`filelist.txt` と大文字小文字のみ異なるファイル名（例: `FILELIST.TXT`）をルート直下から検出対象に含める。
- MUST: 空行と `#` コメント行を無視する。
- MUST: 階層 FileList 展開は、読み込み済み候補内でファイル名が `FileList.txt` / `filelist.txt` に完全一致するエントリのみを対象とする。
- MUST: 階層 FileList 展開中も supersede（新しい request_id）で中断できること。
- MUST: FileList 作成時は、祖先ディレクトリ直下の既存 `FileList.txt` / `filelist.txt` へ作成済み子 FileList の参照を重複なく追記できる。
- MUST: 祖先ディレクトリ直下の既存 FileList へ追記が発生しうる場合、Create File List 実行前に利用者確認を要求する。
- MUST: Create File List の保留状態（overwrite 確認、祖先追記確認、Walker 利用確認、index 完了待ち）では、GUI から明示的にキャンセルできる。
- MUST: Create File List 実行中は status panel にキャンセル導線を表示し、利用者が再実行ボタンや root 変更へ頼らず中断要求できる。
- MUST: Create File List のキャンセル要求後、root 直下の最終置換と祖先 FileList 追記は開始前なら実行してはならない。
- MUST: 上記の祖先 FileList 追記後は、親 FileList の mtime を更新前の値へ戻す。
- MUST: 祖先探索や親 FileList 更新で権限不足・読込失敗が発生した場合はエラーを返さず、その時点で追記処理のみを終了する。
- SHOULD: 相対パスはルート起点で絶対化する。
- SHOULD: 重複を除去する。
- SHOULD: include_files/include_dirs が両方有効な場合、種別判定（FILE/DIR/LINK）は遅延解決して初期読み込みを優先する。

### Preconditions / Postconditions
- Preconditions: 検索ルートが確定している。
- Postconditions: 有効な既存パスだけで候補集合が構築される。

### Edge / Error
- 空ファイルは候補ゼロ件で正常終了する。
- 読み込み失敗時はエラーを返し、終了コードを非ゼロにする。
- 利用者が祖先追記確認を拒否した場合、root 直下の FileList 作成だけを継続し、祖先追記は行わない。
- 利用者が Create File List をキャンセルした場合、進行中 request は `Canceled` として扱い、成功/失敗通知や再インデックスを発生させない。

## SP-002 Walker 走査
### Requirements
- MUST: FileList 未使用時にルート以下を再帰走査し候補化する。
- MUST: ファイル/フォルダの包含条件（include_files/include_dirs）を適用する。
- MUST: インデックス構築中でも GUI は逐次的に候補表示を更新できる。
- SHOULD: 循環リンクを避ける。
- SHOULD: 空クエリ時は新規バッチを即時に一覧へ反映し、非空クエリ時は UI 負荷を抑えるため間引き更新する。

### Preconditions / Postconditions
- Preconditions: `FileList.txt` / `filelist.txt` が使用されない。
- Postconditions: 既存パスのみの候補集合を返す。

### Edge / Error
- 権限不足ディレクトリはスキップし、全体処理は継続する。
- 候補ゼロ件でも正常終了する。

## SP-003 ファジー検索
### Requirements
- MUST: 通常語はファジー検索（AND 条件）で評価する。
- SHOULD: 通常語の各トークンは、subsequence 一致だけの候補よりもリテラル一致する候補を高く順位付けする。
- MUST: `'` は完全一致条件として評価する。
- MUST: `!` は除外条件として評価する。
- MUST: `^` / `$` は非 regex モードでは「先頭/末尾の隣接文字制約 + ファジー評価」として評価する。
- MUST: 検索結果のハイライトは search と同じ query interpretation を用い、exact / include / exclude / anchor / OR の解釈差を生じさせてはならない。
- SHOULD: 厳密な prefix/suffix 一致が必要な場合は regex モードを使える。
- MUST: 上位 `limit` 件を関連度順で返す。
- SHOULD: 大文字小文字差を緩和する。

### Preconditions / Postconditions
- Preconditions: 候補集合が構築済み。
- Postconditions: スコア降順結果を返す。

### Edge / Error
- クエリ空文字または `limit=0` は空結果を返す。
- 正規表現モードで無効パターンは結果を返さず、GUI ではエラー理由を通知する。

## SP-004 ファイル実行/オープン
### Requirements
- MUST: 実行可能ファイルはプロセス起動する。
- MUST: 非実行ファイルは既定アプリでオープンする。
- MUST: Windows の一般 `.ps1` は既定動作で直接実行してはならず、既定アプリでオープンしなければならない。
- MUST: 外部コマンドはシェル展開なしで実行する。
- MUST: Windows で既定アプリ起動やフォルダ表示へパスを渡す際、`&` などの特殊記号をシェル解釈させず、リテラルなパスとして OS へ引き渡す。
- MUST: 実行/オープン直前に、選択パスが現在 root 配下であることを検証する。
- MUST: 上記の root 配下判定はインデクシング処理に追加せず、アクション実行直前だけで行う。
- MUST: root 外パスは一覧表示されていても実行/オープンを拒否し、利用者へ通知する。
- MUST: UNC root を検索 root とする場合も、同一 UNC root 配下のパスは許可する。

### Preconditions / Postconditions
- Preconditions: 選択対象がファイル。
- Postconditions: root 配下なら実行またはオープン要求が OS に渡され、root 外なら拒否通知だけが返る。

### Edge / Error
- 起動失敗時はユーザ向けメッセージを返す。
- 拡張子関連付け未定義は失敗として通知する。
- 別ドライブ、別 UNC share、または `..` 解決後に root 外となるパスは拒否する。

## SP-005 フォルダオープン
### Requirements
- MUST: 選択フォルダを OS 既定のファイルマネージャで開く。
- MUST: 存在しないパスには操作しない。
- MUST: Windows でフォルダを開く際も、パス中の特殊記号をシェル解釈させずに処理する。

### Preconditions / Postconditions
- Preconditions: 選択対象がディレクトリ。
- Postconditions: フォルダオープン要求が OS に渡される。

### Edge / Error
- コマンド不在時は明示的なエラーを返す。

## SP-006 CLI 契約
### Requirements
- MUST: `--cli` 指定時は GUI を起動せず標準出力に結果を表示する。
- MUST: `--root` と `--limit` を受理する。
- MUST: クエリ未指定時は候補一覧を `limit` 件以内で表示する。
- SHOULD: 出力形式は機械処理しやすい行単位とする。

### Preconditions / Postconditions
- Preconditions: CLI モードで起動される。
- Postconditions: 結果またはエラーが標準出力/標準エラーへ出力される。

## SP-007 性能
### Requirements
- SHOULD: 10万件規模で検索応答 100ms 未満を目標とする。
- SHOULD: インデックス構築中も UI 操作が停止しない。
- SHOULD: 正規表現クエリはクエリ単位でコンパイルし、候補ごとの再コンパイルを避ける。
- SHOULD: プレビューキャッシュは上限管理し、長時間利用でメモリが無制限に増加しない。

### Preconditions / Postconditions
- Preconditions: 候補集合が利用可能。
- Postconditions: 計測可能な遅延特性を示せる。

## SP-008 エラー処理
### Requirements
- MUST: ユーザが原因を理解できるメッセージを返す。
- MUST: 失敗時は非ゼロ終了コードを返す。
- SHOULD: GUI では失敗内容を明示表示する。

### Preconditions / Postconditions
- Preconditions: 例外または失敗が発生。
- Postconditions: エラー通知と終了状態が一貫する。

## SP-009 テスト容易性
### Requirements
- MUST: indexer/search/actions/ui_model を分離し単体テスト可能にする。
- MUST: OS 依存処理はモック可能な境界を維持する。
- SHOULD: 仕様IDとテストIDの対応を継続管理する。

### Preconditions / Postconditions
- Preconditions: モジュール境界が定義済み。
- Postconditions: 主要仕様を unit/integration で検証可能。

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
- SHOULD: FileList 読み込み直後の未解決候補は背景解決により FILE/DIR/LINK 表示を後追い更新できる。
- MUST: `Ctrl+N` / `Ctrl+P` / `Ctrl+G` / `Esc` は検索窓フォーカス中でも有効である。
- MUST: 選択パスコピーは Windows/Linux では `Ctrl+Shift+C`、macOS では `Cmd+Shift+C` を受理する。
- MUST: query 履歴は全タブ共通で最大 100 件まで保持し、空文字と連続重複 query は履歴保存しない。
- MUST: query 履歴はセッション復元ファイルへ永続化し、後方互換を保ったまま復元できる。
- MUST: `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` のとき、query 履歴は読み込み・保存の両方を行わない。
- MUST: `Ctrl+R` で履歴検索モードを開始し、同じ検索欄で query history をファジー検索できる。
- MUST: 履歴検索モード中は履歴検索中であることがわかる表記を行い、結果一覧は履歴候補一覧へ切り替える。
- MUST: 履歴検索モード中は `Enter` / `Ctrl+J` / `Ctrl+M` で選択中の履歴を検索欄へ展開し、`Esc` / `Ctrl+G` でキャンセルして開始前 query へ戻す。
- MUST: 検索オプションに `Ignore Case` チェックボックスを表示し、既定で有効にする。無効化時は検索結果とハイライトの両方を case-sensitive に切り替える。
- SHOULD: query 履歴は打鍵ごとではなく、一定時間の無入力または結果移動開始を契機に確定する。
- SHOULD: IME 合成中の未確定文字列は query 履歴へ保存せず、変換確定後の query のみ履歴対象とする。
- MUST: 検索窓フォーカス中でも `ArrowUp` / `ArrowDown` で `Results` の current row を移動できる。
- MUST: `Ctrl+J` / `Ctrl+M` は検索窓フォーカス有無に関わらず `Enter` と同等に実行/オープンを起動する。
- MUST: `Tab` / `Shift+Tab` はフォーカス位置に依存せず現在行の PIN 固定/解除のみを実行し、選択行移動は行わない。
- MUST: `Ctrl+I` は検索窓フォーカス有無に関わらず `Tab` と同等に現在行の PIN 固定/解除を実行する。
- MUST: Root 変更時は旧 Root 由来の選択状態（current row / pinned / preview）を破棄し、誤操作を防止する。
- MUST: Root 変更時は旧 Root 向けに保留中の FileList 上書き確認を破棄する。
- MUST: Root 変更時は query 履歴の参照位置のみリセットし、履歴本体は保持する。
- MUST: IME のスペース/変換確定フォールバック挿入はクエリ末尾固定ではなくカーソル位置へ挿入し、カーソル位置を挿入後位置へ更新する。
- SHOULD: `FLISTWALKER_RESTORE_TABS=1` のときのみ、前回終了時のタブ状態（root/query/filter active tab）を起動時に復元できる。
- MUST: タブ復元が無効、または `--root` / 起動時 query が明示された場合は、従来どおり `Set as default` の root 選択を優先する。
- MUST: `FLISTWALKER_RESTORE_TABS=1` が有効な間は `Set as default` 操作を UI で無効化し、起動 root と競合する永続設定を追加できないようにする。
- SHOULD: タブ復元時は active tab のみ起動直後に再インデックスし、background tab は初回 activate 時に遅延 reindex する。
- MUST: タブバーはドラッグアンドドロップで並び替え可能でなければならず、ドロップ先は既存タブ領域内に限定する。
- MUST: タブ並び替え時は active tab を index ではなく同一タブ実体として維持し、root/query/filter/進行中状態を他タブへ取り違えてはならない。
- SHOULD: 入力デバウンスで連続打鍵時の再描画負荷を抑える。
- MUST: 結果ペインは `Sort` セレクタを持ち、`Score` / `Name (A-Z)` / `Name (Z-A)` / `Modified (New)` / `Modified (Old)` / `Created (New)` / `Created (Old)` を選択できる。

### Preconditions / Postconditions
- Preconditions: GUI モードで起動しインデックス構築可能。
- Postconditions: 利用者がプレビュー確認後に安全に実行/オープンできる。

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

## SP-014 起動時自己更新
### Requirements
- MUST: GUI 起動時に GitHub Releases の最新 version 確認を非同期 worker で実行し、UI スレッドをブロックしてはならない。
- MUST: 現在 version より新しい release が存在する場合、利用者へ更新承認ダイアログを表示する。
- MUST: Windows/Linux の自動更新対象は、現在実行中バイナリに対応する standalone asset と `SHA256SUMS` / `SHA256SUMS.sig` に限定する。
- MUST: ダウンロード後は埋め込み公開鍵で `SHA256SUMS.sig` が `SHA256SUMS` を正しく署名していることを確認し、失敗時は更新を中止する。
- MUST: 署名検証通過後に、`SHA256SUMS` に記載された対象 asset の SHA-256 と一致することを確認し、一致しない場合は更新を中止する。
- MUST: Windows では実行中 EXE を直接上書きせず、一時ディレクトリへ生成した補助 updater を別プロセスとして起動し、旧 EXE 終了後に置換と再起動を行う。
- MUST: Linux では staged binary を一時ディレクトリへ配置し、別プロセスの更新スクリプト経由で置換と再起動を行う。
- MUST: 更新失敗時は既存バイナリを維持し、利用者へ原因を通知する。
- SHOULD: 署名公開鍵が埋め込まれていない開発用ビルドでは、自動更新を manual-only として扱える。
- MUST: macOS では新しい release を検知しても自動置換を試みず、手動更新が必要であることを通知する。
- MUST: 更新ダイアログは、現在提示中の target version を「次のバージョンが出るまで表示しない」として抑止できなければならず、この抑止状態は起動間で保持されなければならない。
- MUST: 抑止済み target version 以下の更新候補は次回起動以降も再表示してはならず、より新しい version を検知した場合のみ再び更新ダイアログを表示しなければならない。
- MUST: `FLISTWALKER_DISABLE_SELF_UPDATE` が truthy な場合、起動時の更新確認、更新ダイアログ表示、更新適用開始を行ってはならない。
- MUST: 手動試験用 override 環境変数（更新 feed URL 差し替え、同一 version 許可、downgrade 許可）は内部検証専用とし、README、release note、配布物、ユーザ向けヘルプへ露出してはならない。
- SHOULD: 更新チェック失敗やダウンロード失敗は通常の検索/操作を妨げない。
- SHOULD: 手動試験のために、更新 feed URL 差し替え、同一 version 許可、downgrade 許可を環境変数で上書きできる。

### Preconditions / Postconditions
- Preconditions: GUI モードで起動し、ネットワーク経由で GitHub Releases へ到達可能。
- Postconditions: 新版が無ければ何も変更せず、新版があれば承認後に検証済みバイナリだけが置換・再起動される。

### Edge / Error
- GitHub API 失敗、タイムアウト、asset 欠落、checksum 不一致は更新失敗として通知し、現行バイナリで継続する。
- 対応外 OS/arch は新版検知のみ行い、自動更新非対応の案内だけを返す。

## SP-012 CI / Release Security Hygiene
### Requirements
- MUST: 通常 CI は Windows/macOS/Linux の release 対象 OS を継続検証する。
- MUST: 通常 CI で `cargo audit` による依存脆弱性検査を実行する。
- MUST: `x86_64-pc-windows-gnu` 向け release build は最終 `flistwalker.exe` に Windows icon resource を含み、Explorer 上で埋め込みアイコンを表示できなければならない。
- MUST: draft release 作成後、macOS notarization は別工程で確認されるまで publish 前提にしてはならない。
- SHOULD: release note / release template / release docs に checksum 検証手順と notarization の扱いを明記する。

### Preconditions / Postconditions
- Preconditions: CI または release workflow を更新する。
- Postconditions: 依存脆弱性検知と release 対象 OS の継続検証が行える。

## SP-011 GUI 回帰テスト計画
### Requirements
- MUST: GUI の主要フロー（起動、検索、選択、実行/オープン、再読込）を手順化する。
- MUST: 仕様IDに対応するテストIDで結果を記録する。
- SHOULD: 主要OS差分（Windows/macOS/Linux）で確認観点を持つ。

### Preconditions / Postconditions
- Preconditions: 対象ビルドが実行可能。
- Postconditions: 回帰実施可否を判定できる記録が残る。
