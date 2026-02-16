# SPEC

## SP-001 FileList 優先読み込み
### Overview
- `FileList.txt` または `filelist.txt` がある場合の候補集合構築を定義する。

### Requirements
- MUST: プロジェクトルート直下で `FileList.txt` と `filelist.txt` を検出する。
- MUST: 両方ある場合は `FileList.txt` を優先する。
- MUST: 空行と `#` コメント行を無視する。
- SHOULD: 相対パスはルート起点で絶対化する。

### Preconditions
- 実行ディレクトリが検索ルートとして決定していること。

### Postconditions
- 有効な既存パスのみで候補集合が生成される。

### Normal cases
- `FileList.txt` を読み、候補として登録する。

### Error cases
- 読み込み失敗時はエラーメッセージを表示し、非ゼロ終了。

### Boundary cases
- 空ファイル時は候補ゼロ件。

### Inputs/Outputs
- Input: `FileList.txt` の行群
- Output: `Path[]`

### Compatibility
- Windows/macOS/Linux のパス区切りを許容。

### Invariants
- 候補は重複除去された正規化済みパスである。

## SP-002 Walker 走査
### Overview
- リスト未提供時のファイル/フォルダ再帰走査を定義する。

### Requirements
- MUST: ルート以下を再帰走査し、ファイル/フォルダを候補化する。
- SHOULD: シンボリックリンク循環を回避する。

### Preconditions
- `FileList.txt` と `filelist.txt` が存在しない。

### Postconditions
- 既存パスの候補集合を返す。

### Normal cases
- `os.scandir` ベースでスタック走査。

### Error cases
- 権限不足ディレクトリはスキップし警告をログする。

### Boundary cases
- 候補0件でも正常終了する。

### Inputs/Outputs
- Input: ルートディレクトリ
- Output: `Path[]`

### Compatibility
- Python 3.11+。

### Invariants
- 各候補はルート配下または絶対パス。

## SP-003 ファジー検索
### Overview
- 候補集合に対する高速検索を定義する。

### Requirements
- MUST: クエリ文字列に対して関連度順で結果を返す。
- MUST: 上位 `limit` 件を返す。
- SHOULD: 大文字小文字差を緩和する。

### Preconditions
- 候補集合が構築済み。

### Postconditions
- スコア降順の検索結果を返す。

### Normal cases
- `rapidfuzz` を使って抽出。

### Error cases
- クエリ未指定時は空配列。

### Boundary cases
- 候補が `limit` 未満なら全件返す。

### Inputs/Outputs
- Input: query, candidates, limit
- Output: `(Path, score)[]`

### Compatibility
- Rust 実装では同等アルゴリズムへ置換可能。

### Invariants
- 結果は候補集合の部分集合。

## SP-004 ファイル実行/オープン
### Overview
- ファイル選択時の動作を定義する。

### Requirements
- MUST: 実行可能ファイルはプロセス起動する。
- MUST: 非実行ファイルは既定アプリでオープンする。

### Preconditions
- 選択パスがファイルである。

### Postconditions
- 起動コマンドが実行される。

### Normal cases
- 実行権限あり: `subprocess.Popen([path])`。

### Error cases
- 起動失敗時は例外をユーザ向けに整形して返す。

### Boundary cases
- 拡張子関連付け未定義の場合は失敗を通知。

### Inputs/Outputs
- Input: file path
- Output: action result

### Compatibility
- OS 別の open コマンド差分を吸収。

### Invariants
- 実行時に候補集合は変更しない。

## SP-005 フォルダオープン
### Overview
- フォルダ選択時の動作を定義する。

### Requirements
- MUST: フォルダを OS 既定のファイルマネージャで開く。

### Preconditions
- 選択パスがディレクトリ。

### Postconditions
- ファイルマネージャ起動。

### Normal cases
- Windows: `explorer` / macOS: `open` / Linux: `xdg-open`。

### Error cases
- コマンド不在時は明示的なエラー。

### Boundary cases
- 既に開いている場合でも追加起動を許容。

### Inputs/Outputs
- Input: dir path
- Output: action result

### Compatibility
- 主要 OS で同一 UX を提供。

### Invariants
- 不正パスには操作しない。

## SP-006 Python→Rust 仕様固定
### Overview
- 試作で定義した I/O 契約を Rust 実装へ移行する。

### Requirements
- MUST: CLI 引数と挙動を文書化する。
- SHOULD: アクション分岐を同じ責務境界で設計する。

### Preconditions
- Python 試作が動作する。

### Postconditions
- Rust 実装時に参照する契約が存在する。

### Normal cases
- docs の DESIGN/API 節で契約を固定。

### Error cases
- 契約未確定項目は TBD として記録。

### Boundary cases
- 将来の GUI 追加でも CLI 契約は後方互換。

### Inputs/Outputs
- Input: Python implementation behavior
- Output: documented contract

### Compatibility
- Rust CLI への 1:1 移植を前提とする。

### Invariants
- AC を満たす操作は言語差に依存しない。

## SP-007 性能
### Overview
- 目標応答時間の基準。

### Requirements
- SHOULD: 10万件規模で検索 100ms 未満。

### Preconditions
- インデックス済み。

### Postconditions
- ベンチ結果を記録可能。

### Normal cases
- 文字列前処理と検索計算を分離。

### Error cases
- 閾値超過時は最適化タスク化。

### Boundary cases
- 低速環境では参考値として扱う。

### Inputs/Outputs
- Input: query + candidate set
- Output: latency metrics

### Compatibility
- Rust で改善可能な設計を維持。

### Invariants
- 正確性を犠牲にしない。

## SP-008 エラー処理
### Overview
- 失敗時の通知契約。

### Requirements
- MUST: ユーザが原因を理解できるエラーメッセージを返す。
- MUST: 非ゼロ終了コードを返す。

### Preconditions
- 例外発生。

### Postconditions
- 終了コードとメッセージが一貫する。

### Normal cases
- 既知例外を分類して表示。

### Error cases
- 未知例外は汎用メッセージ + 詳細ログ。

### Boundary cases
- UI 未初期化でも stderr 出力する。

### Inputs/Outputs
- Input: exception
- Output: message + exit code

### Compatibility
- Rust 実装で同等の終了コード体系を採用。

### Invariants
- 失敗時もプロセスはハングしない。

## SP-009 テスト容易性
### Overview
- TDD を成立させる分割。

### Requirements
- MUST: インデックス、検索、アクションを関数分離する。
- MUST: FileList 判定をユニットテスト可能にする。

### Preconditions
- モジュール境界が定義済み。

### Postconditions
- unit test で主要仕様を検証できる。

### Normal cases
- 副作用を関数境界外へ隔離。

### Error cases
- 依存注入不足時はリファクタ対象。

### Boundary cases
- OS 依存処理はモックで検証。

### Inputs/Outputs
- Input: module boundaries
- Output: testable interfaces

### Compatibility
- Rust 移植時も同じ責務で crate 分割可能。

### Invariants
- 仕様 ID と TC の対応が維持される。

## Traceability (excerpt)
- SP-001 -> DES-001 -> TC-001 (FR-001)
- SP-002 -> DES-002 -> TC-002 (FR-002)
- SP-003 -> DES-003 -> TC-003 (FR-003)
- SP-004 -> DES-004 -> TC-004 (FR-004)
- SP-005 -> DES-004 -> TC-005 (FR-005)
- SP-006 -> DES-005 -> TC-006 (FR-006)
- SP-007 -> DES-006 -> TC-007 (NFR-001)
- SP-008 -> DES-007 -> TC-008 (NFR-002)
- SP-009 -> DES-008 -> TC-009 (NFR-003)
