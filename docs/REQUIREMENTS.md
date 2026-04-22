# REQUIREMENTS

## Background / KPI
- CLI/GUI で `fzf --walker` 相当の高速検索体験を提供する。
- `FileList.txt`（`filelist.txt` 含む）を既存資産として利用し、対象集合を明示的に管理できるようにする。
- KPI-001: 10万件規模の候補に対して、インデックス構築後の検索応答を 100ms 未満（目標）にする。
- KPI-002: Windows/macOS/Linux で同等の検索・オープン体験を提供する。

## Scope
### In
- Rust での CLI/GUI 実装。
- ファイル/フォルダの収集（walker / FileList 両対応）。
- fzf 互換クエリでの検索。
- 検索結果からの実行/オープン。
- GitHub Releases を用いた起動時の更新確認と、Windows/Linux 向け自己更新。
- SDD + TDD ドキュメントとテスト整備。

### Out
- 旧プロトタイプの機能追加。
- ネットワーク共有ドライブ最適化。
- 配布インストーラ作成。
- macOS `.app` bundle の自動更新。

## Use cases
- UC-001: 開発者がプロジェクト配下を横断検索し、目的ファイルを即時オープンする。
- UC-002: 利用者が `FileList.txt` に列挙した対象のみ検索し、実行可能ファイルを起動する。
- UC-003: 利用者が検索結果のフォルダを選択し、OS のファイルマネージャで開く。
- UC-004: 利用者が GUI で候補を確認しながら、複数選択して一括アクションを実行する。
- UC-005: 利用者が起動時に新しい release を検知し、承認後にダウンロードと再起動で更新を適用する。

## Requirements
### Functional (FR)
- FR-001: ツールは FileList 優先モード有効時、検索ルート直下の `FileList.txt` / `filelist.txt`（および `filelist.txt` の大文字小文字違い）を優先して候補集合を構築しなければならない。
- FR-002: ツールはリスト未存在時、walker 方式で再帰走査して候補を構築しなければならない。
- FR-003: ツールは fzf 互換クエリ（`'`、`!`、`^`、`$`）に基づく検索を提供し、関連度順で結果を返さなければならない。
- FR-004: ツールは選択対象がファイルの場合、実行可能なら実行し、非実行ファイルは既定アプリでオープンしなければならない。
- FR-005: ツールは選択対象がフォルダの場合、OS 既定の方法でオープンしなければならない。
- FR-006: ツールは CLI モード（`--cli`）で検索結果を表示し、GUI なしで利用可能でなければならない。
- FR-007: ツールは GUI モードで検索入力、結果リスト、プレビュー、複数選択、一括実行/オープンを提供しなければならない。
- FR-008: ツールは FileList 由来の候補を表示する際に追加の root 配下判定を要求してはならず、インデクシング速度を維持しなければならない。
- FR-009: ツールは実行/オープン直前に選択パスが現在 root 配下であることを検証し、root 外パスのアクションを拒否しなければならない。UNC root を使用する場合も、同一 root 配下であれば許可しなければならない。
- FR-010: ツールは Create File List 実行時、祖先ディレクトリ直下の既存 FileList への追記が発生しうる場合、その前に利用者確認を要求しなければならない。
- FR-011: ツールは query history 永続化を既定で有効にしつつ、明示設定で無効化できなければならない。
- FR-012: ツールは検索結果に対して `Score` / `Name` / `Modified` / `Created` のソートを提供しなければならない。
- FR-013: ツールはソート対象を現在の検索結果スナップショットのみに限定し、インデクシング経路へ追加の属性取得を導入してはならない。
- FR-014: ツールは query が 1 文字でも変化した場合、適用済みの結果ソートを破棄して `Score` 順へ戻さなければならない。
- FR-015: ツールは日付ソートで利用する属性キャッシュを上限付きで保持し、長時間セッションでもメモリを無制限に増やしてはならない。
- FR-016: ツールは GUI タブをドラッグアンドドロップで並び替えでき、移動後も同じタブ内容を active のまま維持しなければならない。
- FR-017: ツールは GUI の検索オプションに `Ignore Case` チェックボックスを提供し、既定で有効にしなければならない。
- FR-018: ツールは GUI 起動直後および検索キャンセル後に、候補が存在する場合は 1 行目を既定選択として表示しなければならない。
- FR-019: ツールは GUI 起動時に GitHub Releases 上の最新 version を非同期確認し、現在 version より新しい配布版がある場合は利用者へ通知しなければならない。
- FR-020: ツールは利用者承認後、Windows/Linux の現在実行中バイナリに対応する release asset と `SHA256SUMS` およびその detached signature をダウンロードし、署名検証と整合性確認後に自己更新を適用しなければならない。
- FR-021: ツールは Windows では実行中 EXE の直接上書きを避け、一時配置した補助 updater で旧バイナリ置換と再起動を行わなければならない。
- FR-022: ツールは検索結果で選択した一般 `.ps1` を既定で直接実行してはならず、少なくとも実行ポリシー迂回なしの経路へ分離しなければならない。
- FR-023: ツールは macOS では新しい version を検知しても自動置換を試みず、手動更新が必要であることを示さなければならない。
- FR-024: ツールは更新ダイアログに、現在提示中の target version を次の version が出るまで再表示しない選択肢を提供し、その抑止状態を起動間で保持しなければならない。
- FR-025: ツールは GUI/CLI で、実行中 binary と同じフォルダにある ignore list ファイルを候補除外ルールとして適用でき、GUI では有効/無効を切り替えるチェックボックスを提供しなければならない。既定では有効でなければならない。
- FR-026: ツールは起動時に runtime config file を読み込み、Windows では実行ファイルと同じフォルダ、Linux/macOS では home ディレクトリを保存先として使わなければならない。これは UI state、saved roots、window trace などの永続化ファイルにも適用しなければならない。Windows の旧バージョンで home ディレクトリに保存されていた同名ファイルが存在する場合、新しい保存先に同名ファイルが存在しなければ自動移行しなければならない。runtime config file が存在しない場合は現在の `FLISTWALKER_*` 環境変数を seed にして自動生成しなければならない。runtime config file が存在する場合は、その内容を runtime settings の source of truth として適用し、同名環境変数は seed としてのみ扱わなければならない。
- FR-027: ツールは release asset に ignore list サンプルを同梱し、自己更新後に実行中 binary と同じフォルダへ ignore list ファイルが存在しない場合は sample を自動配置しなければならない。

### Non-functional (NFR)
- NFR-001: 10万件候補での検索処理は 100ms 未満を目標（SHOULD）とする。
- NFR-002: 例外時はユーザ向けに原因を表示し、非ゼロ終了コードを返すこと。
- NFR-003: モジュールはテスト可能に分離され、主要機能に unit test を持つこと。
- NFR-004: GUI の主要フロー（検索、選択、アクション、再読込）は回帰手順を定義すること。
- NFR-005: セキュリティ衛生として、CI は release 対象 OS を継続検証し、依存関係の脆弱性検査を自動実行しなければならない。
- NFR-006: ソート追加後もインデクシング速度は既存実装より劣化させないこと。
- NFR-007: 更新確認・ダウンロードは UI スレッドをブロックせず、失敗時も通常検索操作を継続可能でなければならない。

### Constraints (CON)
- CON-001: 本実装は Rust で行う。
- CON-002: 外部コマンド実行は配列引数で呼び出し、シェル展開依存を避ける。
- CON-003: root 配下判定はインデクシング経路へ導入せず、実行/オープン直前に行う。
- CON-004: 自己更新は既存 release asset 命名規則（`FlistWalker-<version>-<platform>-<arch>` と `SHA256SUMS`）を前提とし、追加の常駐 updater 事前配置を要求してはならない。

## Acceptance Criteria
- AC-001: FileList 優先モード有効時、`FileList.txt` 系ファイルが存在すれば walker を使わず、列挙対象のみが候補になる。
- AC-002: `FileList.txt`、`filelist.txt`、および `filelist.txt` の大文字小文字違いで同等に動作する。
- AC-003: クエリ入力で関連度順の上位 N 件を返せる。
- AC-004: ファイル選択時に実行/オープン、フォルダ選択時にオープンが行われる。
- AC-005: Rust テスト（unit/integration）が成功する。
- AC-006: GUI で検索・選択・実行/オープン・再読込が確認できる。
- AC-007: root 外パスは結果一覧に表示されても実行/オープンされず、利用者へブロック理由が通知される。
- AC-008: UNC root 配下の候補は root 外判定に誤って拒否されない。
- AC-009: Create File List 実行時、祖先 FileList 更新がありうる場合は確認ダイアログが表示され、拒否時は root 直下の FileList だけが更新される。
- AC-010: `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` 時は query history を読み書きしない。
- AC-011: CI は Linux/macOS/Windows でテストを実行し、依存脆弱性検査を通過する。
- AC-012: GUI の結果ペインから `Score` / `Name` / `Modified` / `Created` を選択できる。
- AC-013: `Name` ソートは即時に並び替わり、`Modified` / `Created` ソートは UI を固めずに完了する。
- AC-014: query 変更後はソート結果が維持されず、検索スコア順へ戻る。
- AC-015: インデクシング時の path 種別/日付属性取得は増えず、ソート属性は検索結果に対してのみ遅延取得される。
- AC-016: GUI でタブをドラッグすると並び順だけが更新され、active tab の root/query/filter は他タブへ入れ替わらない。
- AC-017: GUI の検索オプションに `Ignore Case` が表示され、起動直後から有効で、切り替えると検索結果とハイライトが追従する。
- AC-018: GUI 起動直後と `Esc` / `Ctrl+G` の検索キャンセル後に、候補がある場合は結果リストの 1 行目が選択状態になる。
- AC-019: GUI 起動後に新しい GitHub Release が見つかった場合、更新承認ダイアログが表示される。
- AC-020: Windows/Linux で更新を承認すると、対象 asset と `SHA256SUMS` / detached signature を検証後に置換し、新版本体が再起動する。
- AC-021: Windows では実行中 EXE の自己上書きではなく、一時 updater 経由で更新が適用される。
- AC-022: macOS では更新検知時に自動更新非対応が案内され、誤って自己置換しない。
- AC-023: 利用者が更新ダイアログで「次のバージョンが出るまで表示しない」を選ぶと、その target version は次回起動以降も再表示されず、より新しい version が見つかった場合のみ再びダイアログが表示される。
- AC-024: 実行中 binary と同じフォルダの ignore list ファイルに列挙した項目は、`!old !~` 相当の除外として検索候補から外れ、GUI の Use Ignore List チェックボックスで有効/無効を切り替えられる。
- AC-025: runtime config file が存在しない初回起動では、Windows では実行ファイルと同じフォルダ、Linux/macOS では home directory に、現在の `FLISTWALKER_*` 環境変数を反映した config file が自動生成される。runtime config file が既に存在する場合は、その内容が runtime settings として反映され、環境変数の変更だけでは runtime settings が変化しない。
- AC-027: UI state、saved roots、window trace などの永続化ファイルは、Windows では実行ファイルと同じフォルダに、Linux/macOS では home directory に保存される。
- AC-028: Windows の旧バージョンで home directory にあった runtime config / UI state / saved roots / window trace は、新しい保存先に同名ファイルが無い場合に自動移行される。
- AC-026: release asset には `*.ignore.txt.example` が同梱され、自己更新後に `flistwalker.ignore.txt` が存在しない場合は sample が `flistwalker.ignore.txt.example` として実行バイナリの隣へ配置される。

## Risks
- R-001: OS ごとのオープン/実行差異により挙動不一致が発生する。軽減策: 実行/オープン分岐を抽象化しテストで検証する。
- R-002: 大規模ディレクトリで走査コストが増大する。軽減策: FileList 優先、非同期処理、バッチ更新を維持する。
- R-003: FileList に root 外パスが含まれると、意図しない実行対象が一覧へ混入する。軽減策: 表示は許容しつつ、実行/オープン直前に root 配下判定で拒否する。
- R-004: query history の平文永続化は、運用によっては機微な検索語を残す。軽減策: 永続化無効化設定と注意書きを提供する。
- R-005: 依存脆弱性や release 対象 OS の退行が CI で検知されない。軽減策: Linux 追加と `cargo audit` を必須化する。
- R-006: 日付ソートのために全候補へ `metadata()` を導入すると index/search の体感が悪化する。軽減策: 結果スナップショット限定の遅延解決と上限付きキャッシュを採用する。
- R-007: GitHub API 一時障害やネットワーク不通で起動時更新確認が失敗する。軽減策: 非同期確認として失敗を notice に閉じ込め、検索機能は継続する。
- R-008: 実行中バイナリの置換に失敗すると更新後再起動できない。軽減策: Windows は別 updater、Linux は一時スクリプト経由で置換し、署名済み checksum manifest と整合する staged binary のみ使用する。
- R-009: ignore list の解釈が query とずれると、検索結果と UI 表示が不一致になる。軽減策: 除外判定は query の `!` と同じ比較ルールに寄せ、既定有効/切替状態を session に保持する。
- R-010: runtime config file の自動生成や seed-only 挙動が不明瞭だと、環境変数での一時的な変更が効かず、起動時設定の期待が外れる。軽減策: 初回生成と既存ファイル優先、Windows での exe 隣保存を README / release README / SPEC に明記し、起動時に file が source of truth であることを固定する。UI state、saved roots、window trace も同じ保存先ルールに揃える。
- R-011: release bundle に sample ignore list を含めないと、初回利用者が ignore list の配置例を見つけにくくなる。軽減策: release asset と self-update の両方に sample を含め、既存 ignore list を上書きしない。

## Traceability (excerpt)
- FR-001 -> SP-001 -> DES-001 -> TC-001
- FR-002 -> SP-002 -> DES-002 -> TC-002
- FR-003 -> SP-003 -> DES-003 -> TC-003
- FR-004 -> SP-004 -> DES-004 -> TC-004
- FR-005 -> SP-005 -> DES-004 -> TC-005
- FR-006 -> SP-006 -> DES-005 -> TC-006
- FR-007 -> SP-010 -> DES-009 -> TC-010
- FR-008 -> SP-007 -> DES-006 -> TC-049
- FR-009 -> SP-004 -> DES-004 -> TC-050
- FR-010 -> SP-001, SP-010 -> DES-007, DES-009 -> TC-052
- FR-011 -> SP-010 -> DES-007, DES-009 -> TC-054
- FR-012 -> SP-013 -> DES-013 -> TC-057
- FR-013 -> SP-013 -> DES-006, DES-013 -> TC-058
- FR-014 -> SP-013 -> DES-013 -> TC-059
- FR-015 -> SP-013 -> DES-013 -> TC-060
- FR-016 -> SP-010 -> DES-009 -> TC-064
- FR-017 -> SP-010 -> DES-009 -> TC-069
- FR-018 -> SP-010 -> DES-009 -> TC-070
- FR-019 -> SP-014 -> DES-014, DES-009 -> TC-074
- FR-020 -> SP-014 -> DES-014, DES-007 -> TC-075
- FR-021 -> SP-014 -> DES-014 -> TC-076
- FR-022 -> SP-004 -> DES-004 -> TC-094
- FR-023 -> SP-014 -> DES-014, DES-007 -> TC-077
- FR-024 -> SP-014 -> DES-014 -> TC-081
- FR-025 -> SP-003, SP-010 -> DES-016 -> TC-110
- FR-026 -> SP-016 -> DES-017 -> TC-111, TC-115
- FR-027 -> SP-017 -> DES-018 -> TC-113, TC-114
- NFR-001 -> SP-007 -> DES-006 -> TC-007
- NFR-002 -> SP-008 -> DES-007 -> TC-008
- NFR-003 -> SP-009 -> DES-008 -> TC-009
- NFR-004 -> SP-011 -> DES-010 -> TC-011
- NFR-005 -> SP-012 -> DES-012 -> TC-056
- NFR-006 -> SP-013 -> DES-006, DES-013 -> TC-058
- NFR-007 -> SP-014 -> DES-006, DES-014 -> TC-078
