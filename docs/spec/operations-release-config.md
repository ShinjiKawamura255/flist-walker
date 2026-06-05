# Operations, Release, and Runtime Configuration Specification

## SP-015 Ignore List フィルタ
### Requirements
- MUST: 実行中 binary と同じフォルダにある `flistwalker.ignore.txt` を ignore list ファイルとして読み取れる。
- MUST: ignore list ファイルは 1 行 1 ルールを基本とし、空行と `#` コメント行を無視しなければならない。
- MUST: 検索クエリの `!` 除外は fuzzy fallback を使わず、literal substring / `^` 先頭 / `$` 末尾の一致で候補を除外しなければならない。
- MUST: ignore list の各ルールは、検索クエリの `!` 除外と同じ非 fuzzy の比較ルールで候補を除外しなければならない。
- MUST: GUI は `Use Ignore List` チェックボックスを提供し、既定で有効にしなければならない。
- MUST: チェックボックス有効時は、ignore list に一致する候補を検索結果と空クエリ表示から除外しなければならない。
- MUST: チェックボックス無効時は、ignore list の除外を適用してはならない。
- SHOULD: CLI モードでも同じ ignore list ファイルを適用できる。

### Preconditions / Postconditions
- Preconditions: 実行中 binary のフォルダに ignore list ファイルが存在する、または空/未存在である。
- Postconditions: ignore list に一致する候補は、既定有効時に一覧から除外される。

### Edge / Error
- ignore list ファイルが存在しない、読み取りできない、または空でも正常終了する。
- 1 つのルールが他のルールにマッチしなくても、残りのルールは継続して評価する。

### Regression Guard
- 発生条件: `Use Ignore List` が有効で、`Files` / `Folders` が両方有効な既定状態のまま `all_entries` の高速経路を通ると、ignore 判定が省略されて `old` や `~` を含む候補が結果へ戻る。
- 期待動作: ignore list は空クエリ表示と検索結果の両方で維持され、`Files` / `Folders` 両有効でも literal に一致する除外候補は表示されない。fuzzy でだけ一致する候補は除外しない。
- 非対象範囲: `Use Ignore List` を無効化した場合の候補除外。
- 関連テストID: TC-110, TC-112, TC-117.

## SP-016 Runtime Config Bootstrap
### Requirements
- MUST: ツールは runtime config file と関連する永続化ファイルを、Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` へ保存しなければならない。
- MUST: runtime config file は Windows では `%LocalAppData%\flistwalker\.flistwalker_config.json`、Linux/macOS では `~/.flistwalker/.flistwalker_config.json` を使わなければならない。
- MUST: Windows の旧バージョンで実行ファイル横または home directory に残っている同名ファイル、Linux/macOS の旧バージョンで home directory 直下に残っている同名ファイルは、新しい保存先に同名ファイルが存在しない場合に限り、新しい保存先へ移行しなければならない。
- MUST: runtime config file が存在しない場合、ツールは起動時に現在の `FLISTWALKER_*` 環境変数を seed にした runtime config file を自動生成しなければならない。
- MUST: 自動生成される runtime config file には、一般利用者が調整してよい `walker_max_entries`、`history_persist_disabled`、`restore_tabs_enabled`、`emacs_keybindings_enabled` を既定値で含めなければならない。
- SHOULD: 既存 runtime config file に上記 4 項目が欠けている場合、読み込み時に現在の実効値で項目を補完して書き戻す。
- MUST: runtime config file が存在する場合、ツールはその内容を runtime settings の source of truth として適用し、同名環境変数は seed としてのみ扱わなければならない。
- MUST: runtime config file には search parallelism、walker limits、window trace settings、query history persistence、tab restore、Emacs 風 keybindings、update policy を含めなければならない。
- MUST: GUI は runtime config file を開く設定ボタンを提供し、押下時に config file が存在しない場合は生成してから OS 既定アプリケーションで開かなければならない。既定アプリケーションで開けない場合は、標準的なテキストエディタ相当のフォールバックを試行しなければならない。
- SHOULD: runtime config file は手動追記された `developer` セクションを読み取れる。ただし `developer` セクションは自動生成 config seed に含めてはならず、公開 README や通常ヘルプで案内してはならない。
- MUST: runtime config file の読み込みや自動生成に失敗しても、ツールは通常起動を継続しなければならない。
- SHOULD: runtime config file の読み込み失敗や自動生成失敗は、利用者または診断ログへ警告として出力する。

### Preconditions / Postconditions
- Preconditions: current settings base directory が解決できる、または解決できない場合は config file を生成しない。
- Postconditions: runtime config file が存在する場合、その設定は起動時に process env へ反映されたうえで既存の env 読み取り経路へ伝播する。

### Edge / Error
- runtime config file が破損していても、ツールは安全に default / current env へフォールバックできる。
- seed-only 挙動のため、runtime config file が作成済みの場合は後から環境変数を変えても runtime settings は変化しない。
- Windows の `%LocalAppData%\flistwalker\`、Linux/macOS の `~/.flistwalker/` にある UI state / saved roots / window trace の各ファイルは、同じ保存先ルールで扱う。

## SP-017 Release Sample Ignore List
### Requirements
- MUST: ツールは ignore list sample を埋め込み、起動時に `flistwalker.ignore.txt.example` が実行中 binary と同じフォルダに存在しない場合は sample を自動生成しなければならない。
- MUST: sample は `flistwalker.ignore.txt` にリネームして live ignore list として使えることを利用者へ明示しなければならない。
- MUST: 既存の `flistwalker.ignore.txt` が存在する場合、sample 配置は既存 ignore list を上書きしてはならない。
- SHOULD: sample の生成に失敗しても、本体起動や自己更新は継続できなければならない。

### Preconditions / Postconditions
- Preconditions: 実行中 binary の所在が判定できる。
- Postconditions: sample は利用者が見つけやすい場所に配置され、既存 ignore list は保持される。

### Edge / Error
- sample が既に存在する場合は上書きしない。
- 実行中 binary の隣に ignore list が既にある場合は sample の生成だけを行い、live ignore list を作成しない。

## SP-014 起動時自己更新
### Requirements
- MUST: GUI 起動時に GitHub Releases の最新 version 確認を非同期 worker で実行し、UI スレッドをブロックしてはならない。
- MUST: 現在 version より新しい release が存在する場合、利用者へ更新承認ダイアログを表示する。
- MUST: Windows/Linux の自動更新対象は、現在実行中バイナリに対応する standalone asset と `SHA256SUMS` / `SHA256SUMS.sig` に限定する。
- MUST: Windows/Linux の自動更新では、standalone asset に対応する sidecar `*.LICENSE.txt` と `*.THIRD_PARTY_NOTICES.txt` も取得し、更新後の実行バイナリと同一ディレクトリへ `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` として配置しなければならない。
- MUST: Windows/Linux の自動更新では、standalone asset に対応する sidecar `*.README.txt` も取得し、更新後の実行バイナリと同一ディレクトリへ `README.txt` として配置しなければならない。
- MUST: ダウンロード後は埋め込み公開鍵で `SHA256SUMS.sig` が `SHA256SUMS` を正しく署名していることを確認し、失敗時は更新を中止する。
- MUST: 署名検証通過後に、`SHA256SUMS` に記載された対象 asset の SHA-256 と一致することを確認し、一致しない場合は更新を中止する。
- MUST: Windows では実行中 EXE を直接上書きせず、一時ディレクトリへ生成した補助 updater を別プロセスとして起動し、旧 EXE 終了後に置換と再起動を行う。
- MUST: Linux では staged binary を一時ディレクトリへ配置し、別プロセスの更新スクリプト経由で置換と再起動を行う。
- MUST: 更新失敗時は既存バイナリを維持し、利用者へ原因を通知する。
- SHOULD: 署名公開鍵が埋め込まれていない開発用ビルドでは、自動更新を manual-only として扱える。
- MUST: macOS では新しい release を検知しても自動置換を試みず、手動更新が必要であることを通知する。
- MUST: 更新ダイアログは、現在提示中の target version を「次のバージョンが出るまで表示しない」として抑止できなければならず、この抑止状態は起動間で保持されなければならない。
- MUST: 抑止済み target version 以下の更新候補は次回起動以降も再表示してはならず、より新しい version を検知した場合のみ再び更新ダイアログを表示しなければならない。
- MUST: 起動時の更新確認が失敗した場合、失敗理由を利用者へ確認できる軽量ダイアログを表示しなければならない。ただし通常の検索/操作は継続可能でなければならない。
- MUST: update worker 応答は request_id で相関し、stale 応答が新しい prompt / failure / install_started 状態を上書きしてはならない。
- MUST: update check / install が失敗、抑止、または supersede された場合、pending / in_progress 状態は解放され、通常操作を継続できなければならない。
- SHOULD: 上記の起動時更新確認失敗ダイアログは、「今後この種の起動時エラーを表示しない」として抑止でき、この設定は起動間で保持される。
- MUST: `FLISTWALKER_DISABLE_SELF_UPDATE` が truthy な場合、または実行中バイナリと同一ディレクトリに `FLISTWALKER_DISABLE_SELF_UPDATE` というファイルが存在する場合、起動時の更新確認、更新ダイアログ表示、更新適用開始を行ってはならない。
- MUST: 手動試験用 override 環境変数（更新 feed URL 差し替え、同一 version 許可、downgrade 許可）は内部検証専用とし、README、release note、配布物、ユーザ向けヘルプへ露出してはならない。
- SHOULD: 内部検証用に `FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE` を受け付け、起動時更新確認を意図的に失敗させて失敗ダイアログを強制表示できる。
- SHOULD: 更新チェック失敗やダウンロード失敗は通常の検索/操作を妨げない。
- SHOULD: 手動試験のために、更新 feed URL 差し替え、同一 version 許可、downgrade 許可を環境変数で上書きできる。

### Preconditions / Postconditions
- Preconditions: GUI モードで起動し、ネットワーク経由で GitHub Releases へ到達可能。
- Postconditions: 新版が無ければ何も変更せず、新版があれば承認後に検証済みバイナリだけが置換・再起動される。

### Edge / Error
- GitHub API 失敗、タイムアウト、asset 欠落、checksum 不一致は更新失敗として通知し、現行バイナリで継続する。
- 対応外 OS/arch は新版検知のみ行い、自動更新非対応の案内だけを返す。

## SP-012 CI / Release Security Hygiene
### Requirements
- MUST: 通常 CI は Windows/macOS/Linux の release 対象 OS を継続検証する。
- MUST: 通常 CI で `cargo audit` による依存脆弱性検査を実行する。
- MUST: `x86_64-pc-windows-gnu` 向け release build は最終 `flistwalker.exe` に Windows icon resource を含み、Explorer 上で埋め込みアイコンを表示できなければならない。
- MUST: draft release 作成後、macOS notarization は別工程で確認できる状態を維持する。
- MUST: notarization 環境が未整備な当面の間は、macOS 配布物の notarization 確認を publish 前提条件にしてはならない。その場合 publish 時は GitHub Release 本文の `Security` または `Known issues` に未 notarized である旨を明記しなければならない。
- SHOULD: release note / release template / release docs に checksum 検証手順と notarization の扱いを明記する。

### Preconditions / Postconditions
- Preconditions: CI または release workflow を更新する。
- Postconditions: 依存脆弱性検知と release 対象 OS の継続検証が行える。
