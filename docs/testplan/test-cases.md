# Test Cases

## Test cases
| TC ID | Level | Purpose | Related SP |
| --- | --- | --- | --- |
| TC-001 | unit | `FileList.txt` / `filelist.txt` の優先検出 | SP-001 |
| TC-002 | unit | リスト未存在時に walker 走査で候補生成 | SP-002 |
| TC-003 | unit | ファジー検索が関連度順・件数制限を満たす | SP-003 |
| TC-003A | unit | 通常語の各 token は subsequence-only よりリテラル一致を高順位にする | SP-003 |
| TC-004 | unit | 実行可能ファイルで execute 分岐 | SP-004 |
| TC-004A | unit | open / execute helper seam が open/execute handler を適切に呼び分ける | SP-004, SP-005 |
| TC-005 | unit | フォルダで open 分岐 | SP-005 |
| TC-006 | integration | CLI 契約（`--cli`/`--root`/`--limit`）を満たす | SP-006 |
| TC-006A | integration | CLI の `--limit` は 1000 件へ暗黙丸めせず、そのまま実効上限として扱う | SP-006 |
| TC-007 | perf | 10万件検索の遅延計測 | SP-007 |
| TC-008 | unit | 例外時に非ゼロ終了コードを返す | SP-008 |
| TC-009 | unit | モジュール分離により単体テスト可能 | SP-009 |
| TC-010 | manual+unit | GUI で検索/選択/実行/再読込が行える（`docs/GUI-TESTPLAN.md` の `GSM-001` から `GSM-006`） | SP-010 |
| TC-011 | manual | GUI 回帰手順に基づく再検証が可能（`scripts/gui-smoke-fixture.sh` と `docs/GUI-TESTREPORT.template.md` を使用） | SP-011 |
| TC-012 | unit | プレビューキャッシュが上限件数を超えない | SP-007 |
| TC-012A | unit | `.vbs` など非 `.txt` でも UTF-8 / UTF-16 / 主要レガシー文字コードなら本文プレビューでき、バイナリは unreadable を維持する | SP-010 |
| TC-013 | manual | インデックス中に空クエリ一覧が逐次更新され、操作が継続できる | SP-002, SP-007 |
| TC-014 | unit | FileList 作成の成功/失敗応答で GUI 状態遷移が正しい | SP-010, SP-008 |
| TC-015 | integration | CLI 非空クエリ出力形式と root 不正系を検証する | SP-006, SP-008 |
| TC-016 | unit | Root 変更時に旧 Root 由来の選択状態（current/pinned/preview）がクリアされる | SP-010 |
| TC-017 | unit | Root 変更時に旧 Root 向け FileList 上書き確認が破棄される | SP-010 |
| TC-018 | unit | `Ctrl+Shift+C`（macOS では `Cmd+Shift+C`）は、backend が `Key::C` または `Event::Copy` のどちらで通知しても検索窓フォーカス時に結果パスを優先コピーする | SP-010 |
| TC-019 | unit | 大規模 FileList 解析はストリーミングで中断可能（新 request で supersede） | SP-001, SP-007 |
| TC-020 | manual+unit | Windows: 異解像度モニタ跨ぎ時に復元サイズが破綻せず、IME `Space` / `Shift+Space` で最低限の半角スペースが検索窓へ反映される（`CompositionUpdate` 同フレームでも挿入される） | SP-010, SP-011 |
| TC-021 | unit | 検索窓フォーカス中でも `Ctrl+N` / `Ctrl+P` が current row を移動する | SP-010 |
| TC-022 | unit | 検索窓フォーカス中でも `Ctrl+G` / `Esc` で query clear + filter reset が実行される | SP-010 |
| TC-023 | unit | `Tab` / `Shift+Tab` はフォーカス非依存で PIN 固定/解除のみ実行し current row を維持する | SP-010 |
| TC-023A | unit | tab 切替や `Esc` 系の reset 後も、結果がある場合は visible な current row が復元される | SP-010 |
| TC-024 | unit | IME スペースフォールバックと composition commit fallback はカーソル位置へ挿入し、挿入後カーソルへ更新する | SP-010 |
| TC-025 | unit | FileList ストリーミング時に種別不明候補を先行表示し、種別解決後に FILE/DIR/LINK 表示とフィルタ状態を反映する | SP-001, SP-010, SP-007 |
| TC-026 | unit | 回帰: 検索窓フォーカス中でも `ArrowUp` / `ArrowDown` で current row が移動する | SP-010 |
| TC-027 | unit | 回帰: `Ctrl+I` は検索窓フォーカス有無を問わず PIN をトグルし current row を維持する | SP-010 |
| TC-028 | unit | 回帰: 検索窓フォーカス中でも `Ctrl+J` / `Ctrl+M` が `Enter` 同等に実行/オープンを起動する | SP-010 |
| TC-029 | unit | プロセス shutdown フラグが set/clear 可能で、シグナル経由の終了要求を保持できる | SP-010, SP-008 |
| TC-030 | unit | 階層 FileList 展開は候補集合中の `FileList.txt` / `filelist.txt` 完全一致エントリのみを対象にし、子 FileList 解析中も supersede で中断できる | SP-001, SP-007 |
| TC-031 | unit | indexing 中の非空クエリ自動再検索は小差分では抑制し、十分な差分蓄積時のみ再検索する | SP-007, SP-010 |
| TC-032 | unit | GUI 終了時の worker join はタイムアウト付きで、完了系/タイムアウト系の両方を検証する | SP-008, SP-010 |
| TC-033 | unit | `Ctrl+R` で履歴検索モードを開始し、履歴候補を最新順で表示する | SP-010 |
| TC-034 | unit | query 履歴は空文字と連続重複を保存しない | SP-010 |
| TC-035 | unit | query 履歴はタブ切り替え後も全タブで共通に参照できる | SP-010 |
| TC-036 | unit | Root 変更時は query 履歴の参照位置のみリセットし、履歴本体は保持する | SP-010 |
| TC-037 | unit | 連続打鍵中は query 履歴を保留し、入力区切り後に最終 query だけを保存する | SP-010 |
| TC-038 | unit | IME 合成中の未確定文字列は query 履歴へ保存せず、確定後の query のみを保存する | SP-010 |
| TC-039 | unit | 保存済み `last_root` / `default_root` / tab root は root 存在確認を起動後へ遅延し、active tab index をクランプする | SP-010 |
| TC-040 | unit | タブ復元時は active tab の root/query/filter を復元し、起動直後の index は active tab だけ開始する | SP-010 |
| TC-041 | unit | 復元された background tab は初回 activate 時にだけ lazy refresh を開始する | SP-010 |
| TC-042 | unit | 履歴検索中の `Enter` / `Ctrl+J` / `Ctrl+M` は選択中履歴を query に展開する | SP-010 |
| TC-043 | unit | 履歴検索中の `Esc` / `Ctrl+G` は開始前 query を復元してキャンセルする | SP-010 |
| TC-044 | unit | `Ctrl+Shift+R` は履歴検索を開始しない | SP-010 |
| TC-045 | unit | query 履歴は 100 件上限を維持し、保存状態へ永続化される | SP-010 |
| TC-046 | unit | runtime config の `restore_tabs_enabled` が有効な間は `Set as default` が無効化され、既定 root を保存しない | SP-010 |
| TC-046A | unit | `Manage list` は draft list への追加・選択削除を `Apply` / `OK` でのみ保存済み root list へ反映し、`Cancel` では破棄する。保存済み default root を draft から削除して反映した場合は default root も解除する | SP-010 |
| TC-047 | unit | FileList 作成時は祖先の既存 FileList へ子 FileList 参照を重複なく追記し、親 mtime を維持する | SP-001 |
| TC-048 | unit | 祖先探索は callback/失敗時に即停止し、それ以降の上位階層を処理しない | SP-001 |
| TC-049 | unit | FileList に root 外パスが含まれても候補表示時の追加フィルタを行わず、インデクシング経路の挙動を維持する | SP-007 |
| TC-050 | unit | root 外パスは結果一覧に存在しても execute/open 直前で拒否される | SP-004 |
| TC-051 | unit | UNC root 配下のパスは execute/open 直前の root 判定で許可される | SP-004 |
| TC-052 | unit | Create File List で祖先 FileList への追記が実際に発生しうる場合は確認ダイアログを要求する | SP-001, SP-010 |
| TC-053 | unit | 祖先追記確認を拒否した場合、root 直下の FileList 作成だけを継続する | SP-001, SP-010 |
| TC-054 | unit | runtime config の `history_persist_disabled` が有効なとき query history を保存も復元も行わない | SP-010 |
| TC-055 | manual | README / release docs / release template に平文 history 保存、checksum 検証、notarization の暫定運用と `Security` / `Known issues` 記載前提が明記されている | SP-010, SP-012 |
| TC-056 | integration | CI は Linux/macOS/Windows を対象にし、`cargo audit` を実行する | SP-012 |
| TC-056A | docs+security | `cargo audit` の accepted transitive warning は `docs/OSS_COMPLIANCE.md` に依存経路、受容理由、owner、review cadence、再評価 trigger を記録する | SP-012 |
| TC-057 | unit | `Score` / `Name` / `Modified` / `Created` のソートモード遷移と `Score` 復帰を検証する | SP-013 |
| TC-058 | unit | query 変更や結果更新時にソートを破棄し、インデクシング経路へ属性取得を追加しない | SP-013 |
| TC-059 | unit | 日付ソートの未キャッシュ path は別 worker へ要求し、古い応答は request_id で破棄する | SP-013 |
| TC-060 | unit | sort metadata cache は上限を超えて増加せず、古い項目を破棄する | SP-013 |
| TC-061 | unit | `created()` 取得不可の項目は `Created` ソート時に末尾へ送る | SP-013 |
| TC-062 | manual | 結果ペインの `Sort` UI から各ソートを選択でき、`Modified` / `Created` 中も入力が継続できる | SP-010, SP-013 |
| TC-063 | perf | ソート機能追加後もインデクシング時の属性取得数を増やさず、既存の逐次表示挙動を維持する | SP-007, SP-013 |
| TC-064 | unit | タブのドラッグ移動は並び順のみを更新し、active tab の実体と保存順を維持する | SP-010 |
| TC-068 | unit | 回帰: 検索結果更新で current row は行番号を維持し、結果数縮小時のみ末尾へ丸める | SP-010 |
| TC-069 | unit | 回帰: `Ignore Case` は既定で有効で、無効化時は検索結果とハイライトが case-sensitive になる | SP-010 |
| TC-070 | unit | 回帰: 起動直後と検索キャンセル後は候補がある場合に 1 行目が既定選択になる | SP-010 |
| TC-071 | unit | 大規模候補でも partial top-N の結果が full ranking の先頭と一致する | SP-003, SP-007 |
| TC-072 | unit | 並列検索の収集結果は逐次検索と同じ ranking を返す | SP-003, SP-007 |
| TC-073 | unit | 非アクティブタブの結果キャッシュ compact 後も、再表示時に current row と結果一覧を復元できる | SP-010 |
| TC-074 | unit | GitHub Releases の latest 応答から現在 platform の更新 asset と sidecar 文書（README / LICENSE / THIRD_PARTY_NOTICES）と `SHA256SUMS` を選択できる | SP-014, SP-017 |
| TC-075 | unit | staged binary と sidecar 文書は `SHA256SUMS.sig` の署名検証と `SHA256SUMS` の checksum 検証を通過して private verified bundle になった場合のみ自己更新へ進む | SP-014 |
| TC-076 | unit | Windows/Linux の自己更新は補助 updater 経由で target binary 置換コマンドを生成し、実行中 binary の直接上書きを避け、helper script は no-overwrite creation を使う | SP-014 |
| TC-077 | unit | macOS は新版検知時も自動置換へ進まず、手動更新案内に留める | SP-014 |
| TC-078 | unit | 更新確認/ダウンロード失敗は notice に反映されても GUI 操作を継続できる | SP-014 |
| TC-079 | manual+unit | 手動試験 override により同一 version でも更新ダイアログを表示できる | SP-014 |
| TC-080 | manual+unit | 手動試験 override により downgrade 候補でも更新ダイアログを表示できる | SP-014 |
| TC-081 | unit | 更新ダイアログで抑止した target version は起動間で保持され、より新しい version が出るまで再表示されない | SP-014 |
| TC-082 | unit+perf | 回帰: FileList の `\` 区切り候補を filesystem existence probe なしでプラットフォーム優先解釈し、line-only fast path が metadata-probe baseline を十分に上回ることを維持する（CI 下限 1.20x）。FileList read path は行バッファを再利用し、`lines()` による行ごとの確保 baseline より遅くならないことを明示計測する | SP-001, SP-007 |
| TC-083 | unit+perf | Walker 初期インデクシングは通常ファイル/ディレクトリを `file_type` ベースで流し、eager metadata 解決に対して現行 control baseline で 1.25x 以上の速度差を維持し、その後に遅延種別解決を自動開始する | SP-002, SP-007 |
| TC-084 | unit | Source が FileList のタブで Create File List を確認すると、新規タブを開かずに同一タブの裏で Walker indexing を実行する | SP-001, SP-010 |
| TC-085 | unit | FileList source の Create File List 完了時、元タブが background 化していてもその元タブを再インデックスし、完了前に元タブ root が変わっていた場合は再インデックスしない | SP-001, SP-010 |
| TC-086 | unit | 実行バイナリと同一ディレクトリに `FLISTWALKER_DISABLE_SELF_UPDATE` ファイルがある場合も、自己更新を無効化する | SP-014 |
| TC-087 | unit | Create File List の保留状態は status panel または確認ダイアログ経由でキャンセルできる | SP-001, SP-010 |
| TC-088 | unit | Create File List 実行中のキャンセル要求は cancel flag を立て、`Canceled` 応答で状態と notice を解放する | SP-001, SP-010 |
| TC-089 | unit | Create File List の cancel 済み request は root 直下の既存 `FileList.txt` を置換しない | SP-001 |
| TC-090 | unit | 回帰: GNU Windows ビルドは `resource.o` を最終 `flistwalker` バイナリへ明示リンクし、Explorer アイコン欠落を防ぐ | SP-012 |
| TC-091 | unit | 回帰: GUI 終了時の worker join timeout は短時間に保たれ、close が不要に 2 秒近く遅延しない | SP-008, SP-010 |
| TC-092 | unit | 回帰: shared query module により search と highlight が exact / OR / anchor の query 解釈を一致させる | SP-003, SP-010 |
| TC-093 | unit | regex モードでも plain token / plain OR token は fuzzy 条件で一致し、regex 構文 token だけ regex として評価される | SP-003 |
| TC-094 | unit | タブ accent color は保存・復元時に維持される | SP-010 |
| TC-095 | unit | tab accent palette は light/dark theme で DropSendTo の Slot Color と同じ RGB を返す | SP-010 |
| TC-096 | unit | 起動時更新確認失敗は失敗ダイアログへ反映され、通常操作継続のため pending/in_progress を解放する | SP-014 |
| TC-097 | unit | 起動時更新確認失敗ダイアログの抑止設定は UI state に保存され、抑止中は失敗ダイアログを再表示しない | SP-014 |
| TC-098 | unit | `FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE` はネットワーク非依存で起動時更新確認失敗を強制し、失敗ダイアログ抑止中でも表示できる | SP-014 |
| TC-099 | manual | structural refactoring 後も GUI の主要操作が連続利用に耐える（`GSM-001` から `GSM-010`） | SP-010 |
| TC-100 | unit | self-update candidate 解決は release asset 選択と support classification を分離し、manual-only fallback を契約として保持する | SP-014 |
| TC-101 | unit | update request / install transitions emit trace commands for supportability and retain request_id correlation | SP-014 |
| TC-102 | unit | Create File List の stale requested root completion は cleanup だけを行い、`use_filelist` 復帰や notice 更新を行わない | SP-001, SP-010 |
| TC-103 | unit | Root 変更時に旧 Root 向け FileList の祖先追記確認と Walker 利用確認が破棄される | SP-001, SP-010 |
| TC-104 | unit | render command 経由の tab close / reorder は active tab identity と queue cleanup を壊さない | SP-010 |
| TC-105 | unit | root dropdown selection は popup を閉じて選択 root を適用し、後続の root cleanup flow を開始できる | SP-010 |
| TC-106 | unit | background response routing は active tab を巻き戻さず、tab close 時は request routing cleanup だけを行う | SP-010 |
| TC-107 | unit | `ui_model` の preview text は action policy を埋め込まず、実行可否は `actions.rs` 側の責務に留める | SP-004, SP-010 |
| TC-108 | ci | `cargo llvm-cov` の line coverage gate は 75% 未満への退行を CI で失敗させ、`lcov.info` artifact を継続生成する | SP-012 |
| TC-109 | docs | GitHub Issue template と `docs/SUPPORT.md` は、version / OS / launch mode / reproduction / redaction を求めつつ、既定 telemetry や自動 crash upload を導入しない supportability 導線を固定する | SP-010, SP-012 |
| TC-110 | unit | ignore list ファイルの候補は既定有効で除外され、GUI の `Use Ignore List` チェックボックスで有効/無効を切り替えられる | SP-015 |
| TC-111 | unit | runtime config file が無い初回起動では current env を seed にした `~/.flistwalker/.flistwalker_config.json` が自動生成され、既存 file がある場合は env 変更より file 内容が優先される | SP-016 |
| TC-112 | unit | 回帰: `Files` / `Folders` 両方有効の高速経路でも ignore list は省略されず、`old` / `~` を literal に含む候補が結果へ戻らない | SP-015 |
| TC-113 | unit | 起動時初期化は embedded ignore sample を `flistwalker.ignore.txt.example` として生成し、既存ファイルを上書きしない | SP-017 |
| TC-114 | unit | embedded ignore sample の説明文には `flistwalker.ignore.txt` へリネームして live ignore list として使う案内が含まれる | SP-017 |
| TC-115 | unit | Windows では runtime config / UI state / saved roots / window trace の保存先が `%LocalAppData%\flistwalker\` になり、Linux/macOS では `~/.flistwalker/` に戻る | SP-016 |
| TC-116 | unit | Windows の旧 exe-side / home-directory 配置ファイル、Linux/macOS の旧 home-directory 直下ファイルは、新保存先に同名 file が無い場合だけ移行され、既存の新配置 file を上書きしない | SP-016 |
| TC-117 | unit | `!` 除外と ignore list は fuzzy fallback を使わず、literal / anchor 一致だけで除外し、`Ignore Case` の有効/無効に従う | SP-015 |
| TC-118 | unit | Windows の一般 `.ps1` は Execute ではなく Open 分岐を選ぶ | SP-004 |
| TC-119 | unit | `SHA256SUMS.sig` は改ざんされた manifest を検証失敗にする | SP-014 |
| TC-120 | manual | diagnostics trace smoke: `RUST_LOG` と `FLISTWALKER_WINDOW_TRACE` の責務分離を維持し、対象 flow が canonical event family と相関 field を出す | SP-010, SP-014 |
| TC-121 | unit | Windows regression: `copy_selected_paths` の notice 正規化は `app.shell.runtime.notice` を更新し、旧 shell 直参照の残骸を検出する | SP-010 |
| TC-122 | unit | 回帰: Walker 完了後の kind 解決は表示中結果に限定し、巨大な on-demand root で全件 metadata 解決を継続しない | SP-007 |
| TC-124 | unit+perf | adaptive walker は唯一の backend として同一 dataset の候補件数を標準 read_dir 走査と一致させつつ elapsed time と read_dir 制御指標を診断出力できる。adaptive limit は探索方向を保持し、減少で改善した場合はさらに減少、増加で悪化しない場合は増加探索を継続し、悪化時は方向反転する。Windows の Hidden+System+ReparsePoint 互換 junction は候補化せず、single-worker serial fast path は同じ候補を返して metrics を記録する | SP-002, SP-007, SP-016 |
| TC-125 | unit | `developer` runtime config は手動追記時だけ読み込まれ、自動生成 config seed には出力されない。自動生成 config seed には `walker_max_entries` / `history_persist_disabled` / `restore_tabs_enabled` / `emacs_keybindings_enabled` が既定値で含まれ、既存 config に欠けている場合も読み込み時に補完される。`walker_threads` / `walker_backend` は既存 config から削除され、adaptive walker の initial/max limit 未指定時の既定と明示値を検証し、metrics summary を指定ファイルへ追記できる。`adaptive_limit_avg` と `adaptive_limit_change_count` を含む summary 形式を固定し、平均並列度の読み取り手順を残す | SP-002, SP-016 |
| TC-126 | unit+perf | adaptive walker の制御メトリクスは `adaptive_limit_final` だけでなく `adaptive_limit_avg` と `adaptive_limit_change_count` を出力し、再測定時に平均並列度・揺れ幅・最終値を比較できる。`adaptive_limit_avg` は shutdown/join 尾を少量含みうることを前提に解釈する | SP-002, SP-016 |
| TC-127 | unit | GUI 設定ボタンの handler は runtime config file を生成済みのパスとして解決し、既定アプリ open が失敗した場合にテキストエディタ相当の fallback handler を呼ぶ | SP-016 |
| TC-128 | unit | active indexing の `Finished` 応答時に未反映 entries が残っている場合、GUI frame は全件 drain せず、複数 frame に分けて吸収した後に terminal state へ遷移する | SP-010 |
| TC-129 | unit | Walker が上限打ち切り後に `Truncated` と `Finished` を返し、大きな pending entry backlog が残っていても、GUI は frame budget を維持しながら少数 frame で terminal state へ到達する | SP-010 |
| TC-130 | unit | 空クエリ・フィルタなしの active indexing は、表示更新時に `incremental_filtered_entries` と `runtime.entries` へ全件複製せず、`runtime.index.entries` から上位 `limit` 件だけを表示する | SP-010 |
| TC-131 | unit | active indexing の `Finished` 応答後に pending entry drain が残っていても、status line は `Indexing...` を表示せず、内部後処理 marker により repaint と terminal cleanup は継続する | SP-010 |
| TC-132 | unit | active indexing の terminal cleanup は、大規模 drain 後の `pending_entries` queue を同期的に shrink せず、後処理完了時の UI freeze を避ける | SP-010 |
| TC-133 | unit | notice 消去を含む通常 status line 更新は OS メモリ使用量を同期サンプリングせず、memory sampling は専用更新経路でのみ行う | SP-010 |
| TC-134 | unit | `Finished` 後の pending entry drain は探索中より小さい 1-frame 件数上限で分割され、notice 消去直前に大きな backlog を単一 frame で吸収しない | SP-010 |
| TC-135 | unit | フィルタ適用中の active indexing 完了時は、ingest 中に作った増分 filtered snapshot を terminal `entries` に再利用し、全候補を再走査しない | SP-010 |
| TC-136 | unit | active indexing 中の空クエリ・フィルタなし表示更新は、蓄積済み index entries を `runtime.entries` へ全件 clone せず、表示上限分の results のみ更新する | SP-010 |
| TC-137 | unit | kind filter 用 unknown path queue 構築は、既知 kind / pending / in-flight を重複投入せず、queue 対象 path だけを追加する | SP-010 |
| TC-138 | unit | 保存済み window 位置が現在の表示範囲外にある場合は起動位置を表示範囲内へ補正し、現在の仮想ディスプレイ内の負座標は維持する | SP-010 |
| TC-139 | unit | タブ close ボタンの hover 表示は、タブ accent または clear outline と同系色の背景・細い枠・カーソルで押下領域をタブ本体と区別できる | SP-010 |
| TC-140 | unit | `FLISTWALKER_DISABLE_SELF_UPDATE=1` では起動時更新確認を送らず、更新チェックも `None` を返して自己更新を無効化する | SP-014 |
| TC-141 | unit | runtime config の `emacs_keybindings_enabled=false` では `Ctrl+N` / `Ctrl+P` / `Ctrl+V` / `Alt+V` が current row を移動せず、通常の矢印移動と `PageUp` / `PageDown` は維持される | SP-010, SP-016 |
| TC-142 | unit | runtime config の `emacs_keybindings_enabled=false` では `Ctrl+J` / `Ctrl+M` が実行/オープンを起動せず、通常の `Enter` は維持される | SP-010, SP-016 |
| TC-143 | unit | runtime config の `emacs_keybindings_enabled=false` では検索欄フォーカス中の `Ctrl+K` など TextEdit 側の Emacs 風編集 chord も query を変更しない | SP-010, SP-016 |
| TC-144 | unit | `Ctrl+Shift+T` は現在プロセス中に閉じたタブを LIFO で active tab として復元し、可能な限り閉じる前の tab index へ戻し、復元対象がない場合はタブ構成を変更しない。復元タブは新しい tab id を持ち、古い pending request/routing 状態を復元しない。閉じたタブ stack は直近 25 件を上限にする。起動時復元された background tab を初回 activate 前に閉じて復元しても lazy refresh を実行する | SP-010 |
