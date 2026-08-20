# Operations, Release, and Runtime Configuration Specification

## SP-012 CI / Release Security Hygiene
### Requirements
- MUST: required CI は Windows/macOS/Linux の release 対象 OS を version-addressed runner 世代、固定 Rust、full SHA Action、固定 CI tool version で継続検証し、hosted image version を run evidence に残す。番号付き hosted runner image 内の package drift は残存リスクとして扱う。
- MUST: `master` 変更は PR と required `CI Gate` / `CI Policy Guardian` を経由し、required approving review は 0 件とする。PR ごとに rebase auto-merge と merge後branch削除を1回だけ登録し、merge commit と squash merge を禁止して linear history を要求する。ローカルcommitの境界・順序・message・authorは保持し、SHAとcommitter metadataの変更は許容する。各変更は、clean な local `master` で fetch、FF-only pull、current branch `master`、および `master == origin/master` を確認してから、最初のcommit前にfeature branchで開始しなければならない。auto-merge の実完了後は、active task が作業ツリーの clean 状態、記録済み PR 番号の `MERGED` 状態、`mergedAt`、base `master`、および削除候補と一致するhead branchを確認してから `origin/master` を fetch し、ローカル `master` を fast-forward 同期する。current branch `master` と `master == origin/master` を再確認した後、local feature branch は `master` に到達可能で他 worktree が使用していない場合だけ通常削除する。rebaseでSHAが書き換わり通常削除だけが拒否された場合に限り、対象が `master` 以外、同一PR identity、同期済みmaster、未使用worktree、`git rev-list --merges origin/master..refs/heads/<head-branch>` の空出力、および `git log --cherry-pick --right-only --no-merges origin/master...refs/heads/<head-branch>` の空出力を全て確認して、local feature branchだけを強制削除してよい。未マージ、dirty worktree、branch作成前または同期後の確認失敗、PR不一致、patch差分、feature branchのmerge commit、または branch 使用中では状態を変更せず停止する。feature branchは自由にpushでき、force-with-leaseは非保護feature branchに限る。`master` の直接push、force push、branch deletion、admin bypass、強制 reset、`master`へのrebase/merge、remote branch削除でgateまたは後処理を回避してはならない。
- MUST: `CI Gate` は CI policy、release 対象 OS test/build、clippy/coverage を集約する。任意階層の `Cargo.toml` / `Cargo.lock`、`rust/.cargo/audit.toml`、audit workflow、CI policy checker/test の変更では `cargo audit` も集約し、対象変更で audit が skipped の場合は失敗しなければならない。非対象変更の skipped は正常としてよい。
- MUST: accepted vulnerability advisory はcargo-auditがproject-local configとして自動読込する`rust/.cargo/audit.toml`に限定し、根拠・owner・review cadence・再評価 trigger を `docs/OSS_COMPLIANCE.md` に保持する。unmaintained warning は出力上で可視のままにする。
- MUST: workflow、Dependabot設定、toolchain、audit exception、CI policy checker/testはdefault branch版をimmutable trusted setとし、通常PRではrunner/action/tool version pinだけを変更可能にする。accepted advisoryを含む構造変更は設定snapshot、独立agent review、guardian requirementの一時解除と即時復元、protected-route再検証を伴うcontrolled rolloutでのみ行う。
- MUST: scheduled security audit は後日公開 advisory を検知し、失敗 issue を同じ run で作成または更新する。agent は 24 時間以内に分類する。
- MUST: latest runner/Rust canary は required gate と分離し、失敗 issue を同じ run で作成または更新する。agent は 7 日以内に分類する。
- MUST: canary failure/success、security notice、EOL/deprecation、dependency MSRV、hosted image drift を pin 更新検討 trigger とする。通常 promotion は scheduled canary 2 回連続成功と candidate `CI Gate` 成功を必要とし、security/EOL/deprecation 対応でも candidate gate を省略してはならない。
- MUST: workflow は least privilege permissions、job timeout、branch/PR concurrency を定義し、untrusted code を `pull_request_target` で実行してはならない。`CI Policy Guardian` に限り read-only `pull_request_target` を許可し、default branch の trusted policy だけを実行して PR policy blob を data として検査し、PR head checkout/実行、secret、cache、artifact を禁止する。Cargo cache は download data に限定し、tool binary と `rust/target` を共有してはならない。
- MUST: `x86_64-pc-windows-gnu` 向け release build は最終 `flistwalker.exe` に Windows icon resource を含み、Explorer 上で埋め込みアイコンを表示できなければならない。
- MUST: Windows release ZIP のarchive rootは`flistwalker.exe`、`README.txt`、`LICENSE.txt`、`THIRD_PARTY_NOTICES.txt`の4項目だけを含み、flat GitHub Release asset用のversion付きsidecar名をarchive内へ流用してはならない。
- MUST: draft release 作成後、macOS notarization は別工程で確認できる状態を維持する。
- MUST: notarization 環境が未整備な当面の間は、macOS 配布物の notarization 確認を publish 前提条件にしてはならない。その場合 publish 時は GitHub Release 本文の `Security` または `Known issues` に未 notarized である旨を明記しなければならない。
- SHOULD: release note / release template / release docs に checksum 検証手順と notarization の扱いを明記する。

### Preconditions / Postconditions
- Preconditions: CI、release workflow、runner/Rust/Action/tool pin、または repository merge policy を更新する。
- Postconditions: default branch の`scripts/check_ci_policy.py`を実行する`CI Policy Guardian`と`CI Gate`がrequired契約を検証し、security/latest driftは追跡issueへ接続される。
## SP-014 起動時自己更新
### Requirements
- MUST: GUI 起動時に GitHub Releases の最新 version 確認を非同期 worker で実行し、UI スレッドをブロックしてはならない。
- MUST: TUI 起動時にも最新 version 確認を非同期 worker で実行し、入力ループをブロックしてはならない。candidate の受信は英語の手動更新案内だけを表示し、download/apply を開始してはならない。
- MUST: CLI からの更新適用は利用者が `--update` を明示した場合だけ開始し、`--check-update` と TUI 通知は installation state を変更してはならない。alias 由来の `--cli` は更新承認を追加も取消もしない。
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
- MUST: helper 起動は installation directory を child current directory に固定してはならない。Windows で canonical helper path が `\\?\` / `\\?\UNC\` 形式の場合、最初の起動失敗時だけ同一 path の非 verbatim 表現でも再試行し、両方失敗した通知では OS error を保持しつつ利用者向け path から `\\?\` を除去しなければならない。
- MUST: helper は acknowledgement 後に旧 process の終了を最大 30 秒待ち、timeout を binary commit 前失敗として扱わなければならない。
- MUST: sidecar を先に適用し、binary 置換を唯一の commit point として最後に行わなければならない。Windows の既存 target は同一 volume の native `ReplaceFileW(target, new, backup, 0, null, null)` を updater process 内で使い、Linux の既存 target は create-new backup の同期後に同一 directory rename を使い、不在 target は同一 directory の no-overwrite hard-link promotion と source unlink を使わなければならない。
- MUST: binary commit 前の失敗と新 process の生成失敗では、元から存在した target を検証済み backup から復元し、元から無かった target を削除して旧 bundle の hash を確認しなければならない。
- MUST: Windows の更新後 process 起動は最大3ラウンド、ラウンド間100msの bounded retry とし、canonical target が verbatim drive/UNC形式なら各ラウンドで同一 path の非verbatim表現も試さなければならない。GUI restart は生成後500ms以内に終了した process を起動失敗として扱い、新版起動失敗時は旧 bundle へ rollback して同じ起動契約で旧GUIを再起動しなければならない。新版と旧版の起動が両方失敗した場合は、両方の診断を失わず helper failure として終了しなければならない。
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
- Preconditions: GUI/TUI の自動確認、または明示的な CLI 更新操作で起動し、ネットワーク経由で GitHub Releases へ到達可能。
- Postconditions: 新版が無ければ何も変更せず、新版があれば承認後に検証済み bundle 全体が置換・再起動される。失敗または中断時は検証済み旧 bundle へ戻るか、完全な新 bundle を保持するか、曖昧状態を変更せず停止する。

### Edge / Error
- GitHub API 失敗、timeout/deadline、redirect/origin 違反、上限超過、manifest 不正、asset 欠落、checksum 不一致は更新失敗として通知し、現行バイナリで継続する。
- transaction lock/marker 衝突、helper acknowledgement 不成立、parent wait timeout、backup/atomic primitive 不成立、recovery ambiguity は fail closed とし、既存 installation と recovery 証跡を変更しない。
- 対応外 OS/arch は新版検知のみ行い、自動更新非対応の案内だけを返す。

### Regression Guard: windows-updater-in-process-replace
- Scenario: Windows updater が marker と target の原子的置換ごとに `powershell.exe` を外部起動し、更新中にターミナルが何度も表示されるか、一時的な process 起動失敗で適用が失敗する。
- Expected Behavior: Windows の既存 target 置換は updater process 内の `ReplaceFileW` だけで行い、`powershell.exe` の存在や `PATH` に依存しない。backup あり/なし、失敗時の source 保持、verbatim/UNC を含む非 lossy UTF-16 path、interior NUL 拒否を維持する。
- Non-goals: antivirus、権限、別 process による file lock 自体の解消、production binary を使った live update の自動試験。
- Related Tests: TC-158, TC-159, TC-160, TC-171; `tc171_regression_windows_file_replace_*`.
- Notes for Future Changes: 外部 shell/process に戻す、Windows path を lossy 変換する、または `ReplaceFileW` の target/source/backup 順を変更する場合は paired regression tests と VM-005 を同一変更で更新する。

### Regression Guard: windows-updater-helper-spawn-path
- Scenario: Windows の長い installation directory で canonical path が `\\?\` 形式になり、helper 起動時に同じ長い directory を child current directory に指定すると、実行ファイル自体は有効でも `CreateProcessW` が失敗する。
- Expected Behavior: helper は parent の current directory を継承して起動し、verbatim path での起動失敗時は非 verbatim の同一 path を 1 回だけ試す。起動失敗で parent 所有 transaction を破棄して次回試行を妨げず、最終エラーには OS error を含めるが `\\?\` / `\\?\UNC\` を表示しない。
- Non-goals: directory の実行権限不足、antivirus/WDAC による executable block、別 process の file lock を迂回すること。
- Related Tests: TC-179; `tc179_regression_helper_launch_does_not_force_install_directory_as_current_dir`, `tc179_regression_windows_helper_launch_retries_without_verbatim_prefix`, `tc179_regression_windows_helper_spawn_error_hides_verbatim_prefix`, `tc179_regression_windows_normal_helper_path_is_not_retried`, `tc179_regression_failed_helper_launch_cleanup_allows_a_fresh_prepare`.
- Notes for Future Changes: helper の working directory、Windows path spelling、または spawn error の組み立てを変更するときは TC-179 と VM-005 の sandbox self-update を同一変更で確認する。

### Regression Guard: windows-updater-restart-handoff
- Scenario: Windows の更新で新版 process の生成が一過性に失敗し、旧 bundle への rollback 後に行う旧GUIの単発再起動も失敗すると、installation は安全に旧版へ戻っていても画面が再表示されず、利用者が手動起動するまで停止する。
- Expected Behavior: 新版とrollback後の旧版は、最大3ラウンド・100ms間隔、verbatim path時の非verbatim代替を含む同じbounded restart契約を使う。GUI childが500ms以内に終了した場合も再試行し、全試行失敗時は新版と旧版の両エラーを保持する。
- Non-goals: antivirus/WDACや権限設定を迂回すること、500ms経過後の任意時点のGUI crashを完全検出すること、production binaryを置換する自動試験。
- Related Tests: TC-159, TC-160, TC-186; `tc186_regression_windows_restart_retries_a_transient_spawn_failure`, `tc186_regression_windows_restart_retries_without_verbatim_prefix`, `tc186_regression_windows_restart_exhaustion_is_bounded_and_diagnostic`, `tc186_regression_new_and_old_restart_failures_are_both_reported`.
- Notes for Future Changes: restart attempt数、待機時間、path表現、GUI startup grace、rollback後の再起動error処理を変更するときはTC-186とVM-005を同一変更で確認し、旧版復旧を単発・silent failureへ戻さない。

## SP-015 Ignore List フィルタ
### Requirements
- MUST: 実行中 binary と同じフォルダにある `flistwalker.ignore.txt` を ignore list ファイルとして読み取れる。
- MUST: ignore list ファイルは 1 行 1 ルールを基本とし、空行と `#` コメント行を無視しなければならない。
- MUST: ignore list は UTF-8 と任意の先頭 UTF-8 BOM、LF/CRLFを同じterm列として解釈し、path separatorの `/` と `\\` をplatform間で同じliteral pathとして比較しなければならない。
- MUST: 検索クエリの `!` 除外は fuzzy fallback を使わず、literal substring / `^` 先頭 / `$` 末尾の一致で候補を除外しなければならない。
- MUST: ignore list の各ルールは、検索クエリの `!` 除外と同じ非 fuzzy の比較ルールで候補を除外しなければならない。
- MUST: GUI は `Use Ignore List` チェックボックスを提供し、既定で有効にしなければならない。
- MUST: チェックボックス有効時は、ignore list に一致する候補を検索結果と空クエリ表示から除外しなければならない。
- MUST: チェックボックス無効時は、ignore list の除外を適用してはならない。
- MUST: batch CLI とTUIは既定sidecarと `--ignore-file` の同じdecoder/matcherを使い、FileList/Walker、空query/非空queryのどの経路でも除外を適用しなければならない。

### Preconditions / Postconditions
- Preconditions: 実行中 binary のフォルダに ignore list ファイルが存在する、または空/未存在である。
- Postconditions: ignore list に一致する候補は、既定有効時に一覧から除外される。

### Edge / Error
- ignore list ファイルが存在しない、または空なら空termとして正常継続する。CLI/TUIでは、存在する既定sidecarまたは明示fileが読めない、もしくはUTF-8不正なら除外なしで続行せず明示errorにする。
- 1 つのルールが他のルールにマッチしなくても、残りのルールは継続して評価する。

### Regression Guard
- 発生条件: `Use Ignore List` が有効で、`Files` / `Folders` が両方有効な既定状態のまま `all_entries` の高速経路を通ると、ignore 判定が省略されて `old` や `~` を含む候補が結果へ戻る。
- 期待動作: ignore list は空クエリ表示と検索結果の両方で維持され、`Files` / `Folders` 両有効でも literal に一致する除外候補は表示されない。fuzzy でだけ一致する候補は除外しない。
- 非対象範囲: `Use Ignore List` を無効化した場合の候補除外。
- 関連テストID: TC-110, TC-112, TC-117, TC-176.

## SP-016 Runtime Config Bootstrap
### Requirements
- MUST: ツールは runtime config file と関連する永続化ファイルを、Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` へ保存しなければならない。
- MUST: runtime config file は Windows では `%LocalAppData%\flistwalker\.flistwalker_config.json`、Linux/macOS では `~/.flistwalker/.flistwalker_config.json` を使わなければならない。
- MUST: Windows の旧バージョンで実行ファイル横または home directory に残っている同名ファイル、Linux/macOS の旧バージョンで home directory 直下に残っている同名ファイルは、新しい保存先に同名ファイルが存在しない場合に限り、新しい保存先へ移行しなければならない。
- MUST: runtime config file が存在しない場合、ツールは有効な GUI、batch CLI、interactive CLI、`--list-saved-roots`、`--create-filelist` の dispatch 前に現在の `FLISTWALKER_*` 環境変数を seed にした runtime config file を自動生成しなければならない。内部 update helper と引数検証失敗は bootstrap 対象外とする。
- MUST: 自動生成される runtime config file には、一般利用者が調整してよい `walker_max_entries`、`history_persist_disabled`、`restore_tabs_enabled`、`emacs_keybindings_enabled`、`tab_pin_moves_to_next_row` を既定値で含めなければならない。
- SHOULD: 既存 runtime config file に上記 5 項目が欠けている場合、読み込み時に現在の実効値で項目を補完して書き戻す。
- MUST: runtime config file が存在する場合、ツールはその内容を runtime settings の source of truth として適用し、同名環境変数は seed としてのみ扱わなければならない。
- MUST: shared persistence は default/saved-root の read-only access と query-history mutation を分離する。`history_persist_disabled` が true のとき、history load/save と history diagnostic text は no-op とする。
- MUST: history writer は full snapshot ではなく ordered、trimmed、nonempty delta を submit する。cross-process sidecar lock の下で latest JSON を reread し、各 delta を exact duplicate removal、most-recent append、front trim 100 entries の順で適用する。serialized writers が別 query を追加した場合は commit order で両方を保持する。
- MUST: GUI/TUI frame code は `UiStatePatch + history_delta` を enqueue するだけとする。non-history leaf は last-write-wins、history delta は enqueue order で全て concat し、unknown top-level/nested JSON field は保持する。lock timeout/write failure は exact coalesced patch/delta を retry 用に保持し、successful commit はその generation だけ clear する。
- MUST: bounded lock wait、latest-read merge、atomic write は persistence worker が行い、frame dispatch を block してはならない。graceful shutdown は frame rendering 外で bounded flush を要求する。crash-before-flush history loss は residual risk とする。
- MUST: runtime config file には search parallelism、walker limits、window trace settings、query history persistence、tab restore、Emacs 風 keybindings、Tab pin movement、update policy を含めなければならない。GUI/TUI の通常 Walker index は同じ `walker_max_entries` と adaptive initial/max limit を参照しなければならない。
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
- Postconditions: build 成功時は Windows icon/resource、`asInvoker` manifest、console subsystem を持ち、意図しない MSYS2 runtime DLL に依存しない byte-identical release EXE が 2 名で存在する。単一 EXE の GUI mode は runtime に console から切り離される。

### Edge / Error
- `winget` 不在、承認拒否、install 失敗、install 後の再検出失敗、build/strip 失敗では後続 build を実行せず、原因と再実行または手動導入コマンドを表示する。
- install 後に現在の process で再検出できない場合は、新しい PowerShell を開いて再実行する案内を表示する。
- partial install は自動 rollback せず、導入済み package ID/package 名を表示して再実行可能な状態を保つ。
