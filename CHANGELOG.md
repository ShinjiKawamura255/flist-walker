# Changelog

このファイルは Keep a Changelog 形式と SemVer に沿って管理します。

## [Unreleased]
### Added
-

### Changed
-

### Fixed
-

### Breaking
-

### Deprecated
-

### Security
-

### Known issues
-

## [0.17.0] - 2026-04-22
### Added
- 実行ファイルと同じフォルダに置く `flistwalker.ignore.txt` による検索除外リストを追加し、既定で有効化した。
- 起動時に環境変数から runtime config を seed して `~/.flistwalker_config.json` を自動生成するようにした。

### Changed
- runtime config の公開文書を一般ユーザ向けに整理し、高度な設定項目は案内しないようにした。
- Windows GNU ビルドでの更新確認に `rustls` を使うよう戻し、TLS backend 未設定エラーを回避した。
- Linux の軽量 perf gate を hosted runner のばらつきに合わせて緩和した。
- GitHub Actions の Clippy and Coverage job で `actions/upload-artifact` を Node 24 対応版へ更新した。

### Fixed
- Release 用 README の生成と macOS release packaging の文面崩れを修正した。
- Clippy が嫌う runtime config の初期化パターンを整理した。

### Breaking

### Deprecated

### Security
- 更新アセットは署名付き `SHA256SUMS.sig` と checksum 照合で検証する。

### Known issues
- macOS 配布物は notarization 環境が整うまで未 notarized の場合がある。

## [0.16.1] - 2026-04-06
### Fixed
- オンデマンドファイルを含む巨大なエントリリスト（50万件超）でのIndexingフリーズを修正。`Arc<Vec<Entry>>` に対する O(N) クローンを排除し、メタデータ解決中のUIハングを解消。
- `all_entries` 走査時の不要なパス複製と中間ベクタ生成を削減し、インデックス完了後の後処理を高速化。

## [0.16.0] - 2026-04-02
### Added
- `Entry { path, kind }` domain model を導入し、index/search/app 間の候補表現を統一した。
- structural refactoring 後の GUI smoke test 手順を `docs/TESTPLAN.md` に追加し、別環境での確認結果を記録できるようにした。

### Changed
- `FlistWalkerApp` の責務を段階的に再編し、worker bus、runtime UI state、query/history state、search coordinator、cache/root browser/request-tab routing を専用 state holder へ分離した。
- `index_pipeline` test を機能別の複数 module へ分割し、`ARCHITECTURE.md` / `DESIGN.md` / `TESTPLAN.md` を現行構成へ追従更新した。
- structural refactoring の完了に伴い temporary change plan rule を `AGENTS.md` から削除した。

### Fixed
- incremental search/index path の不要 clone を削減し、`sync_entries_from_incremental` と `apply_entry_filters` の全量再確保を抑制した。
- `SearchCoordinator`、cache state、launch seed/bootstrap bag の field 直書きを減らし、request lifecycle と初期化経路の局所性を改善した。
- `cargo clippy --all-targets -- -D warnings` と `cargo doc --no-deps` が通るように state accessor と bootstrap 型を整理した。

### Breaking
- 

### Deprecated
- 

### Security
- structural refactoring 後も self-update の sidecar 文書、署名検証、checksum 検証まわりの手動確認経路を維持し、GUI smoke 手順へ記録した。
- macOS 配布物は当面未 notarized のまま publish する場合がある。

### Known issues
- macOS の自動更新は未対応で、GitHub Releases からの手動更新が必要。
- macOS 配布物は未 notarized の場合がある。

## [0.15.0] - 2026-04-01
### Added
- tagged release 前に Linux / macOS / Windows native で `cargo test --locked` と `cargo audit` を実行する preflight gate を追加した。
- 通常 CI に Windows native runner を追加し、Windows 固有分岐の回帰を早めに検出できるようにした。
- `main.rs` の root viewport 構築を helper 化し、起動 geometry / icon / app_id の回帰を test で固定した。

### Changed
- `app.rs` の責務を段階的に分離し、tab lifecycle、pipeline、cache、search/index coordinator、session/state helper を専用 module へ移した。
- `eframe` / `egui` を `0.29.1` 系へ更新し、`run_native`、text cursor、IME、widget API の追従を行った。
- `EFRAME-UPGRADE-NOTES`、architecture 関連 docs、OSS compliance 運用メモを現在の構成に合わせて更新した。

### Fixed
- CLI の `--limit` に暗黙で入っていた 1000 件上限を撤廃した。
- Results 行の描画と hitbox のずれで、表示がセンタリングされクリックが効きにくくなる回帰を修正した。
- 日本語 IME の確定文字が消える問題と、変換中 Space が半角スペースとして混入する問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- release 前 gate を workflow に追加し、tag push から draft release 作成までの導線で未検証 build が通らないようにした。
- 依存更新に合わせて `THIRD_PARTY_NOTICES.txt` を更新し、OSS compliance チェック運用を docs に明文化した。
- macOS 配布物は当面未 notarized のまま publish する場合がある。

### Known issues
- macOS の自動更新は未対応で、GitHub Releases からの手動更新が必要。
- macOS 配布物は未 notarized の場合がある。

## [0.14.0] - 2026-03-31
### Added
- タブごとに accent color を設定できるようにし、active / inactive で見分けやすい表示を追加した。
- regex モードでも plain token と plain OR token は引き続き fuzzy 条件で一致するようにした。
- 起動時更新確認失敗を、ネットワークや feed の問題を調べやすい専用ダイアログで確認できるようにした。
- 起動時更新確認失敗ダイアログを強制表示する内部検証用フラグ `FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE` を追加した。

### Changed
- 非空 query の結果一覧描画を可視行中心に絞り、検索入力中やカーソル移動時の UI 応答性を改善した。
- 起動時更新確認失敗ダイアログの文言を一般ユーザ向けに見直し、`Details` セクションで内部エラーを分けて表示するようにした。

### Fixed
- 検索窓で query が入っている状態でも、左右カーソル移動や Backspace が重くなりやすい問題を改善した。
- 起動時の自己更新チェックが失敗したとき、何も表示されず原因が分からないままになる問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 自己更新の起動時確認失敗も可視化し、ネットワーク・feed・asset 不整合の切り分けをしやすくした。
- macOS 配布物は当面未 notarized のまま publish する場合がある。

### Known issues
- macOS の自動更新は未対応で、GitHub Releases からの手動更新が必要。
- macOS 配布物は未 notarized の場合がある。

## [0.13.4] - 2026-03-30
### Added
- 実行バイナリと同じディレクトリに `FLISTWALKER_DISABLE_SELF_UPDATE` ファイルがある場合も、自己更新を無効化できるようにした。

### Changed
- `FileList` / Walker の初期インデクシング経路を見直し、Windows と WSL をまたぐ運用でも高速経路を優先するようにした。
- `Create File List` 完了後の再読み込みは、そのまま現在タブを使って進めるようにした。
- 自己更新は standalone バイナリだけでなく、対応する `LICENSE.txt` と `THIRD_PARTY_NOTICES.txt` も同じディレクトリへ配置するようにした。

### Fixed
- `Create File List` 完了後のバックグラウンド再インデックスで worker 送信に失敗した場合、タブが進行中表示のまま残る問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- standalone バイナリ配布に対応する `LICENSE` / `THIRD_PARTY_NOTICES` sidecar asset を自己更新でも追従配置するようにした。
- `THIRD_PARTY_NOTICES.txt` に自己更新関連依存の notice 反映漏れを補完した。

### Known issues
- macOS の自動更新は未対応で、GitHub Releases からの手動更新が必要。

## [0.13.3] - 2026-03-29
### Added
- 更新署名用の鍵ペアを生成する `generate_update_keypair` helper を追加した。

### Changed
- Windows の一般 `.ps1` は既定で直接実行せず、既定アプリでオープンするようにした。
- 自己更新の release packaging と GitHub Actions workflow で `SHA256SUMS.sig` を生成・添付するようにした。
- 自動更新対象ビルドは埋め込み公開鍵の有無で auto/manual-only を切り替えるようにした。

### Fixed
- `FileList` の Windows 形式相対パスを、実行環境に存在する候補を優先して解釈するようにし、Linux/macOS での誤解釈を防いだ。

### Breaking
- 

### Deprecated
- 

### Security
- 自己更新は `SHA256SUMS.sig` の署名検証と `SHA256SUMS` の checksum 検証を通過した asset のみ適用するようにした。
- 一般ファイル操作で選択した `.ps1` に `ExecutionPolicy Bypass` を使わないようにした。

### Known issues
- macOS の自動更新は未対応で、GitHub Releases からの手動更新が必要。

## [0.13.2] - 2026-03-29
### Added
- Create File List の保留中および実行中に、GUI から明示的にキャンセルできるようにした。
- フッター右端に現在の version を常時表示するようにした。

### Changed
- Create File List 実行中の status panel は、長い path や notice が表示されても右端の version 表示と干渉しないレイアウトにした。

### Fixed
- 自己更新ダイアログで `Download and Restart` を複数回押しても、重複した更新 request を送らないようにした。
- `FileList.txt` と UI state / saved roots の保存を atomic write にし、途中中断で既存ファイルを失いにくくした。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS の自動更新は未対応で、GitHub Releases からの手動更新が必要。

## [0.13.1] - 2026-03-29
### Added
- CLI で `--version` / `-V` により現在の version を表示できるようにした。
- `FLISTWALKER_DISABLE_SELF_UPDATE=1` を追加し、起動時の更新確認と更新適用を明示的に無効化できるようにした。

### Changed
- 自己更新の無効化フラグが有効な場合は、GUI 起動時の update request を送らず、updater 側でも更新処理を開始しないようにした。

### Fixed
- 

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS の自動更新は未対応で、GitHub Releases からの手動更新が必要。

## [0.13.0] - 2026-03-28
### Added
- 起動時に GitHub Releases を確認し、新しい version があれば自己更新ダイアログを表示するようにした。
- Windows 向けに、same-version / downgrade / custom feed を安全に試せる PowerShell の手動検証スクリプトを追加した。

### Changed
- 更新ダイアログに `Don't show again until the next version` を追加し、抑止した target version は次の version が出るまで再表示しないようにした。
- 起動時の更新確認は UI を block せず、Windows/Linux は staged update 後に再起動、macOS は手動更新案内のみとする運用にした。

### Fixed
- Shift+Double Click で Shift+Enter と同じく親フォルダを開けるようにした。
- 起動時にネットワークがオフラインでも、更新確認失敗をエラー表示せず通常どおり起動・操作できるようにした。

### Breaking
- 

### Deprecated
- 

### Security
- ダウンロードした更新 asset は `SHA256SUMS` と照合した場合のみ適用し、配布 notice へ追加依存のライセンス情報を反映した。

### Known issues
- macOS の自動更新は未対応で、GitHub Releases からの手動更新が必要。

## [0.12.3] - 2026-03-28
### Added
- 

### Changed
- Windows のファイル/フォルダ Action 通知と失敗通知で、表示上は `\\?\` / `\\?\UNC\` を除去し、ユーザ向けパス表示を読みやすくした。

### Fixed
- Windows で Action フッタや `Action failed:` 通知に `\\?\` が表示される問題を修正した。
- Windows で `&` などの特殊記号を含むパスを開く・親フォルダを開く際に、`cmd /C start` 経由のシェル解釈で失敗しうる問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.12.2] - 2026-03-27
### Added
- 

### Changed
- 空白区切りの通常検索 token は、subsequence だけで一致する候補よりもリテラル一致する候補を高く順位付けするようにした。

### Fixed
- 空白区切り query で、各 token のリテラル一致が十分に順位へ反映されず、意図した候補が上位に出にくい問題を改善した。
- Windows で `\\?\` / `\\?\UNC\` 付きパスを扱う場合、Result 表示の相対パスと検索対象の相対化規約がずれて、見えていない絶対パス側の文字でヒットしうる問題を改善した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.12.1] - 2026-03-26
### Added
- Results の属性表示に `LINK` を追加し、シンボリックリンクと Windows ショートカット（`.lnk`）を判別できるようにした。

### Changed
- CHANGELOG の欠落していた過去リリース記録を補完し、版履歴を参照しやすくした。

### Fixed
- 

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.12.0] - 2026-03-24
### Added
- 

### Changed
- 検索クエリを要求単位で前処理し、候補ごとの path 文字列化と正規化の重複を減らした。
- 大規模候補集合では検索 worker 内で並列評価を行い、表示用ランキングは partial top-N 抽出へ切り替えて、数十万件規模でも検索応答を改善した。
- 非アクティブタブは `results` と preview の表示キャッシュを compact し、再表示時は `base_results` から復元するようにした。

### Fixed
- 数十万件規模の結果集合で、全件ソートと重複正規化がボトルネックになって検索が遅くなる問題を改善した。
- 重いタブを複数開いたまま運用すると、非アクティブタブの結果表示キャッシュが残り続けてメモリを圧迫しやすい問題を改善した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.11.6] - 2026-03-21
### Added
- GUI の検索オプションに `Ignore Case` チェックボックスを追加し、既定で有効にした。

### Changed
- 検索結果とハイライトで共有クエリ解釈を使うようにし、`'` / `!` / `^` / `$` / `|` の挙動を一致させた。
- 検索結果更新時は current row を行番号ベースで維持し、結果数が減った場合のみ末尾へ丸めるようにした。
- GUI 起動直後および `Esc` / `Ctrl+G` の検索キャンセル後は、候補が存在する場合に 1 行目を既定選択にした。

### Fixed
- 検索窓がインアクティブなときに `Home` / `End` / `PageUp` / `PageDown` で結果一覧を操作できるようにした。
- 検索結果の再適用でカーソルが勝手に先頭へ飛ばず、保持できる場合は同じ行番号を維持するようにした。
- 検索欄フォーカス中でも `Esc` / `Ctrl+G` のキャンセル後に結果一覧へ戻り、カーソルが消えないようにした。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.11.5] - 2026-03-19
### Added
- `Ctrl+1` から `Ctrl+9` で左から対応するタブへ直接切り替えられるようにした。

### Changed
- `Ctrl+Tab` / `Ctrl+Shift+Tab` のタブ切り替えをブラウザ系アプリと同じ操作系にそろえ、macOS でも `Ctrl` 系ショートカットとして扱うようにした。
- README のショートカット説明を実装に合わせて更新した。

### Fixed
- 検索欄と履歴検索欄へ複数行テキストを貼り付けたときに、改行や不可視の書式文字がそのまま残って検索しづらくなる問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.11.4] - 2026-03-18
### Added
- 

### Changed
- 

### Fixed
- タブドラッグ移動の追加後に、通常のマウスクリックでタブ切り替えできなくなっていた問題を修正した。
- 結果ソートのプルダウンで `Modified` / `Created` 項目が折り返されて 2 行表示になる問題を修正した。
- worker shutdown 時に `sort-metadata` request channel の切り離しが漏れ、終了時に timeout しやすい問題を修正した。
- macOS で `Preview` 表示設定がアプリ再起動後に引き継がれない問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.11.3] - 2026-03-17
### Added
- 

### Changed
- Windows GNU release profile と post-build strip を追加し、配布 EXE サイズを縮小した。

### Fixed
- Windows GNU ビルドで生成した EXE に複数サイズのアイコンを埋め込み、Explorer / タスクバー / 通知領域で `flist-walker` のアイコンが汎用アイコンへ化ける問題を修正した。
- `Create File List` 系の確認ダイアログ表示中に、十字キーや `Enter` / `Space` がメインウィンドウへ漏れてしまう問題を修正した。
- GUI 終了時に worker shutdown timeout まで待たされ、close 操作が重くなる問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.11.2] - 2026-03-17
### Added
- 

### Changed
- 

### Fixed
- Windows GNU ビルドで生成した EXE に複数サイズのアイコンを埋め込み、Explorer / タスクバー / 通知領域で `flist-walker` のアイコンが汎用アイコンへ化ける問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.11.1] - 2026-03-16
### Added
- タブをドラッグアンドドロップで並べ替えられるようにした。
- release asset と standalone バイナリに `THIRD_PARTY_NOTICES` を同梱するようにした。

### Changed
- Windows 向けクロスビルドを `x86_64-pc-windows-gnu` + mingw-w64 ベースへ移行した。

### Fixed
- 結果ソートのラベル位置を調整し、selector と縦位置がずれる問題を修正した。
- Linux/WSL 上で Windows 向け EXE 名の大文字小文字差異によりビルド後処理が失敗する問題を修正した。
- CI matrix job で bash shell 解決が不安定になる問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- Linux など `created()` を取得できない環境では、`Created` ソート時に該当項目は末尾へ並ぶ。
- macOS 配布物は publish 前に notarization 確認が必要。

## [0.11.0] - 2026-03-14
### Added
- 結果ペインに `Score` / `Name` / `Modified` / `Created` のソートを追加した。
- `Modified` / `Created` ソート用に、検索結果だけを対象にした非同期属性取得と上限付きキャッシュを追加した。
- `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` により、query history 永続化を明示的に無効化できるようにした。

### Changed
- ソート適用後もカーソルは同じアイテムへ追従せず、同じ行番号に留まるようにした。
- `Create File List` で祖先 FileList への追記がありうる場合は、事前確認を必須にした。

### Fixed
- Windows/macOS CI で共通に落ちていたアクション要求キュー投入テストの不安定要因を修正した。
- `cargo audit` で検出された `idna 0.4.0` 依存を解消し、CI が通らない問題を修正した。
- GitHub Actions の release asset 集約が macOS `.app` bundle 内部の `FlistWalker` / `Info.plist` / `FlistWalker.icns` まで拾ってしまう問題を修正した。
- query history の履歴検索アクションと default root 起動順の不整合を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- `FileList.txt` に root 外パスが含まれていても、実行/オープン直前に現在 root 配下かを検証し、root 外アクションを拒否するようにした。
- 通常 CI に Linux と `cargo audit` を追加し、release 対象 OS と依存脆弱性を継続検証するようにした。
- query history は既定で平文永続化されるため、必要に応じて `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` を利用する運用を明示した。

### Known issues
- Linux など `created()` を取得できない環境では、`Created` ソート時に該当項目は末尾へ並ぶ。
- macOS 配布物は publish 前に notarization 確認が必要。

## [0.10.3] - 2026-03-09
### Added
- 

### Changed
- 

### Fixed
- Windows の 8.3 短縮パスと macOS の `/private` パス差分により cross-platform test が不安定になる問題を修正した。
- query history の UI state 永続化テストが共有保存先へ依存し、並列実行時に干渉して失敗する問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- 

## [0.10.2] - 2026-03-09
### Added
- 

### Changed
- 

### Fixed
- release 用 `README` 生成時に shell literal が展開され、PowerShell 環境変数例や `Set as default` 表記が崩れる問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。

## [0.10.1] - 2026-03-09
### Added
- `Create File List` 実行時に、子 `FileList` 参照を祖先 `FileList` へ伝搬して追記できるようにした。

### Changed
- draft release 公開前の preflight 手順と、タブ復元を含む release 運用ドキュメントを更新した。

### Fixed
- OneDrive など Cloud Files API の placeholder を provider 横断で検出し、プレビュー読み込み対象から除外するようにした。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。

## [0.10.0] - 2026-03-07
### Added
- 

### Changed
- `Esc` を `Ctrl+G` と同じ clear/cancel 操作へ統一し、`Ctrl+Shift+R` を廃止した。
- `Ctrl+R` を query history のファジー検索へ変更し、同じ検索欄で履歴検索中とわかる表示および `Enter` / `Ctrl+J` / `Ctrl+M` による展開を追加した。
- query history を全タブ共通で最大100件まで永続化するようにした。

### Fixed
- 

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。

## [0.9.2] - 2026-03-07
### Added
- 

### Changed
- 

### Fixed
- GitHub Actions の macOS release build に、サポート対象の Intel runner を使うように修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。

## [0.9.1] - 2026-03-07
### Added
- tag push を契機に Linux / Windows / macOS 向け asset をビルドし、draft release まで自動作成する workflow を追加した。
- open/execute 実行中は、フッターに spinner と進捗ラベルを表示するようにした。

### Changed
- リリース asset 方針と draft release 運用を、GitHub Actions ベースのフローへ更新した。

### Fixed
- 前回起動時に使っていた root を UI state へ保持し、更新後の再起動でも startup root が意図せず初期化されないようにした。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。

## [0.9.0] - 2026-03-07
### Added
- macOS 向けショートカット差分を整理し、`Cmd+T` / `Cmd+W` / `Cmd+Tab` 系と `Cmd+Shift+C` を主要操作として揃えた。
- タブ単位の検索履歴を追加し、`Ctrl+R` / `Ctrl+Shift+R` で前後移動できるようにした。
- `FLISTWALKER_RESTORE_TABS=1` による opt-in のタブセッション復元を追加した。
- macOS 向け `.app` / zip / tar.gz のリリースアセット生成スクリプトを追加した。

### Changed
- `rust/src/app.rs` を `tests` / `workers` / `session` / `input` / `render` に段階分割し、責務境界を明確化した。
- `Shift+Enter` の格納フォルダオープンは同一フォルダを重複起動しない挙動に統一した。
- README とリリース導線を Windows/macOS 両対応前提へ更新した。

### Fixed
- index/search/preview/action の並行処理を hardening し、古い応答で UI 状態が巻き戻る経路を抑制した。
- worker shutdown と join timeout を見直し、終了時のハングや stale request の取り残しを抑えた。
- incremental query narrowing 用の prefix candidate cache を追加し、絞り込み中の検索応答を改善した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。

## [0.8.0] - 2026-03-04
### Added
- 

### Changed
- 階層 FileList 展開の判定を「全ディレクトリ探索」から「読み込み済み候補内の `FileList.txt` / `filelist.txt` 完全一致検出」へ変更。
- 階層 FileList 展開の処理キューを浅い階層優先の優先度キューへ変更し、多数のネスト FileList での探索効率を改善。

### Fixed
- 階層 FileList 展開中の子 FileList 解析でも supersede による中断が効くように修正。
- FileList ストリーミング時の不要な全件保持を減らし、メモリ圧迫に起因する体感遅延を抑制。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.7.2] - 2026-03-04
### Added
- 

### Changed
- 

### Fixed
- `Use FileList` 有効時でも、実際のソースが Walker fallback の場合は `FileList` OFF 時と同じ操作系で扱うようにした。
- ファイル/フォルダの open/execute を UI スレッド外の worker へ移し、遅い I/O で画面が固まる問題を修正した。
- Preview 境界ドラッグ中に Results 一覧が巻き込まれてスクロールしてしまう問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- notarization 前の macOS アプリは Gatekeeper 警告の対象になる場合がある。

## [0.7.1] - 2026-03-04
### Added
- 

### Changed
- 

### Fixed
- GUI ワーカーに shutdown フラグ共有と `Drop` 時の `join` を追加し、中断時の終了処理を明示化。
- `Ctrl+C` などのシグナル受信で shutdown 要求を立て、GUI 側で window close へ収束させる終了経路を追加。
- walker インデックスのキャンセル確認間隔を短縮し、多数エントリ走査中の中断反映を改善。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.7.0] - 2026-03-03
### Added
- 

### Changed
- `Files` / `Folders` 切り替え時は、現在のトグル状態（include flags）で再インデックスする動作へ変更。
- `Use FileList` 有効時の `Create File List` は、FileList 優先フローの整合性を維持する実行順へ調整。

### Fixed
- Root 変更時にタブタイトルが即時追従しない問題を修正し、メモリ表示の定期更新も安定化。
- ウィンドウリサイズ時に Preview 幅が意図せず変わる問題を修正。
- 検索窓フォーカス系ショートカットと Tab 操作の回帰を修正。
- `Use FileList` 利用中に `Files` / `Folders` フィルタを操作できてしまう不整合を解消（無効化）。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.6.0] - 2026-03-02
### Added
- ステータスバーにプロセスのメモリ使用量表示を追加した。

### Changed
- walker を並列走査するようにし、大規模ツリーの探索スループットを改善した。
- `FileList` I/O 削減、path kind 判定の遅延、preview cache の総バイト上限化、タブ間の重複データ削減でメモリ使用量を抑制した。
- walker cap と調整用環境変数を引き上げ、チューニング方針を見直した。

### Fixed
- アクティブタブ状態の毎フレーム clone や一時バッファ肥大化に起因する無駄な負荷を減らした。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.5.1] - 2026-02-27
### Added
- タブタイトル上でのマウス中クリックによるタブクローズ操作を追加。

### Changed
- 

### Fixed
- OR 条件（`|`）で左右それぞれに `'` を付けたクエリ（例: `'abc|xyz`, `abc|'xyz`, `'abc|'xyz`）を検索仕様どおりに評価するよう修正。
- 上記クエリで左辺/右辺に `'` を付けた場合に、可視判定とハイライト表示が崩れる問題を修正。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.5.0] - 2026-02-28
### Added
- 検索クエリ入力欄に OR 条件（`|`）のヒントをマウスポインタ位置へ表示するツールチップを追加した。
- Results 一覧で OR 条件ごとのハイライト表示に対応した。

### Changed
- `Use FileList` 有効時は、FileList のストリーミング読み込みを優先しつつ、パス種別判定を遅延する方式へ変更した。

### Fixed
- Preview 非表示時に不要なプレビュー I/O を行わないようにした。
- 検索欄フォーカス中でも `Ctrl+N` / `Ctrl+P` / `Ctrl+Shift+C` / `Ctrl+G` を使えるように再調整した。
- `Tab` / `Shift+Tab` のピン操作と、タブ単位の index/search スケジューリングを安定化した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.4.0] - 2026-02-25
### Added
- 

### Changed
- 

### Fixed
- 日本語 IME の通常入力モードと日本語入力モードの両方で、Space 入力が安定して通るように修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.3.3] - 2026-02-25
### Added
- 

### Changed
- デバッグトレースは既定で無効化し、必要時のみ有効化する運用へ変更した。

### Fixed
- インデクシング中のストリーミング更新を安定化し、大規模候補での待ち時間を短縮した。
- IME 入力とウィンドウ操作が競合したときに、入力やフォーカス挙動が不安定になる問題を修正した。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.3.2] - 2026-02-24
### Added
- 

### Changed
- 

### Fixed
- ライトテーマ時に Results の非ハイライト文字色が薄すぎる問題を修正し、可読性を改善。
- 大規模 FileList 読み込みで `Source: None` 表示が長時間残り、再読込反映が遅延しやすい問題を修正（ストリーミング解析 + supersede 中断対応）。
- `Create File List` は一時ファイルをローカル temp に生成してから最終配置へ移動する方式に変更（クロスデバイスは copy フォールバック）。
- 検索窓フォーカス中の `Ctrl+Shift+C` で検索文字列が優先コピーされる回帰を再修正（後段でパスコピー実行）。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.3.1] - 2026-02-23
### Added
- 

### Changed
- 

### Fixed
- Preview の内容が長い場合でも Preview 領域外へ描画がはみ出さないように修正し、フッター（`Entries` / `Results`）表示が隠れないよう改善。
- `Copy Path(s)` 実行時のフッター通知 `Copied path:` から Windows 拡張パス接頭辞（`\\?\`）を除去。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.3.0] - 2026-02-20
### Added
- Root 行に `Set as default` を追加し、次回起動時の既定 Root を保存・適用。
- 既存 `FileList.txt` / `filelist.txt` がある場合の上書き確認ダイアログを追加。

### Changed
- ウィンドウ位置/サイズ、Results/Preview 分割幅、Preview 表示状態を永続化。
- `Create File List` は再 Walker を行わず、最新インデックス結果を再利用する方式へ変更。
- インデックス中の `Create File List` は完了後に遅延実行する動作へ変更。
- 結果行の自動スクロールを「可視範囲外になった時のみ追従」に調整。
- 複数トークン検索時、各トークンの完全一致候補を優先しやすいスコアリングへ調整。

### Fixed
- `Entries` 表示が Root 変更/Refresh Index 後に古い件数を保持する問題を修正。
- `Ctrl+L` の検索窓フォーカス動作をトグル化。
- 検索窓フォーカス中の `Ctrl+Shift+C` で検索文字列がコピーされる問題を修正（Results パスのみコピー）。
- 全角入力時に Space/Shift+Space が効かない問題を修正（半角スペース挿入）。
- FileList 作成中に Root 変更が発生しても UI 状態を巻き戻さないよう安定化。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.2.1] - 2026-02-19
### Added
- 結果一覧で `Ctrl+V` / `Alt+V` によるページ移動を追加。
- `FileList` のパス解釈を拡張し、バックスラッシュ相対パス・引用符付き行・Windows絶対パス（WSL変換含む）を許容。

### Changed
- `Use FileList` の初期値を OFF に変更。
- `Files` / `Folders` は再インデックスではなく表示フィルタとして即時反映する動作へ変更。
- リリースzip同梱の実行ファイル名を `flistwalker.exe` に変更。
- リリースzip同梱 `README.txt` を拡充（操作手順・検索演算子・ショートカット・オプション説明）。

### Fixed
- インデクシング中にオプション切替すると `Source: None` 固定や更新停止が起きる問題を修正（旧リクエストの中断制御を追加）。
- インデクシング中のスクロール位置が先頭へ戻る問題を修正。
- オンデマンドファイルに対する無駄なプレビュー読み込みを抑止。
- `Tab` 複数選択時に検索窓とボタン間でフォーカスが不安定に移動する問題を緩和。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.2.0] - 2026-02-18
### Added
- FileList 作成フローの状態遷移テスト（成功/失敗/再インデックス）を追加。
- CLI 契約テストを拡張（スコア付き出力、0件、不正 root）。

### Changed
- インデックス中の逐次表示方針を改善し、空クエリ時は即時反映、非空クエリ時は時間窓で間引き反映する方式に更新。
- 正規表現検索はクエリ単位で事前コンパイルして再利用。

### Fixed
- Windows で `.ps1` を選択した際に実行されない問題を修正（PowerShell 明示起動）。
- 無効な正規表現が「0件ヒット」に見える問題を修正し、GUI notice へ検索エラーを表示。
- 起動直後/Root 変更直後に結果が一瞬表示後に消える不具合を修正。
- プレビューキャッシュの上限未設定によるメモリ増加リスクを修正（FIFO 上限）。

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

## [0.1.0] - 2026-02-17
### Added
- 初回公開。
- Windows 向けに `exe単体 + zip` を提供。

### Changed
- Rust 版をメイン実装として整理。

### Fixed
- 

### Breaking
- 

### Deprecated
- 

### Security
- 

### Known issues
- macOS アセットは未提供。

[Unreleased]: https://github.com/ShinjiKawamura255/flist-walker/compare/v0.12.2...HEAD
[0.12.2]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.12.2
[0.12.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.12.1
[0.12.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.12.0
[0.11.6]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.11.6
[0.11.5]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.11.5
[0.11.4]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.11.4
[0.11.3]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.11.3
[0.11.2]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.11.2
[0.11.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.11.1
[0.11.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.11.0
[0.10.3]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.10.3
[0.10.2]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.10.2
[0.10.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.10.1
[0.10.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.10.0
[0.9.2]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.9.2
[0.9.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.9.1
[0.9.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.9.0
[0.8.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.8.0
[0.7.2]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.7.2
[0.7.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.7.1
[0.7.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.7.0
[0.6.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.6.0
[0.5.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.5.1
[0.5.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.5.0
[0.4.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.4.0
[0.3.3]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.3.3
[0.3.2]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.3.2
[0.3.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.3.1
[0.3.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.3.0
[0.2.1]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.2.1
[0.2.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.2.0
[0.1.0]: https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.1.0
