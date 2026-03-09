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
- MUST: 上記の祖先 FileList 追記後は、親 FileList の mtime を更新前の値へ戻す。
- MUST: 祖先探索や親 FileList 更新で権限不足・読込失敗が発生した場合はエラーを返さず、その時点で追記処理のみを終了する。
- SHOULD: 相対パスはルート起点で絶対化する。
- SHOULD: 重複を除去する。
- SHOULD: include_files/include_dirs が両方有効な場合、種別判定（FILE/DIR）は遅延解決して初期読み込みを優先する。

### Preconditions / Postconditions
- Preconditions: 検索ルートが確定している。
- Postconditions: 有効な既存パスだけで候補集合が構築される。

### Edge / Error
- 空ファイルは候補ゼロ件で正常終了する。
- 読み込み失敗時はエラーを返し、終了コードを非ゼロにする。

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
- MUST: `'` は完全一致条件として評価する。
- MUST: `!` は除外条件として評価する。
- MUST: `^` / `$` は非 regex モードでは「先頭/末尾の隣接文字制約 + ファジー評価」として評価する。
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
- MUST: 外部コマンドはシェル展開なしで実行する。

### Preconditions / Postconditions
- Preconditions: 選択対象がファイル。
- Postconditions: 実行またはオープン要求が OS に渡される。

### Edge / Error
- 起動失敗時はユーザ向けメッセージを返す。
- 拡張子関連付け未定義は失敗として通知する。

## SP-005 フォルダオープン
### Requirements
- MUST: 選択フォルダを OS 既定のファイルマネージャで開く。
- MUST: 存在しないパスには操作しない。

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
- MUST: 非マッチは非表示とし、一致箇所ハイライトを提供する。
- MUST: 複数選択と一括アクションを提供する。
- MUST: Windows では on-demand placeholder と判定できるファイルの本文プレビューを行わず、取得系 I/O による意図しないダウンロードを避ける。
- SHOULD: FileList 読み込み直後の未解決候補は背景解決により FILE/DIR 表示を後追い更新できる。
- MUST: `Ctrl+N` / `Ctrl+P` / `Ctrl+G` / `Esc` は検索窓フォーカス中でも有効である。
- MUST: 選択パスコピーは Windows/Linux では `Ctrl+Shift+C`、macOS では `Cmd+Shift+C` を受理する。
- MUST: query 履歴は全タブ共通で最大 100 件まで保持し、空文字と連続重複 query は履歴保存しない。
- MUST: query 履歴はセッション復元ファイルへ永続化し、後方互換を保ったまま復元できる。
- MUST: `Ctrl+R` で履歴検索モードを開始し、同じ検索欄で query history をファジー検索できる。
- MUST: 履歴検索モード中は履歴検索中であることがわかる表記を行い、結果一覧は履歴候補一覧へ切り替える。
- MUST: 履歴検索モード中は `Enter` / `Ctrl+J` / `Ctrl+M` で選択中の履歴を検索欄へ展開し、`Esc` / `Ctrl+G` でキャンセルして開始前 query へ戻す。
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
- SHOULD: 入力デバウンスで連続打鍵時の再描画負荷を抑える。

### Preconditions / Postconditions
- Preconditions: GUI モードで起動しインデックス構築可能。
- Postconditions: 利用者がプレビュー確認後に安全に実行/オープンできる。

## SP-011 GUI 回帰テスト計画
### Requirements
- MUST: GUI の主要フロー（起動、検索、選択、実行/オープン、再読込）を手順化する。
- MUST: 仕様IDに対応するテストIDで結果を記録する。
- SHOULD: 主要OS差分（Windows/macOS/Linux）で確認観点を持つ。

### Preconditions / Postconditions
- Preconditions: 対象ビルドが実行可能。
- Postconditions: 回帰実施可否を判定できる記録が残る。
