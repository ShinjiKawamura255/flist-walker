# Operations, Release, and Runtime Configuration Specification

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
## SP-014 起動時自己更新
### Requirements
- MUST: GUI 起動時に GitHub Releases の最新 version 確認を非同期 worker で実行し、UI スレッドをブロックしてはならない。
- MUST: 現在 version より新しい release が存在する場合、利用者へ更新承認ダイアログを表示する。
- MUST: Windows/Linux の自動更新対象は、現在実行中バイナリに対応する standalone asset と `SHA256SUMS` / `SHA256SUMS.sig` に限定する。
- MUST: Windows/Linux の自動更新では、standalone asset に対応する sidecar `*.LICENSE.txt` と `*.THIRD_PARTY_NOTICES.txt` も取得し、更新後の実行バイナリと同一ディレクトリへ `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` として配置しなければならない。
- MUST: Windows/Linux の自動更新では、standalone asset に対応する sidecar `*.README.txt` も取得し、更新後の実行バイナリと同一ディレクトリへ `README.txt` として配置しなければならない。
- MUST: release metadata は 2 MiB、`SHA256SUMS` は 1 MiB、`SHA256SUMS.sig` は 64 KiB、standalone binary は 512 MiB、各 sidecar は 16 MiB の decoded byte 上限を持ち、`Content-Length` の有無や値にかかわらず streaming reader が実受信 byte 数を強制しなければならない。
- MUST: 接続 timeout は 10 秒、無通信 timeout は 30 秒、1 request の deadline は 5 分、update staging 全体の monotonic deadline は 10 分とし、timeout/deadline 到達時は更新を中止しなければならない。
- MUST: redirect は最大 3 hop を明示処理し、production は HTTPS かつ `api.github.com`、`github.com`、または `*.githubusercontent.com` のみに制限しなければならない。開発・自動試験だけは loopback HTTP を許可してよい。
- MUST: 先に `SHA256SUMS` と `SHA256SUMS.sig` だけを取得し、埋め込み公開鍵で署名を検証してから配布 asset を取得しなければならない。manifest は空白区切りの SHA-256 と単一 filename からなる厳密な行文法を使い、必須 asset の欠落、重複、未知 filename、無効 digest を拒否しなければならない。
- MUST: 署名検証通過後、対象 binary と全 sidecar を private create-new file へ streaming download しながら SHA-256 を計算し、manifest と一致した完全な bundle だけを `VerifiedUpdateBundle` として activation へ渡さなければならない。
- MUST: staging 失敗時は main process がこの要求で create-new した partial file と staging directory だけを helper 起動前に削除し、既存 path を cleanup 対象にしてはならない。
- MUST: activation 準備は現在 executable の canonical parent 内の固定派生名を使い、target、`.new`、backup、lock、marker が directory、symlink、Windows reparse point、または parent 外である場合は更新を開始してはならない。
- MUST: 1 個の create-new active lock と versioned durable marker で transaction を排他し、marker は transaction/parent/helper identity、global phase、各 target の存在・旧新 hash・`prepared|intent|applied|rolled_back` 状態を write-ahead で記録しなければならない。
- MUST: helper は parent が durable `helper_registered` phase と helper identity を記録したことを確認し、create-new acknowledgement を同期するまで filesystem mutation を行ってはならない。parent は acknowledgement を検証するまで適用開始を通知せず、本体終了を許可してはならない。
- MUST: helper は acknowledgement 後に旧 process の終了を最大 30 秒待ち、timeout を binary commit 前失敗として扱わなければならない。
- MUST: sidecar を先に適用し、binary 置換を唯一の commit point として最後に行わなければならない。Windows の既存 target は同一 volume の `[System.IO.File]::Replace(new, target, backup, false)`、Linux の既存 target は create-new backup の同期後に同一 directory rename を使い、不在 target は同一 directory の no-overwrite hard-link promotion と source unlink を使わなければならない。
- MUST: binary commit 前の失敗と新 process の生成失敗では、元から存在した target を検証済み backup から復元し、元から無かった target を削除して旧 bundle の hash を確認しなければならない。
- MUST: 起動時 recovery は marker phase と旧新 hash から precommit rollback、完全な committed bundle、rolled-back bundle のいずれかへ収束させなければならない。live 登録 helper が存在する transaction と同時に回復してはならず、欠落 backup、hash 不一致、不正 state 遷移、path/type 変化は ambiguous として証跡を保持し、新しい update を開始してはならない。
- MUST: 検証では Windows/Linux の同一 filesystem 上にある inert dummy file だけを使い、実行中 FlistWalker binary の置換または外部 application の起動を行ってはならない。
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
- Postconditions: 新版が無ければ何も変更せず、新版があれば承認後に検証済み bundle 全体が置換・再起動される。失敗または中断時は検証済み旧 bundle へ戻るか、完全な新 bundle を保持するか、曖昧状態を変更せず停止する。

### Edge / Error
- GitHub API 失敗、timeout/deadline、redirect/origin 違反、上限超過、manifest 不正、asset 欠落、checksum 不一致は更新失敗として通知し、現行バイナリで継続する。
- transaction lock/marker 衝突、helper acknowledgement 不成立、parent wait timeout、backup/atomic primitive 不成立、recovery ambiguity は fail closed とし、既存 installation と recovery 証跡を変更しない。
- 対応外 OS/arch は新版検知のみ行い、自動更新非対応の案内だけを返す。

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
- MUST: 自動生成される runtime config file には、一般利用者が調整してよい `walker_max_entries`、`history_persist_disabled`、`restore_tabs_enabled`、`emacs_keybindings_enabled`、`tab_pin_moves_to_next_row` を既定値で含めなければならない。
- SHOULD: 既存 runtime config file に上記 5 項目が欠けている場合、読み込み時に現在の実効値で項目を補完して書き戻す。
- MUST: runtime config file が存在する場合、ツールはその内容を runtime settings の source of truth として適用し、同名環境変数は seed としてのみ扱わなければならない。
- MUST: runtime config file には search parallelism、walker limits、window trace settings、query history persistence、tab restore、Emacs 風 keybindings、Tab pin movement、update policy を含めなければならない。
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

## SP-018 PowerShell Windows GNU Build
### Requirements
- MUST: `scripts/build-rust-win.ps1` は Windows PowerShell から `cargo build --release --locked --target x86_64-pc-windows-gnu` を実行し、`rust/target/x86_64-pc-windows-gnu/release/flistwalker.exe` と `FlistWalker.exe` を生成しなければならない。
- MUST: `scripts/build-rust-win-clean.ps1` は同じ依存解決契約を使い、対象 target の clean 後に release build を実行しなければならない。
- MUST: `-CheckOnly` は検出だけを行い、install、`rustup target add`、clean、build、copy、strip を実行してはならない。
- MUST: `-NoInstall` は prompt を表示せず、不足項目と手動導入コマンドを表示して非ゼロ終了しなければならない。
- MUST: `-InstallMissing` は Rustup、Rust GNU target、MSYS2、`mingw-w64-x86_64-gcc` の導入を明示承認済みとして扱う。通常モードは各導入単位を別々に確認し、非対話環境では `-NoInstall` 相当で動作しなければならない。
- MUST: Rustup と MSYS2 の bootstrap は `winget` の exact package ID と `winget` source を指定し、実行前に package ID、変更内容、管理者権限を要求する可能性を表示しなければならない。
- MUST: MSYS2 package 導入は `C:\msys64\usr\bin\pacman.exe` または検出した同等パスを直接実行し、`pacman -S --needed --noconfirm mingw-w64-x86_64-gcc` を使わなければならない。`pacman -Sy` 単独による partial upgrade を行ってはならない。
- MUST: install 後は process/User/Machine PATH、Cargo home、MSYS2 固定候補を再読込し、`cargo`、`rustup`、`gcc`、`g++`、`ar`、`ranlib`、`windres`、`strip` を再検出しなければならない。永続 PATH をスクリプト自身が直接変更してはならない。
- MUST: GNU tool は `FLISTWALKER_WINDOWS_*` override、MSYS2 mingw64 固定候補、PATH の順で解決し、解決結果を Cargo target と `build.rs` 用環境変数へ設定しなければならない。
- MUST: Windows host の GNU build でも `windres` と `ar` を使って Windows resource を生成し、`resource.o` を `flistwalker` GUI binary へ明示リンクしなければならない。
- MUST: strip は実体へ一度だけ適用し、大小文字を無視して同一パスとなる自己 copy を避けたうえで、最終的な 2 名の EXE を byte-identical にしなければならない。

### Preconditions / Postconditions
- Preconditions: Windows PowerShell 5.1 または PowerShell 7 で repository checkout を利用し、既存依存を使うか、利用者が不足依存の導入を承認する。
- Postconditions: build 成功時は Windows icon/resource、`asInvoker` manifest、GUI subsystem を持ち、意図しない MSYS2 runtime DLL に依存しない release EXE が 2 名で存在する。

### Edge / Error
- `winget` 不在、承認拒否、install 失敗、install 後の再検出失敗、build/strip 失敗では後続 build を実行せず、原因と再実行または手動導入コマンドを表示する。
- install 後に現在の process で再検出できない場合は、新しい PowerShell を開いて再実行する案内を表示する。
- partial install は自動 rollback せず、導入済み package ID/package 名を表示して再実行可能な状態を保つ。
