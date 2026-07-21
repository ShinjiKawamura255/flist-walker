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
- MUST: `--cli` 指定時は GUI を起動せず標準出力に結果を表示する。
- MUST: `--root` と `--limit` を受理する。
- MUST: クエリ未指定時は候補一覧を `limit` 件以内で表示する。
- MUST: CLI の `--limit` は実効値を追加で 1000 件へ丸めてはならない。
- SHOULD: 出力形式は機械処理しやすい行単位とする。

### Preconditions / Postconditions
- Preconditions: CLI モードで起動される。
- Postconditions: 結果またはエラーが標準出力/標準エラーへ出力される。

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
