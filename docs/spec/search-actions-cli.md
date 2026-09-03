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
- MUST: UI は filesystem I/O を行わない字句的 precheck だけを実施する。raw path が captured root から始まり、`.` / `..` の component 正規化によって root 外へ出ることを証明できる場合だけ `Reject` とする。別の absolute prefix に見える path は、link root の解決済み表現である可能性があるため `Defer` として worker へ渡す。相対パス、大小文字、prefix、非 UTF-8、drive-relative、rooted-without-drive、UNC、verbatim prefix など、字句情報だけでは安全性を確定できない入力も `Defer` とする。
- MUST: action request は、選択内容と同時に取得した trusted root を worker へ渡す。インデクシング経路には root 配下判定を追加しない。
- MUST: worker は OS への要求を発行する前に、trusted root とすべての effective target を解決し、raw effective target が OS-aware な component 比較で字句的 trusted root 配下にあるか、解決済み effective target が解決済み root 配下にある場合だけ権威的に許可する。単純な文字列 prefix や lossy な表示文字列を認可判断に使用してはならない。
- MUST: direct action の effective target は選択対象、open-containing-folder の effective target は通常ファイルまたは file link の字句的な親、ディレクトリまたは directory link/junction 自身とする。字句上 trusted root 配下に置かれた file/directory link と junction は、解決先が物理的な root 外でも許可する。trusted root 自体が link/junction の場合は、その字句的配下と解決済み配下のどちらの path 表現も許可する。
- MUST: 複数選択では、すべての effective target の解決と認可が成功するまで OS backend を一度も呼び出してはならない。1 件でも解決不能、または字句的 root と解決済み root のどちらにも属さない場合は要求全体を拒否する。
- MUST: worker は各 backend 呼び出しの直前にも対応する raw effective target を再解決し、root 配下判定を繰り返す。リンク先を含む execution path が事前認可時から変化した場合、または再検証が途中で失敗した場合は残りを実行せず、すでに開始した件数を含む partial completion として通知し、完了済み外部アクションを rollback したと主張してはならない。
- MUST: shared action request は trusted root、current-row selection snapshot、request identity、cancellation token を保持する。whole-request の事前認可成功後、単一 backend 呼び出しの直前に freshness/cancel 確認と再認可を行う。root switch または exit cancellation の観測後、新しい backend 呼び出しを開始してはならない。開始済み OS action は不可逆として扱う。
- MUST: OS backend へ渡す path は最後に認可した解決済み execution path とする。成功/失敗通知は利用者が選択した display path または effective display path を使い、拒否した root 外の解決先を表示してはならない。
- MUST: 字句的 root と解決済み root のどちらにも属さないパスは一覧表示されていても実行/オープンを拒否し、利用者へ通知する。
- MUST: UNC root を検索 root とする場合も、字句的または解決済みの同一 root 配下は許可し、どちらにも属さない別 share または root 外は拒否する。

### Preconditions / Postconditions
- Preconditions: 1 件以上の対象と、その選択時点の trusted root が action request に含まれる。
- Postconditions: 全対象について字句的または解決済み root 配下として事前認可が成功した場合だけ、各対象の直前再検証後に解決済み execution path が OS へ渡される。事前認可失敗では backend 呼び出しは 0 件となる。

### Edge / Error
- 起動失敗時はユーザ向けメッセージを返す。
- 拡張子関連付け未定義は失敗として通知する。
- trusted root または effective target を解決できない場合は fail closed とする。
- open-containing-folder の対象種別を metadata で確定できない場合、壊れた link、未対応の特殊種別は親フォルダへ推測変換せず fail closed とする。worker は事前認可時に使った各 source path の種別と effective target を backend 呼び出し直前に再導出し、変更されていれば残件を停止する。
- 別ドライブ、別 UNC share、または root 外の path でも、trusted root 自体の link 解決済み表現、字句的 root 配下の link/junction 経由、または解決済み root 配下のいずれかに該当すれば許可する。いずれにも該当しない path と、raw path が trusted root から始まりながら `..` の component 正規化で root 外へ出る path は拒否する。
- open-containing-folder で root 内の file link を選んだ場合は link の字句的な親を開き、directory link/junction を選んだ場合は解決済み directory を開く。direct action では file/directory link の解決済み target を開く。
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
- MUST: `--check-update` と `--update` は `--cli` を必要としない独立した CLI 操作とし、query、検索、FileList、action、GUI/TUI 起動と組み合わせてはならない。ただし `flistwalker --cli` を含む shell alias との互換性のため、この2操作と同時指定された `--cli` だけは意味を持たない互換フラグとして受理する。
- MUST: `--check-update` は更新を適用せず、最新版、更新候補、更新確認無効、または失敗を英語で報告する。更新候補がある場合は `flistwalker --update` を手動実行する案内を表示する。
- MUST: `--update` は利用者による明示的な更新承認として扱う。Windows/Linux の自動更新対応 bundle だけを既存の検証・transaction 経路へ渡し、manual-only platform では release URL を英語で表示して非ゼロ終了する。CLI から開始した更新の適用確認は X11 / Wayland を要求しない内部ヘッドレス再起動で完了し、GUI を起動してはならない。
- MUST: `--cli` 指定時は GUI を起動せず標準出力に結果を表示する。
- MUST: `--root` と `--limit` を受理し、既存の `--cli [QUERY] --root ... --limit ...` invocation を維持する。本仕様では subcommand を追加しない。
- MUST: クエリ未指定時は候補一覧を `limit` 件以内で表示する。
- MUST: CLI の `--limit` は実効値を追加で 1000 件へ丸めてはならない。
- MUST: batch CLI は `--sort score|name-asc|name-desc|modified-desc|modified-asc|created-desc|created-asc|size-desc|size-asc` を受理し、既定を `score` とする。全 match set を sort してから `limit` を適用し、`limit=0` は target 0 件とする。
- MUST: batch CLI は `--action print|open|reveal` を受理し、既定を `print` とする。`--action-all` は `open` / `reveal` のみで有効とし、non-print action の post-sort/post-limit target が複数で `--action-all` が無い場合は backend 呼び出し前に拒否する。既定 target は 1 件、`--action-all` は全 target である。
- MUST: batch CLI と interactive CLI は `-x` / `--exec COMMAND... {} ...` を受理し、固定 command より後の独立した `{}` 引数を正確に1個要求する。埋め込み `{}`、placeholder 不在、複数 placeholder は runtime config bootstrap と index/action dispatch より前に usage error とする。`-x` は残りの command template を受理するため、FlistWalker option はその前に置く。
- MUST: exec mode は post-sort/post-limit の全結果、または terminal 復旧後に確定した全選択を、各 path が独立した正規化済み絶対 argv となるよう `{}` の位置へ展開する。shell を暗黙起動せず、結果0件では command を1回も起動しない。
- MUST: Windows では標準 process API が暗黙に `cmd.exe` を介する `.bat` / `.cmd` program の直接指定を拒否し、`cmd.exe /C script.cmd ... {}` のような明示指定だけを shell semantics への opt-in として許可する。
- MUST: exec mode は inherited environment と固定 argv の使用量を差し引いた実行環境の command-line 上限まで path を順序どおり貪欲にまとめ、batch を直列実行する。起動時に OS が argument-list-too-long を返した batch は順序を保って細分化し、単一 path でも起動できない場合だけ失敗とする。`--exec-max-args N` は1 batch の path 数へ追加上限を設定し、`--dry-run` は認可と batch 計画だけを行って command を起動せず、対象数と batch 数を stderr へ出力する。
- MUST: exec mode は全 target を root 配下として事前認可し、各 batch 起動直前に再認可する。spawn failure、child 非ゼロ、認可変更、cancel を観測した場合は後続 batch を起動せず、完了 path 数と全 path 数を stderr へ報告する。success/dry-run は exit 0、失敗は exit 1、cancel は exit 130 とする。
- MUST: exec mode は `--absolute`、`--print0`、`--action open|reveal`、`--action-all` と同時指定できない。`--fail-no-match` は exec の結果0件にも既存どおり適用する。
- MUST: `print` は `--color` 未指定、非 TTY の `--color`/`--color auto`、および `--color never` で既存 path-only stdout framing を維持する。色が有効な場合は GUI/TUI と同じクエリ解釈による一致文字列だけを ANSI でハイライトし、非一致部分を装飾してはならない。`--color always` は TTY 判定を上書きする。non-print action は stdout に result path を書かず、progress、diagnostic、partial summary を stderr に書く。non-print action と `--absolute` または `--print0` の組合せは argument error とする。
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
- MUST: interactive CLI が root path を options summary、切替・refresh status、root picker、error に表示するときは共有 display normalization を通し、Windows の `\\?\` / `\\?\UNC\` extended prefix を利用者向け文字列へ露出してはならない。
- MUST: Windows release は単一の console-subsystem EXE とし、PowerShell / cmd から起動した batch CLI と interactive CLI が呼出元 console、同期完了、終了 code、標準 handle を維持しなければならない。GUI mode だけは native window 起動前に console から切り離し、CLI/TUI と GUI のために別 EXE を要求してはならない。
- MUST: CLI は `--color[=auto|always|never]` を受理する。未指定は `never` として ANSI 色エスケープを出力せず、値を省略した `--color` は `auto` とする。`auto` は batch CLI の stdout が TTY かつ空でない `NO_COLOR` 環境変数がない場合だけ色を有効化し、pipe/redirect 時は path-only stdout framing を維持する。`always` はこの自動判定を上書き、`never` は ANSI 色エスケープを出力しない。interactive CLI では画面描画に同じ色モードを適用する。
- MUST: interactive CLI の alternate screen、cursor、status/help、検索結果描画は標準エラー出力だけを使用し、terminal 復旧後に選択結果を標準出力へ出力するか、exec mode の外部 command へ渡す。外部 command は terminal guard 解放前に起動してはならない。
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
- MUST: TUI history overlay は Enter で highlighted history query を適用し、Esc で draft を復元して閉じ、Ctrl-C で全体を exit 130 とする。help overlay は Enter/Esc で閉じ、side-effect key を dispatch してはならない。options/sort/root overlay は Enter だけで highlighted choice を適用し、Esc は旧 state を保存して閉じ、Ctrl-C は exit 130 とする。`emacs_keybindings_enabled=true` のとき、全 overlay は共有 input command mapping を継承し、`Ctrl+N` / `Ctrl+P` を `Down` / `Up`、`Ctrl+J` / `Ctrl+M` を `Enter`、`Ctrl+G` を `Esc` と同義にする。対応する通常 command がない overlay では Emacs chord だけの新規 command を追加せず、help と各 overlay の操作案内へ有効な Emacs 風 shortcut だけを掲載する。
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
## SP-019 Named roots and search presets

### Requirements
- MUST: catalog 管理は positional subcommand を予約せず、`--list-named-roots`、`--add-named-root NAME=PATH`、`--remove-named-root NAME`、`--list-presets`、`--save-preset NAME`、`--remove-preset NAME`、`--preset NAME` の明示 long option で提供する。
- MUST: `--preset` は batch/TUI で利用でき、preset が所有する root/query/type/source/regex/case/ignore/sort の明示指定とは競合する。limit、color、output framing、明示 action は invocation が所有する。
- MUST: preset は pure search state だけを保持し、exec/open/reveal/update/FileList mutation を保存しない。
- MUST: presetの作成・削除はCLI/TUIとGUIで提供する。GUIはメイン画面の`Presets...` buttonとOS primary modifierの`Primary+Shift+P`のどちらからも同じpickerを開き、button tooltipにはplatformに対応するshortcutを表示する。preset名のfuzzy filter、`Up` / `Down`選択、`Enter`適用、`Add`追加、`F2` / `Edit`編集、`Delete` buttonからの確認付き削除、`Esc` cancelを提供する。preset一覧・選択・編集UIはmodal内に保ち、メイン画面へ埋め込まない。
- MUST: GUI picker は catalog を worker で読み込み、最新 request の response だけを採用する。loading/error/empty/no-match を modal 内で区別し、表示中の入力を背後の検索・選択・copy・実行へ漏らしてはならない。
- MUST: GUI editor は新規追加時に現在tabのroot/query/type/source/regex/case/ignore/sortを初期値とするdraftを作り、編集時は選択中presetの同fieldをdraftとする。root は absolute path のテキスト入力と folder picker の両方で指定できるようにする。folder picker の選択は draft の root だけを更新し、picker の cancel は手入力値を維持し、picker failure は draft と editor 内の error を残す。`Primary+Enter` / `Save` で専用 worker による lock付き read-modify-write を行う。追加とrenameは他presetとのcase-insensitive name collisionを拒否し、renameは元の位置と保存時点のunknown fieldsを保持する。保存失敗時は draft と error を残し、`Esc` / `Cancel` は未保存 draft を破棄して picker へ戻る。
- MUST: GUI picker と preset editor は同じ Named Root 管理画面への導線を提供する。管理画面は一覧選択、追加、名称・absolute path編集、削除確認をmodal内で行い、path はテキスト入力、folder picker、現在 root の採用を選べるようにする。folder picker の選択は draft の path だけを更新し、picker の cancel は手入力値を維持し、picker failure は draft と editor 内の error を残す。mutationを専用workerのlock付きread-modify-writeへ渡す。renameは元の位置とunknown fieldsを保持して参照presetの`root_name`を新名称へ更新し、case-insensitive collisionを拒否する。削除は参照presetの`root_name`を解除し、保存済みabsolute snapshotを残す。保存失敗時は入力とerrorを保持する。
- MUST: GUI の preset 適用は root/query/type/source/regex/case/ignore/sort を現在 tab へ反映し、root/source/type が変わる場合だけ既存 index refresh 経路を使う。適用自体は結果の open/execute/reveal や FileList mutation を開始してはならない。preset追加・編集・削除はcatalogだけを更新し、現在tabの検索状態または副作用actionを変更してはならない。削除失敗時は確認状態とerrorを保持する。
- MUST: GUI の preset 適用は preset 所有stateを検索要求より先に一度だけ確定し、index refreshが不要な場合もentry filterを再構築して最新requestを一度だけ発行する。適用前requestのresponseはrequest identityで破棄し、presetが所有しないtabのsort scopeは維持する。
- MUST: catalog management success は exit 0、lookup/storage failure は exit 1、argument contract failure は exit 2 とする。

### Regression Guard: atomic same-root preset application

- Scenario: root/source/typeが同じpresetを非空queryへ適用し、Ignore Listとsortだけを切り替えると、旧entry snapshotで検索されるか、旧requestのresponseでpreset sortが巻き戻る。
- Expected Behavior: preset所有stateを先に確定し、filter済みentriesとpreset sortを持つ最新requestを一度だけ発行する。旧requestをcancelしてresponseを破棄し、最新response後もpreset sortとtab所有sort scopeを維持する。
- Non-goals: presetへsort scopeを追加すること、storage schema変更、root/source/type変更時のindex refresh省略。
- Related Tests: TC-174; `regression_same_root_preset_applies_filters_and_sort_before_fresh_search`, `regression_same_root_preset_disabling_ignore_restores_all_search_entries`.
- Notes for Future Changes: preset適用経路でfilter、sort、search dispatchを個別に並べ直さず、atomic runtime transitionとpaired regression testsを同時に更新する。Ignore Listのtest fixtureはplatform所有のtemp ancestor（macOSの`/var/folders`など）に偶然一致しないfilename固有sentinelを使い、候補path全体に対するfilter契約を検証する。

### Preconditions / Postconditions
- Preconditions: runtime settings directory が解決でき、名前は trim 後に非空で制御文字と `=` を含まず、GUIで入力するnamed-root pathはabsoluteかつworkerで既存directoryとして解決できる。
- Postconditions: 成功した mutation は atomic catalog へ反映され、旧 saved-root file と action state は変更されない。

### Edge / Error
- malformed/newer catalog、lock timeout、write failure は既存 catalog を上書きせず明示失敗する。
- named root 削除後も preset は保存済み absolute snapshot で解決可能とする。

## SP-020 Field-scoped query terms

### Requirements
- MUST: field指定のないtermは既存どおりfilename優先かつvisible path全体を対象にする。
- MUST: positive/exact/exclusion termは、先頭の任意の `!` の後に `name:`、`path:`、`dir:`、`ext:` を1つ指定できる。field markerの後へ既存の `'`、`^`、`$`、token内 `|` を適用し、OR alternativeは同じfieldを継承する。
- MUST: `name:` はbasename、`path:` はroot相対visible path、`dir:` はその親pathまたは空、`ext:` はdotなしの最終suffixを対象とする。既知directoryとdotfileはextensionなし、`foo.tar.gz` は `gz` とする。
- MUST: field pathのseparatorは照合時だけ `/` へ正規化し、表示pathと既存の非field query契約は変更しない。
- MUST: `name:` など既知fieldの空値はcompile errorとし、未知の `prefix:` はfield指定ではない通常termとして扱う。
- MUST: regex判定はfield markerを除いたvalueへ適用し、scoreにfield markerを含めない。highlightはfield内の一致位置をvisible path上の文字位置へ写像する。
- SHOULD: shellでoperatorや空白が解釈されることを避けるため、CLI利用例はQUERY全体を引用符で囲む。

### Preconditions / Postconditions
- Preconditions: queryはGUI、batch CLI、TUIの共有compiled queryへ渡される。
- Postconditions: field指定はterm単位で閉じ、fieldなしtermと併用できる。

### Edge / Error
- 空の既知fieldは検索結果0件へ黙って変換せず、利用者へcompile errorを返す。
- directory kindが既知の候補は名前にdotがあっても `ext:` に一致させない。

## SP-021 参照

候補収集の `--max-depth`、GUI tab、preset、FileList / Walker 間の共通契約は [SP-021 候補収集の最大深度](indexing-performance.md#sp-021-候補収集の最大深度) を正本とする。

## SP-023 CLI 専用 executable と高速一回検索

### Requirements
- MUST: Cargo package は universal executable `flistwalker` と CLI 専用 executable `fw` を生成する。`flistwalker` の GUI/CLI/TUI 契約は変更しない。
- MUST: `fw` は引数解析前に hidden updater restart を処理し、その後 `--cli` を内部注入して既存 CLI/TUI parser・validation・実行経路を共有する。help の program name は `fw` とする。
- MUST: batch CLI は metadata 付き index entry を path へ戻して再分類せず、一回限りの検索で prefix cache の lookup/store と cache 用 full-result clone/sort を行わない。検索結果、score、order、filter、limit は従来経路と一致させる。
- MUST: updater は process entrypoint で universal/CLI variant を明示し、同じ variant の standalone binary asset のみを選択する。variant 間の version skew は許容する。release 上の version 付き README/LICENSE/THIRD_PARTY_NOTICES asset は共有するが、自己更新後のローカル配置は universal の `README.txt` / `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` と CLI の `fw.README.txt` / `fw.LICENSE.txt` / `fw.THIRD_PARTY_NOTICES.txt` に分離する。
- MUST: release は既存 archive と macOS app asset を universal-only のまま維持し、Windows/Linux/macOS x86_64/macOS arm64 の `fw` standalone asset を追加する。統合集約後は厳密に 28 asset、`SHA256SUMS` は 26 distribution entry とする。
- MUST: NFR-014 の性能測定を満たす。2回目の全測定でも未達の場合は release を停止する。
- MUST: Windows `fw.exe` は CLI の open/reveal 契約に必要な Shell32/User32 を許容する一方、GUI framework/rendering/window 系 import（GDI32、OpenGL32、imm32、psapi、dwmapi、uxtheme）を持たない。

### Edge / Error
- 対象 release に `fw` asset がない場合、`fw` updater は universal binary へフォールバックせず fail closed とする。
- `fw` と `flistwalker --cli` の同じ invocation は stdout、stderr、exit code を一致させる。ただし help/program name と updater の手動実行案内は実行 variant の command 名を使う。
