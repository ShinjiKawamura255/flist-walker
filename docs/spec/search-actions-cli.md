# Search, Actions, CLI, Errors, and Testability Specification

## SP-003 ファジー検索
### Requirements
- MUST: 通常語はファジー検索（AND 条件）で評価する。
- SHOULD: 通常語の各トークンは、subsequence 一致だけの候補よりもリテラル一致する候補を高く順位付けする。
- MUST: `'` は完全一致条件として評価する。
- MUST: `!` は除外条件として評価する。
- MUST: `^` / `$` は非 regex モードでは「先頭/末尾の隣接文字制約 + ファジー評価」として評価する。
- MUST: regex モードでも include token が regex 構文（例: `[](){}.*+?\\`）を含まない plain token の場合は、非 regex モードと同じファジー条件として評価する。
- MUST: regex モードで include token が regex 構文を含む場合のみ、その token を regex として評価する。
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

### Preconditions / Postconditions
- Preconditions: モジュール境界が定義済み。
- Postconditions: 主要仕様を unit/integration で検証可能。
