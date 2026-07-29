# Search, Actions, CLI, Errors, and Testability Specification

## SP-003 ファジー検索
### Requirements
- MUST: 通常語はファジー検索（AND 条件）で評価する。
- SHOULD: 通常語の各トークンは、subsequence 一致だけの候補よりもリテラル一致する候補を高く順位付けする。
- MUST: `'` は完全一致条件として評価する。
- MUST: 空白区切りで同じ unanchored 完全一致 token が複数回指定された場合、その token の指定回数以上のリテラル出現を含む候補だけを一致として扱う。
- MUST: `!` は除外条件として評価する。
- MUST: `^` / `$` は非 regex モードでは「先頭/末尾の隣接文字制約 + ファジー評価」として評価する。
- MUST: regex モードでも include token が regex 構文（例: `[](){}.*+?\\`）を含まない plain token の場合は、非 regex モードと同じファジー条件として評価する。
- MUST: regex モードで include token が regex 構文を含む場合のみ、その token を regex として評価する。
- MUST: token 内の `|` は OR alternative として評価し、空 alternative は別の有効 alternative がある場合に無視する。各 alternative の先頭にある `'` はその alternative だけを完全一致にする。
- MUST: 検索結果のハイライトは search と同じ query interpretation を用い、exact / include / exclude / anchor / OR の解釈差を生じさせてはならない。
- MUST: query は検索要求ごと、または GUI highlight cache scope ごとに1回だけ compile し、候補ごと・表示行ごとの再 parse / regex compile を行ってはならない。
- MUST: 同一候補に対する match/visibility、score、highlight span は同じ compiled query と clause matcher から導出し、search 完了後に別 evaluator で可視性を再判定してはならない。
- MUST: rank-only 評価は全候補分の highlight span を割り当てず、highlight span が必要な候補だけ同じ clause matcher を span 付きで評価する。
- MUST: 既存の query/search/highlight 公開 API は、空 query、無効 regex、相対/絶対表示、case mode を含む既存の戻り値・エラー契約を維持する。
- MUST: 公開 `has_visible_match` adapter は positive term の成立だけを投影し、除外 term を判定に含めない。公開 highlight adapter は各 positive term の一致 span を独立に投影し、別の positive term の不一致または除外 term の一致を理由に、成立済み span を消してはならない。authoritative search evaluation だけが全 positive term と除外 term を組み合わせた最終 truth を返す。
- SHOULD: 厳密な prefix/suffix 一致が必要な場合は regex モードを使える。
- MUST: 上位 `limit` 件を関連度順で返す。
- SHOULD: 大文字小文字差を緩和する。

### Preconditions / Postconditions
- Preconditions: 候補集合が構築済み。
- Postconditions: スコア降順結果を返す。

### Edge / Error
- クエリ空文字または `limit=0` は空結果を返す。
- 正規表現モードで無効パターンは結果を返さず、GUI ではエラー理由を通知する。

### Regression Guard: Public positive projections
- Scenario: shared evaluator 移行で `main !src` の公開 visibility adapter と `main zzzz` の公開 highlight adapter が authoritative full-query truth を適用し、従来の positive projection を失う。
- Expected Behavior: visibility は `main` の成立を返し、highlight は成立した `main` の span を返す一方、authoritative search は除外または未成立 term を含む候補を結果から除く。
- Non-goals: search の除外条件、AND 条件、score/order、operator syntax は変更しない。
- Related Tests: `tc_155_regression_visible_match_remains_a_positive_term_projection`、`tc_155_regression_highlight_remains_a_partial_positive_projection`。
- Notes for Future Changes: 公開 projection adapter を full `evaluate` へ置換せず、共有 clause primitive から projection と authoritative truth を分けて導出する。

## SP-004 ファイル実行/オープン
### Requirements
- MUST: 実行可能ファイルはプロセス起動する。
- MUST: 非実行ファイルは既定アプリでオープンする。
- MUST: Windows の一般 `.ps1` は既定動作で直接実行してはならず、既定アプリでオープンしなければならない。
- MUST: 外部コマンドはシェル展開なしで実行する。
- MUST: Windows で既定アプリ起動やフォルダ表示へパスを渡す際、`&` などの特殊記号をシェル解釈させず、リテラルなパスとして OS へ引き渡す。
- MUST: UI は filesystem I/O を行わない字句的 precheck だけを実施し、captured root からの逸脱を証明できる入力だけを `Reject` とする。相対パス、大小文字、prefix、非 UTF-8、drive-relative、rooted-without-drive、UNC、verbatim prefix など、字句情報だけでは安全性を確定できない入力は `Defer` として worker へ渡す。
- MUST: action request は、選択内容と同時に取得した trusted root を worker へ渡す。インデクシング経路には root 配下判定を追加しない。
- MUST: worker は OS への要求を発行する前に、trusted root とすべての effective target を解決し、解決済み path component によって root 配下であることを権威的に検証する。文字列 prefix、lossy 変換、区切り文字置換、手動の大小文字変換を認可判断に使用してはならない。
- MUST: direct action の effective target は選択対象、open-containing-folder の effective target は通常ファイルまたは file link の字句的な親、ディレクトリまたは directory link/junction 自身とする。解決後の effective target が解決済み root 配下でなければ拒否する。
- MUST: 複数選択では、すべての effective target の解決と認可が成功するまで OS backend を一度も呼び出してはならない。1 件でも解決不能または root 外なら要求全体を拒否する。
- MUST: worker は各 backend 呼び出しの直前にも対応する raw effective target を再解決し、root 配下判定を繰り返す。再検証が途中で失敗した場合は残りを実行せず、すでに開始した件数を含む partial completion として通知し、完了済み外部アクションを rollback したと主張してはならない。
- MUST: shared action request は trusted root、current-row selection snapshot、request identity、cancellation token を保持する。whole-request の事前認可成功後、単一 backend 呼び出しの直前に freshness/cancel 確認と再認可を行う。root switch または exit cancellation の観測後、新しい backend 呼び出しを開始してはならない。開始済み OS action は不可逆として扱う。
- MUST: OS backend へ渡す path は最後に認可した解決済み execution path とする。成功/失敗通知は利用者が選択した display path または effective display path を使い、拒否した root 外の解決先を表示してはならない。
- MUST: root 外パスは一覧表示されていても実行/オープンを拒否し、利用者へ通知する。
- MUST: UNC root を検索 root とする場合も、解決済みの同一 root 配下は許可し、別 share または root 外は拒否する。

### Preconditions / Postconditions
- Preconditions: 1 件以上の対象と、その選択時点の trusted root が action request に含まれる。
- Postconditions: 全対象の事前認可が成功した場合だけ、各対象の直前再検証後に解決済み execution path が OS へ渡される。事前認可失敗では backend 呼び出しは 0 件となる。

### Edge / Error
- 起動失敗時はユーザ向けメッセージを返す。
- 拡張子関連付け未定義は失敗として通知する。
- trusted root または effective target を解決できない場合は fail closed とする。
- open-containing-folder の対象種別を metadata で確定できない場合、壊れた link、未対応の特殊種別は親フォルダへ推測変換せず fail closed とする。worker は事前認可時に使った各 source path の種別と effective target を backend 呼び出し直前に再導出し、変更されていれば残件を停止する。
- 別ドライブ、別 UNC share、`..` 解決後に root 外となるパス、root 内に置かれた link/junction の解決先が root 外となるパスは拒否する。
- open-containing-folder で root 内の file link を選び、その link の字句的な親が root 内に解決される場合は、file link の解決先が root 外でも親フォルダの表示を許可する。file link の解決先そのものを開いてはならない。
- 最終再検証と OS による利用の間に filesystem object が変化する TOCTOU は完全には排除できない。最後に再解決した path を backend へ渡して露出時間を最小化し、保証範囲を過大に表現しない。
- OS leaf の詳細エラーは内部 trace に記録し、利用者向け失敗通知へ canonical execution path や OS エラー本文を転記してはならない。通知は対応する display path と、blocked / failed / partial の結果だけを示す。

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
- MUST: `--check-update` と `--update` は `--cli` を必要としない排他的な CLI 操作とし、query、検索、FileList、action、GUI/TUI 起動と組み合わせてはならない。
- MUST: `--check-update` は更新を適用せず、最新版、更新候補、更新確認無効、または失敗を英語で報告する。更新候補がある場合は `flistwalker --update` を手動実行する案内を表示する。
- MUST: `--update` は利用者による明示的な更新承認として扱う。Windows/Linux の自動更新対応 bundle だけを既存の検証・transaction 経路へ渡し、manual-only platform では release URL を英語で表示して非ゼロ終了する。
- MUST: `--cli` 指定時は GUI を起動せず標準出力に結果を表示する。
- MUST: `--root` と `--limit` を受理し、既存の `--cli [QUERY] --root ... --limit ...` invocation を維持する。本仕様では subcommand を追加しない。
- MUST: クエリ未指定時は候補一覧を `limit` 件以内で表示する。
- MUST: CLI の `--limit` は実効値を追加で 1000 件へ丸めてはならない。
- MUST: batch CLI は `--sort score|name-asc|name-desc|modified-desc|modified-asc|created-desc|created-asc|size-desc|size-asc` を受理し、既定を `score` とする。全 match set を sort してから `limit` を適用し、`limit=0` は target 0 件とする。
- MUST: batch CLI は `--action print|open|reveal` を受理し、既定を `print` とする。`--action-all` は `open` / `reveal` のみで有効とし、non-print action の post-sort/post-limit target が複数で `--action-all` が無い場合は backend 呼び出し前に拒否する。既定 target は 1 件、`--action-all` は全 target である。
- MUST: `print` は既存 path-only stdout framing を維持する。non-print action は stdout に result path を書かず、progress、diagnostic、partial summary を stderr に書く。non-print action と `--absolute` または `--print0` の組合せは argument error とする。
- MUST: action/root option の argument または組合せ error は exit 2、authorization/executor/partial failure は exit 1、cancellation は exit 130 とする。no match は既存どおり exit 0、`--fail-no-match` 指定時だけ exit 1 とする。preflight authorization failure は backend 呼び出し 0 件とする。
- MUST: root selector は `--root PATH`、`--use-default-root`、`--saved-root INDEX` の高々 1 つとする。saved-root index は `--list-saved-roots` の one-based order とし、selector 無しは current-directory behavior を維持する。無効 default/index は indexing 前に exit 2 とする。
- MUST: `--list-saved-roots` は exclusive batch operation とし、indexing/action/write を行わない。通常は one-based index と absolute stored path を newline で、`--print0` では path-only record を NUL で出力する。
- MUST: `--create-filelist` は query search/action/listing と排他的であり、non-default search/filter/sort option を拒否する。`--overwrite-filelist` と `--propagate-ancestors` は `--create-filelist` を必要とし、root selector は有効である。
- MUST: batch CLI の既定出力は root 相対 path の改行区切りとし、スコアや ANSI 装飾を付加してはならない。既定の一致なしは stdout 空・exit 0 とする。
- Compatibility: query 指定時に `[score] absolute-path` を出力していた旧形式は、script-safe な単一path形式へ意図的に置き換える。旧 invocation は維持するが旧出力 framing は維持せず、絶対pathが必要なconsumerは `--absolute` へ移行する。score出力はCLI契約に含めない。
- MUST: batch CLI は `--absolute`、`--print0`、`--fail-no-match`、`--type all|file|folder`、`--regex`、`--case-sensitive`、`--source auto|filelist|walker`、`--ignore-file PATH`、`--no-ignore`、`--progress` を受理する。`--absolute` は path 形式だけ、`--print0` は delimiter だけを変更し、`--fail-no-match` は一致なしを exit 1 にする。
- MUST: batch CLI の `--progress` は indexing 開始、候補件数と所要時間、全一致件数・返却件数と検索所要時間を stderr へ出力し、stdout framing を変更してはならない。path 出力は全出力を別 buffer に複製せず、上限付き writer で逐次書き込む。
- MUST: `--ignore-file` と `--no-ignore` は同時指定を拒否する。`--source filelist` は root 直下に FileList がなければ非ゼロ終了し、`auto` は FileList 優先、`walker` は FileList を使用しない。
- MUST: CLI 専用 option は `--cli` を必要とし、`--interactive` 単独指定は GUI を起動せず引数エラーとする。
- MAY: `--cli --interactive` でインタラクティブ CLI を起動する。
- MUST: interactive CLI は `--root`、`--use-default-root`、`--saved-root` を起動 root として受理し、`--sort` を初期 sort、`--no-ignore` を初期 Ignore 無効状態として反映する。`--no-ignore` でも読み込んだ ignore terms は保持し、TUI で Ignore を再度有効化したときに再読込なしで適用する。batch 専用の `--progress` と `--fail-no-match` は interactive との組合せを引数エラーにする。
- MUST: interactive CLI は標準入力と標準エラー出力の双方が TTY でない場合、raw mode や ANSI 描画を開始せず非ゼロ終了する。標準出力は TTY を要求せず pipe/redirect を許可する。
- MUST: interactive CLI の alternate screen、cursor、status/help、検索結果描画は標準エラー出力だけを使用し、選択結果だけを terminal 復旧後に標準出力へ出力する。
- MUST: interactive CLI は更新確認を入力ループ外で非同期実行し、新しい version を検知した場合に `Update available: v<version> — Run flistwalker --update after exiting` を英語で表示する。この通知は更新を開始せず、更新確認失敗も検索、入力、終了を妨げてはならない。
- MUST: インタラクティブ CLI は query 入力、上下移動、`Enter` による選択結果の標準出力、`Esc` / `Ctrl-C` による終了を提供する。
- MUST: `Esc` / `Ctrl-C` は worker cancellation を要求し、terminal 復旧後に選択結果を出力せず exit 130 とする。batch CLI の Ctrl-C も FileList/walker の cancellable index path を停止して exit 130 とする。
- MUST: `Tab` は現在行の pin を切り替え、pin がある場合の `Enter` は現在の filter 結果に含まれない pin も pin 順で出力する。選択可能な結果も pin もない `Enter` は終了してはならない。
- MUST: query editor は挿入 cursor、左右移動、Home/End、Backspace/Delete、paste を扱い、上下/PageUp/PageDown と key repeat で結果を移動できる。terminal resize 後は表示可能行数から viewport を再計算する。
- SHOULD: インタラクティブ CLI は GUI と同じ query 解釈による一致文字のハイライトを表示する。
- MUST: インタラクティブ CLI の index/search は UI 入力ループと別スレッドで実行し、query 文字列だけでなく単調増加 request identity により最新でない検索応答を表示してはならない。結果更新時は選択中 path が残る限り同じ path を維持する。
- SHOULD: インタラクティブ CLI は状態変更時だけ端末を再描画し、候補全件を毎回描画してはならない。
- MUST: interactive CLI の各描画フレームは terminal synchronized update で開始・終了を囲み、全画面消去を含むフレーム途中の状態を表示してはならない。フレームは端末出力へ送る前にメモリ上で完成させ、終了 command は payload 書き込み失敗時も best-effort で送信する。
- MUST: terminal session は raw mode、alternate screen、cursor 非表示、bracketed paste の成立状態を個別に追跡し、setup 途中失敗、event/draw error、正常終了、cancel、unwind で成立済み状態だけを逆順に best-effort 復旧する。選択結果は guard 解放後だけ出力する。
- SHOULD: interactive indexing の増分 batch は検索再実行を throttle/debounce し、結果更新のたびに current row を先頭へ戻してはならない。
- MUST: TUI normal state は Enter で current/pins を terminal 復旧後に出力し、Esc で exit 130、Ctrl-C で worker cancel と exit 130、Tab/Shift+Tab で pin toggle とする。runtime config の `tab_pin_moves_to_next_row=true` では pin toggle 後に次行へ進み、`false` では current row を維持する。`emacs_keybindings_enabled=true` では `Ctrl+N` / `Ctrl+P`、`Ctrl+V` / `Alt+V`、`Ctrl+I`、`Ctrl+J` / `Ctrl+M`、`Ctrl+G` / `Ctrl+R` と normal query / history filter の editing chords を GUI と同じ意味で有効にし、`false` ではこれらを TUI action として処理しない。`Ctrl+O` と `Shift+Enter` は pins に関わらず current row だけを action target とする。
- MUST: TUI history overlay は Enter で highlighted history query を適用し、Esc で draft を復元して閉じ、Ctrl-C で全体を exit 130 とする。help overlay は Enter/Esc で閉じ、side-effect key を dispatch してはならない。options/sort/root overlay は Enter だけで highlighted choice を適用し、Esc は旧 state を保存して閉じ、Ctrl-C は exit 130 とする。`emacs_keybindings_enabled=true` のときだけ各 overlay の `Ctrl+G` を Esc と同義にし、help と各 overlay の操作案内へ有効な Emacs 風 shortcut だけを掲載する。
- MUST: TUI FileList confirmation は confirm choice に加えて overwrite confirmation を要求し、active confirmation 中は action/root/refresh dispatch を許可しない。preview は width 100 column 以上で既定有効、狭幅では collapse し、`Alt+P` で toggle し、I/O は worker-only とする。
- MUST: non-FileList worker busy 中も navigation/query と request-identity による latest request supersession を許可し、stale response は state を変更してはならない。root/source transition は root-scoped current/pin/preview state を clear してから新結果を受理する。
- MUST: TUI index worker は選択 root の metadata error、missing、non-directory を `IndexFailed` として返し、空 index の正常完了へ変換してはならない。Walker source は GUI index worker と同じ adaptive traversal と runtime config の adaptive initial/max limit を使い、`walker_max_entries` 件で候補投入を停止して request identity 付き truncation 応答を terminal finish より先に返さなければならない。最新 request の cap notice は検索結果更新後も次の index 開始まで status に保持する。増分候補は immutable batch として共有し、入力ループ上の batch 追加で既存 path 全件を copy-on-write 複製してはならない。
- MUST: TUI event loop は worker 応答を iteration ごとの固定件数まで反映して key polling を継続し、backlog が残る場合だけ待機時間を 0 にして次 iteration へ進む。
- MUST: FileList active 中は Enter/Esc/Ctrl-C/root switch を pending intent として記録して cancel を要求し、transaction settlement 前に output、root switch、terminal return を行ってはならない。intent priority は sticky `CancelExit` > latest `SwitchRoot(path)` > `SelectOutput` とする。generic 250ms detach path は FileList worker に適用してはならない。FileList 作成用 snapshot は表示中 TUI index や `walker_max_entries` cap を再利用せず、walker-only/all-kinds の完全な fresh snapshot とする。
- MUST: FileList worker は panic-contained transaction report を返す。panic/channel disconnect/missing terminal response は join 後 failed settlement として合成し、selection/root intent を success として再開してはならない。rollback/report failure は recovery path を表示し、selection/root intent を TUI 内で明示 retry/exit まで保持する。ただし `CancelExit` は terminal を復旧して exit 1 とする。

### Preconditions / Postconditions
- Preconditions: CLI モードで起動される。
- Postconditions: batch/interactive の結果だけが指定 delimiter で標準出力へ出力される。進捗、TUI、診断、エラーは標準エラー出力を使用し、interactive 終了後は terminal state が復旧済みとなる。

### Edge / Error
- 引数契約違反、root/source/ignore/regex/index error は非ゼロ終了する。
- `--fail-no-match` なしの一致なしは正常終了、指定時は exit 1 とする。
- stdout が pipe でも stdin+stderr が TTY なら interactive mode を許可する。stdin または stderr が非 TTY なら terminal setup 前に拒否する。
- Unicode 列幅は `unicode-width` の幅計算と terminal 幅に従って clip し、combining/emoji/East Asian ambiguous width の端末固有差を完全には保証しない。

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
- MUST: query domain は compiled query、prepared candidate、compiled ignore terms を所有し、search は traversal/ranking、UI は bounded highlight cache/rendering を所有する。
- MUST: ignore terms は CLI の filter operation ごと、GUI の terms/case scope または filter pass ごとに1回だけ compile し、候補 loop で公開 single-path adapter を繰り返し compile してはならない。

### Preconditions / Postconditions
- Preconditions: モジュール境界が定義済み。
- Postconditions: 主要仕様を unit/integration で検証可能。
