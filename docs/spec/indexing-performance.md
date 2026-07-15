# Indexing and Performance Specification

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
- MUST: Source が FileList のタブで Create File List を実行する場合、新規タブを開かずに同一タブの裏で Walker indexing を実行し、その結果で FileList を作成しなければならない。作成完了後は同じタブを新しい FileList で再インデックスしなければならない。
- MUST: 上記の FileList 作成完了後再インデックスは、元タブが非アクティブに変わっていても元タブに対して継続しなければならない。一方、完了前にその元タブの root が変更されていた場合は、旧 root 向けの再インデックスや `use_filelist` 復帰を行ってはならない。
- MUST: Create File List worker 応答は request_id と requested root の組で相関し、requested root と一致しない stale completion / failure / cancel では pending / in_progress cleanup 以外の follow-up（`use_filelist` 復帰、再インデックス、notice 更新）を行ってはならない。
- SHOULD: 相対パスはルート起点で絶対化する。
- SHOULD: 重複を除去する。
- SHOULD: include_files/include_dirs が両方有効な場合、通常の FILE/DIR は即時確定し、LINK の表示は先行できる一方でリンク先の FILE/DIR 判定は遅延解決して初期読み込みを優先する。
- MUST: include_files/include_dirs が両方有効な FileList ストリーム解析では、パス区切りのプラットフォーム差異は字句変換だけで吸収し、候補選択のための per-line filesystem existence probe を追加してはならない。
- SHOULD: 非 Windows で `\` を含む FileList 行を include_files/include_dirs 両有効の高速経路で読む場合、Windows/WSL 互換を優先して `/` 正規化候補を先に扱ってよい。表示は実装依存とし、初期ストリームで literal `\` filename との曖昧性解消は必須としない。

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
- MUST: Walker の初期ストリームでは、通常ファイル/ディレクトリの種別判定のために per-entry `metadata` / `symlink_metadata` を追加してはならない。`file_type` で確認できる LINK identity は先行表示してよいが、リンク先の FILE/DIR 判定は完了後または必要時の後処理へ遅延しなければならない。
- MUST: Walker は `file_type` で通常 FILE/DIR でも symlink でもない特殊ファイルを LINK に昇格させてはならず、現行のファイル/フォルダ候補から除外しなければならない。
- MUST: Walker で遅延させたリンク先種別判定は、インデクシング完了時または上限打ち切り時（`Truncated`）の後に自動で実行を開始しなければならない。解決済み OTHER または解決不能の終端状態を未解決として再キューしてはならない。
- MUST: Walker backend は adaptive のみを使用し、jwalk backend への runtime config 切替口を持ってはならない。
- SHOULD: adaptive walker backend は developer-only config の `walker_adaptive_initial_limit` と `walker_adaptive_max_limit` により、初期同時 read_dir 数と最大同時 read_dir 数を別々に指定できる。未指定時の最大値は論理コア数の半分（端数切り上げ、最低 1、既定上限 8）とし、初期値は最大値の半分（端数切り上げ、最低 1）とする。
- SHOULD: adaptive walker backend の自動調整は、単発の read_dir 遅延ではなく、短いサンプル窓の throughput を比較して行う。探索方向が未確定の場合は、窓内の完了件数 / 経過時間が前窓から有意に改善した場合に limit を 1 段増やし、悪化した場合に 1 段減らし、誤差帯では維持する。探索方向が確定した後は、改善または誤差帯では同方向へ 1 段進み、悪化した場合は方向を反転しなければならない。
- SHOULD: Walker の summary metrics は `adaptive_limit_final` に加えて `adaptive_limit_avg` と `adaptive_limit_change_count` を出力し、再測定時に平均的な並列度と揺れ幅を確認できるようにしなければならない。`adaptive_limit_avg` は実ワーク中の時間加重平均を主対象としつつ、終了時の停止・join 尾を少量含みうることを明示しなければならない。
- SHOULD: `walker_threads` と `walker_backend` が既存 runtime config file に残っている場合、読み込み時に削除して以後の起動へ持ち越してはならない。
- SHOULD: `walker_adaptive_initial_limit` と `walker_adaptive_max_limit` は developer-only tuning 項目として扱う。公開向け設定として拡張してはならない。
- SHOULD: adaptive walker backend は最大 worker 数が 1 の場合、channel / condvar / 複数 worker を使わない serial fast path で走査できること。
- MUST: adaptive walker backend は Windows の Explorer で通常非表示となる互換用 junction（Hidden + System + ReparsePoint）を候補化してはならない。また、reparse point directory はリンク自体を候補化できても、リンク先へ再帰してはならない。
- SHOULD: developer-only metrics が有効な場合、Walker は indexing request の完了・打ち切り・キャンセル・失敗時に bounded summary を 1 回だけ診断ログへ出力し、per-entry / per-directory の継続ログを出してはならない。
- SHOULD: developer-only metrics の `walker_metrics_log_path` が手動指定された場合、Walker は release GUI build でも console/stderr に依存せず、同じ bounded summary を指定ファイルへ追記できる。
- SHOULD: 循環リンクを避ける。
- SHOULD: 空クエリ時は新規バッチを即時に一覧へ反映し、非空クエリ時は UI 負荷を抑えるため間引き更新する。

### Preconditions / Postconditions
- Preconditions: `FileList.txt` / `filelist.txt` が使用されない。
- Postconditions: 既存パスのみの候補集合を返す。

### Edge / Error
- 権限不足ディレクトリはスキップし、全体処理は継続する。
- 候補ゼロ件でも正常終了する。

## SP-007 性能
### Requirements
- SHOULD: 10万件規模で検索応答 100ms 未満を目標とする。
- SHOULD: インデックス構築中も UI 操作が停止しない。
- SHOULD: 正規表現クエリはクエリ単位でコンパイルし、候補ごとの再コンパイルを避ける。
- SHOULD: プレビューキャッシュは上限管理し、長時間利用でメモリが無制限に増加しない。

### Preconditions / Postconditions
- Preconditions: 候補集合が利用可能。
- Postconditions: 計測可能な遅延特性を示せる。
