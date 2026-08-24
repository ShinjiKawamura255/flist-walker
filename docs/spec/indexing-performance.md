# Indexing and Performance Specification

## SP-001 FileList 優先読み込み
### Requirements
- MUST: FileList 優先モード有効時のみ、検索ルート直下で `FileList.txt` と `filelist.txt` を検出する。
- MUST: 両方ある場合は `FileList.txt` を優先する。
- MUST: 上記2名が無い場合、`filelist.txt` と大文字小文字のみ異なるファイル名（例: `FILELIST.TXT`）をルート直下から検出対象に含める。
- MUST: FileList 検出中に同一 tab の新しい request_id が発行された場合、root 直下の discovery は次の候補確認前に中断し、旧 request は `Canceled` として終端しなければならない。
- MUST: 空行と `#` コメント行を無視する。
- MUST: root と階層の FileList は UTF-8 として読み込む。byte offset 0 の UTF-8 BOM (`EF BB BF`) は 1 個だけ任意とし、候補文字列には含めない。UTF-8 BOM の有無、LF/CRLF、ASCII/非 ASCII path の違いで候補解釈を変えてはならない。
- MUST: UTF-16LE/BE BOM、NUL byte、不正または途中切れ UTF-8 を locale、置換文字、または文字コード推測で復号してはならない。FileList path と `expected UTF-8 (optional BOM)`、不正 byte offset または拒否理由を含むエラーで失敗しなければならない。
- MUST: optional BOM と CR/LF を除く 1 論理行の encoded payload は 1 MiB 以下とする。上限超過は候補化せず、FileList path と上限を含むエラーで失敗しなければならない。
- MUST: encoding preflight と line parse は 64 KiB 以下の chunk ごとに supersede/cancel を確認しなければならない。安定した不正 root FileList は callback/候補を 0 件に保ち、不正な階層 FileList はその local replacement 完成前に親由来 subtree を変更してはならない。
- MUST: 階層 FileList 展開は、読み込み済み候補内でファイル名が `FileList.txt` / `filelist.txt` に完全一致するエントリのみを対象とする。
- MUST: 階層 FileList 展開中も supersede（新しい request_id）で中断できること。
- MUST: FileList 作成時は、祖先ディレクトリ直下の既存 `FileList.txt` / `filelist.txt` へ作成済み子 FileList の参照を重複なく追記できる。
- MUST: 祖先ディレクトリ直下の既存 FileList へ追記が発生しうる場合、Create File List 実行前に利用者確認を要求する。
- MUST: Create File List の保留状態（overwrite 確認、祖先追記確認、Walker 利用確認、index 完了待ち）では、GUI から明示的にキャンセルできる。
- MUST: Create File List 実行中は status panel にキャンセル導線を表示し、利用者が再実行ボタンや root 変更へ頼らず中断要求できる。
- MUST: Create File List のキャンセル要求後、root 直下の最終置換と祖先 FileList 追記は開始前なら実行してはならない。
- MUST: 上記の祖先 FileList 追記後は、親 FileList の mtime を更新前の値へ戻す。
- MUST: 祖先 FileList の参照重複判定と追記も同じ UTF-8/optional-BOM/NUL/行上限契約を使わなければならない。先頭 BOM は最初の参照文字列に含めず、拒否対象の親 FileList は書き換えず、既存どおりその時点で祖先追記だけを終了する。
- MUST: 祖先探索や親 FileList 更新で権限不足・読込失敗が発生した場合はエラーを返さず、その時点で追記処理のみを終了する。
- MUST: Source が FileList のタブで Create File List を実行する場合、新規タブを開かずに同一タブの裏で Walker indexing を実行し、その結果で FileList を作成しなければならない。作成完了後は同じタブを新しい FileList で再インデックスしなければならない。
- MUST: 上記の FileList 作成完了後再インデックスは、元タブが非アクティブに変わっていても元タブに対して継続しなければならない。一方、完了前にその元タブの root が変更されていた場合は、旧 root 向けの再インデックスや `use_filelist` 復帰を行ってはならない。
- MUST: Create File List worker 応答は request_id と requested root の組で相関し、requested root と一致しない stale completion / failure / cancel では pending / in_progress cleanup 以外の follow-up（`use_filelist` 復帰、再インデックス、notice 更新）を行ってはならない。
- MUST: FileList create は root detection precedence で選択した既存 root target を再利用し、target が無い場合だけ `FileList.txt` を新規 target とする。canonical names と case variant が併存する fixture でも write plan は決定論的で、lower-priority target を暗黙に上書きしてはならない。
- MUST: FileList create は全 contents と root/ancestor target metadata を commit 前に precompute する。既存 root target は `--overwrite-filelist` または interactive overwrite consent 無しに 0 write で拒否し、ancestor は root-only を既定とし、propagation consent は precomputed ancestor target set だけを認可する。
- MUST: replacement の直前ごとに cancellation を確認する。write/read error は failure とし、partial failure/cancel では committed target の全てを rollback するよう試行する。success は exit 0、commit 前 cancel または rollback 完了済み clean cancel は exit 130、write/read/rollback error は cancel 起因でも exit 1 とする。committed/failed/rolled-back/rollback-failed display path は stderr にのみ報告し、stdout は空にする。cross-file crash atomicity は保証しない。
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
- UTF-8 BOM だけのファイルは候補ゼロ件で正常終了する。
- 読み込み失敗時はエラーを返し、終了コードを非ゼロにする。
- 安定した拒否対象 root FileList は valid prefix を候補として返さない。FileList が validation と parse の間に同一 handle 上で in-place 更新された場合、valid UTF-8 の混在 snapshot までは検知保証しないが、各 parse chunk の strict UTF-8/NUL/行上限確認は維持する。
- 利用者が祖先追記確認を拒否した場合、root 直下の FileList 作成だけを継続し、祖先追記は行わない。
- 利用者が Create File List をキャンセルした場合、進行中 request は `Canceled` として扱い、成功/失敗通知や再インデックスを発生させない。
- transaction panic は worker-owned report を使って rollback を試行し、成功として扱わない。force-kill/crash 後の cross-file atomicity は本仕様の対象外とする。

### Regression Guard: single-discovery nested FileList path basis

- Scenario: root直下のFileListをcancel可能な探索で一度だけ検出した後、rootだけをcanonicalizeすると、Windowsでは通常pathとverbatim pathが混在し、nested FileListがroot配下と判定されない。
- Expected Behavior: 検出済みFileListとrootを同じcanonical path basisへ正規化し、新しいnested FileListによるsubtree置換とdepthごとのoverrideを維持する。
- Non-goals: FileList discoveryの再実行、候補行ごとのcanonicalize、nested FileListのmtime優先規則の変更。
- Related Tests: TC-030、`build_index_overrides_subtree_with_newer_nested_filelist_regression`、`build_index_applies_newest_filelist_per_depth_regression`。
- Notes for Future Changes: caller-owned discovery結果をbuild APIへ渡す場合、rootとの比較前に両者のpath basisを一致させる。

### Regression Guard: startup/refresh FileList discovery freshness polarity

- Scenario: index worker の freshness callback（`true=current`）を FileList discovery の cancellation callback（`true=cancel`）へ極性変換せず渡すと、正常な最新 startup/refresh request が discovery 前に superseded となり、GUI は応答したまま Source None / Entries 0 に留まる。
- Expected Behavior: 明示 root の小規模な local FileList は production worker から bounded time 内に `Started(FileList)`、実 entry、`Finished(FileList)` を返し、app startup と Refresh Index の双方が Source と Entries を終端状態へ反映する。
- Non-goals: UI thread での同期 discovery、正常な stale request の cancel 緩和、network root 向けの完了時間保証。
- Related Tests: TC-152、`tc_152_native_filelist_request_starts_and_finishes_within_deadline_regression`、`tc_152_startup_and_refresh_settle_filelist_source_and_entries_regression`。
- Notes for Future Changes: freshness predicate を cancellation API へ渡す境界では極性を明示し、liveness だけでなく Source と Entries の settlement を検証する。

### Regression Guard: TUI initial FileList discovery ownership

- Scenario: interactive CLIがterminalを所有してからrequired FileListの不存在を検出すると非TTY環境では契約errorよりterminal errorが先に出る。また、Autoをmain threadで探索するとUI cancellationが届かず、preflightと初回workerが同じrootを重複探索する。明示Walkerがindex完了時にFileList有無を既知扱いすると、F6のoverwrite判定も誤る。
- Expected Behavior: 既定Autoは初回から`WorkerOwned`とし、index worker内のcancel predicateで単一discoveryを行う。`--source filelist`だけはfail-fastのためterminal/event loop開始前にroot直下を同期確認する。この限定preflightはterminal cancellationをまだ受け取れないbounded例外であり、成功結果を最初のworker requestへ移譲して再探索しない。`--source walker`は初回index前のFileList discoveryを0回とし、index後も存在状態をunknownに保つ。F6時はcancel可能な専用workerで遅延discoveryし、既存ならoverwrite確認を完了するまでfresh snapshotを生成せず、未存在ならcreateへ進む。discovery中のroot switch/output確定はworkerをcancelし、success/cancel/failureのいずれでもrequest identity確認後に一度だけ適用してpending intentとconfirmationをclearする。stale responseは新しいactive requestとintentを変更してはならない。
- Non-goals: Auto/FileListの後続refreshにおけるfresh discovery、F6実行時のFileList作成確認、GUI indexing ownershipの変更。
- Related Tests: TC-162、TC-166、`tc_162_startup_discovery_ownership_is_source_specific_regression`、`tc_162_initial_filelist_discovery_is_consumed_without_rescan_regression`、`tc_162_explicit_walker_performs_zero_filelist_discovery_regression`、`tc_166_walker_f6_lazy_discovery_confirms_before_snapshot_regression`、`tc_006_source_controls_filelist_and_walker_selection`。
- Notes for Future Changes: preflight結果はfirst requestだけが消費し、後続requestはworker-owned discoveryへ戻す。

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
- MUST: TC-156 の 10万件 fixture は計測区間外で構築し、release mode で cold（空 prefix cache）と warm（cacheable query を seed 後に単調延長）を5回以上計測して median と maximum を報告する。
- MUST: TC-156 は selective fuzzy、multi-token AND、exact、inverse/exclusion、anchor、OR、regex の sparse/dense shape と unknown-kind `ext:` shape を含め、cold/warm の path・score・order 一致と warm の評価候補削減を検証し、各 shape の maximum が 250ms 未満であることを weekly regression ceiling として検証する。
- MUST: TC-185 は計測区間外で生成した exactly 1,000,000 件の固定 `Entry::file` 候補に対し、selective-fuzzy と dense-fuzzy の固定 query を空 cache から各 7 回実行する。nearest-rank の p50/p95/p99、候補・評価・match・result 件数を stable `tc_185` label で記録し、各 repetition の件数・path・score・order 一致を検証する。
- MUST: TC-185 の RSS は `before_fixture`、`after_fixture`、`peak_search`、候補・result・cache drop 後の最大 1 秒 quiescence を経た `after_drop_quiescence` を記録する。取得不能は明示し、値は hosted baseline 蓄積中の observational signal として扱い hard threshold を設定しない。
- MUST: prefix cache は候補 snapshot の live identity と、`ignore_case`、`prefer_relative`、正規化 root を key に含め、異なる検索意味論の候補集合を再利用してはならない。
- MUST: unknown-kind 候補の `ext:` 評価は file projection と directory projection が異なる候補だけ filesystem kind を解決し、両 projection が同じ候補へ per-candidate probe を行ってはならない。
- MUST: interactive CLI は candidate batch snapshot と root/ignore scope が同一の連続検索で同じ projected entry snapshot を再利用し、prefix cache の live identity を維持する。scope 変更時は再構築する。
- MUST: GUI/TUI の新しい同一対象検索 request は旧 request を supersede し、候補評価中も最大 256 候補間隔で協調キャンセルを確認する。cancel 済みの partial result を publish してはならない。
- MUST: weekly perf workflow は TC-156 と TC-185 を同じ固定 entrypoint から明示実行する。
- SHOULD: プレビューキャッシュは上限管理し、長時間利用でメモリが無制限に増加しない。

### Preconditions / Postconditions
- Preconditions: 候補集合が利用可能。
- Postconditions: 計測可能な遅延特性を示せる。

## SP-021 候補収集の最大深度

### Requirements
- MUST: `max_depth` は検索 root 自体を depth 0、root 直下のファイルまたはフォルダを depth 1 とする root 相対 path component 数の上限として解釈する。root 自体は通常の候補に含めない。
- MUST: 上限未指定は無制限とし、既存の候補集合を変更しない。明示値は 1 以上の整数とし、0、負数、整数以外は CLI では indexing 前の引数エラーにする。
- MUST: depth N のフォルダ自体は候補条件を満たす場合に含められるが、Walker はその配下へ再帰してはならない。結果生成後の除外だけで済ませず、不要な `read_dir` を開始しない。
- MUST: FileList は root FileList と有効な nested FileList の各候補を同じ root 基準で字句的に判定し、上限を超える候補を stream callback、種別解決、nested override discovery より前に除外する。depth 判定のために canonicalize、metadata、existence probe を追加してはならない。
- MUST: 明示上限があるとき、root 相対深度を字句的に定義できない absolute path、root 外 path、または `..` により root 外へ出る FileList entry は候補から除外する。上限未指定時の既存 FileList 解釈は維持する。
- MUST: `--source auto|filelist|walker` は同じ深度境界を使用する。深度制限は ignore/query/sort/`--limit` より先に候補収集へ適用する。
- MUST: `--max-depth N` は batch CLI と interactive CLI の起動条件として利用でき、GUI 起動時は active tab の初期値として利用できる。`--max-depth` は `--create-filelist` と競合し、FileList 作成用 fresh Walker snapshot を部分化してはならない。
- MUST: GUI は active tab の現在値を上部検索オプション行の `Folders` と `Preview` の間に `Depth: All` または `Depth: ≤ N` として常時表示する。control は popup draft を `Apply` したときだけ active tab の値を変更して非同期再indexを開始し、古い response は request identity で破棄する。
- MUST: GUI の max depth は tab-local pure-search state として query変更、refresh、root変更、source変更、tab切替を越えて維持する。既存他タブを変更せず、新規タブは無制限で開始する。session tab restore が有効な場合は保存・復元する。
- MUST: preset は max depth を所有し、GUI editor と picker detail で `All` または上限値を表示する。適用は active tab へ値をコピーし、他タブを変更しない。別 preset 適用または UI 変更まで値を維持し、preset の後続編集・削除で適用済み tab を変更しない。旧 preset の欠落 field は無制限として扱う。

### Preconditions / Postconditions
- Preconditions: 検索 root と候補 source が確定している。
- Postconditions: 収集された全候補の root 相対 depth は明示上限以下であり、上限未指定時は従来どおりである。

### Edge / Error
- symlink、junction、reparse point 自体の depth は字句 path で数え、既存の非再帰 policy を変更しない。
- 権限不足や消失 path の扱いは SP-001 / SP-002 の既存契約を維持する。
