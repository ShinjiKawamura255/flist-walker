# Product Scope and Acceptance Criteria

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
- AC-010: runtime config の `history_persist_disabled` が有効なときは query history を読み書きしない。
- AC-010A: runtime config の `emacs_keybindings_enabled` が `false` のときは Emacs 風ショートカットをアプリ操作として消費せず、`true` または未指定時は既存どおり有効にする。
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
- AC-024: 実行中 binary と同じフォルダの ignore list ファイルに列挙した項目は、`!old !~` 相当の非 fuzzy 除外として検索候補から外れ、GUI の Use Ignore List チェックボックスで有効/無効を切り替えられる。
- AC-025: runtime config file が存在しない初回起動では、Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` に、現在の `FLISTWALKER_*` 環境変数を反映した config file が自動生成される。自動生成された config file は、一般利用者向けの `walker_max_entries` / `history_persist_disabled` / `restore_tabs_enabled` / `emacs_keybindings_enabled` を既定値で保持し、詳細項目は設定済み環境変数だけを保持する。runtime config file が既に存在する場合は、その内容が runtime settings として反映され、環境変数の変更だけでは runtime settings が変化しない。
- AC-027: UI state、saved roots、window trace などの永続化ファイルは、Windows では `%LocalAppData%\flistwalker\` に、Linux/macOS では `~/.flistwalker/` に保存される。
- AC-028: Windows の旧バージョンで実行ファイル横または home directory にあった runtime config / UI state / saved roots / window trace、Linux/macOS の旧バージョンで home directory 直下にあった同名ファイルは、新しい保存先に同名ファイルが無い場合だけ自動移行される。
- AC-026: `flistwalker.ignore.txt.example` が存在しない状態で起動しても、ツールは sample を実行バイナリの隣へ自動生成し、`flistwalker.ignore.txt` へのリネーム案内を提供する。
- AC-029: GUI の設定ボタンを押すと runtime config file が生成済みの状態で開かれ、既定アプリケーションが失敗した場合はテキストエディタ相当のフォールバックが試行される。
- AC-030: 保存済みウィンドウ位置が現在の仮想ディスプレイ矩形外にある状態で GUI を起動しても、初期ウィンドウは現在の表示範囲内に配置される。現在の仮想ディスプレイ内の負座標配置は維持される。
- AC-031: タブの close ボタン上へマウスを移動すると、close ボタンの押下領域が背景・枠・カーソルなどでタブ本体の押下領域と区別できる。
- AC-032: GUI で複数タブを閉じた後、`Ctrl+Shift+T` は最後に閉じたタブから順に active tab として復元し、可能な限り閉じる前の tab 位置へ戻す。復元対象がない場合はタブ数を変えずに通知する。
