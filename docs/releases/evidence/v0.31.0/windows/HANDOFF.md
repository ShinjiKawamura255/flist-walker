# v0.31.0 Windows確認準備

状態: **準備のみ。Windows実画面・TC-193はNOT RUN。タグ・公開は行わない。**

候補にはmacOSビルドの固定Rust版不一致とmaster CIの未解決警告があり、公開可能とは判定されていない。Windowsの独立確認材料として保持する。後続の修正版候補へbindingや結果を自動転記せず、source/build/実バイナリを再照合する。

対象はsource `3194f03c27f9a72f06a836dbd02e9141d8740a26`、[候補run39087986481](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/39087986481)、attempt1。結果の正本は[候補パケット](../PREPARATION.md)。別ビルド、インストール済み実行ファイル、古い候補へ置き換えない。

## 実行前の条件

1. Windows x86_64の通常負荷のあるロック解除済みデスクトップ、PowerShell、標準Python3、`gh`の既存認証を用意する。追加インストールやOS安全機能の無効化は行わない。GUI起動とOS外観変更は、対象の隔離セッションについて明示承認を受けてから行う。
2. `CANDIDATE-BINDING.json`、manifest/signature、バイナリのhashを照合する。候補全28assetの配布検証と暗号署名検証はmacOS側の記録を参照し、Windowsの新規PASSに言い換えない。
3. 候補はGit管理外の新しい専用ディレクトリへ取得する。artifact ID/expiry/digestはbindingに従う。コピー前に既存のアプリを列挙し、既存プロセス・設定には触らない。

```powershell
gh run download 39087986481 --repo ShinjiKawamura255/flist-walker --name validated-release-bundle-v0.31.0-3194f03c27f9a72f06a836dbd02e9141d8740a26 --dir .\candidate-bundle
Get-FileHash -Algorithm SHA256 .\candidate-bundle\SHA256SUMS
Get-FileHash -Algorithm SHA256 .\candidate-bundle\SHA256SUMS.sig
Get-FileHash -Algorithm SHA256 .\candidate-bundle\FlistWalker-0.31.0-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\candidate-bundle\fw-0.31.0-windows-x86_64.exe
```

bindingとhelperは同一パケットのものを使う。同梱ハンドオフZIPの`candidate-bundle`はWindows用8ファイルのsubset（両exe、universal zip、3 sidecars、全体manifest、署名）であり、全28配布物ではない。同梱subsetがある場合は上の`gh run download`を実行せず、hash照合から進める。取得コマンドはディレクトリがまだない場合だけ使う。全28配布物が必要なら別の新規ディレクトリへ取得する。artifact期限切れなら元候補を勝手に再生成せず、release ownerへ戻す。

## TC-193（GUIとは独立）

共有runtime-config初期化が変わったため、v0.30.2の値は再利用しない。付属helperはこのsource/run/attemptだけを許可する。初回結果・部分失敗・warmupを保持し、既存output/rawファイルへの上書きを拒否する。

```powershell
py -3 .\verify-windows-candidate.py --bundle .\candidate-bundle --binding .\CANDIDATE-BINDING.json --output .\windows-verification.json
```

- 固定shallow200-file fixture、内容は`owned TC-193 fixture`と改行、名前file_000.txt〜file_199.txt。
- 同一環境/fixture、5warmup+25測定/variant、順序を交互にする。universalにだけ`--cli`を加える。argv配列、直接child process、stdout/stderr redirect、同期終了までmonotonic timerで測定する。
- `fw median / universal median <= 0.70`。60回のexit0、stderr空、200件の内容と順序/framing一致を必須にする。raw JSONLはalias/versionも含むので、TC-193のargvで60件を抽出する。
- 直前/直後のCPU・空きメモリ、Windows build/CPU/Python、検証開始終了時刻を別途保存する。計測中は別build/install/testを始めない。
- FAIL/timeoutは原結果・rawを返す。通るまでの反復、sample/warmup/閾値変更は禁止。helperの時間計測後にrawを書き、記録のI/Oを測定区間へ入れない。
- helperは一時profileのみHOME/USERPROFILE/LOCALAPPDATA/APPDATAを差し替え、childだけ自己更新を無効化する。実更新・外部Open/Reveal/Execは行わない。終了時に所有した一時fixture/profileのみ削除する。

## Windows資産検査

候補そのものをobjdump/strings等で確認する。universal=PE32+ x86_64 / subsystem2、fw=PE32+ x86_64 / subsystem3。両方の`.rsrc`、icon/group-icon/version/manifest、asInvoker、0.31.0を確認する。MSYS/MinGW/libgcc/libstdc++/libwinpthread runtime依存がなく、fwにGDI32/OpenGL32/imm32/psapi/dwmapi/uxtheme importがないことを確認する。Shell32/User32は許容する。macOS側の構造検査結果は別証跡であり、Windows側未実行ならNOT RUNと記す。

## 選択したnative実画面

[DETERMINISTIC-CI.json](DETERMINISTIC-CI.json)には今回のWindows preflightで成功した関連テストの原結果を記録した。これは実画面・OS通知・TC-193の測定結果ではない。

新しいtask所有profile/app/fixtureを用い、起動前にsource/run/hash、各絶対パス、既存プロセス一覧、OS外観/ウィンドウ寸法、操作範囲を記録する。子プロセスのみ設定を隔離し自己更新を無効化する。通常の640x400と760x560程度のclient領域を使い、実寸も保存する。画面キャプチャは個人情報を避け、Git管理外に置く。

初回起動にrootを渡す場合は所有fixtureだけにする。tab/query復元を確認する再起動には`--root`やstartup queryを渡さない。明示した起動値が復元値に優先する仕様だからである。各再起動前に同じ候補PIDの通常終了を確認し、同じ隔離profileを再利用する。

| 対象 | 操作と期待結果 |
| --- | --- |
| GSM-013 Settings / footer | 通常表示と短いwindowでbodyをscroll。Save→Cancelが右下にあり、JSON操作と分離され、値/エラー/next-launch通知が読める。dirty draftでReload確認とCancel、無効な数値、Save、Cancel、Reset相当の既存操作を確認し、永続元が誤って変わらないことを記録する。Open JSONは可視/配置まで。外部editor起動は別の明示承認なしに実行しない。 |
| GSM-007/013 query restore | 二つの所有rootにalpha/betaファイルを用意し、各tabへ異なるquery/filter、active tabを設定。Restore previous tabs OFFで子Restore search queriesがdisabledになり値を保持する。tabs ON/query OFFで保存→通常終了→同profile再起動: roots/filters/activeが復元しqueryは空。query ONにして異なるqueryを保存→終了→再起動: 両queryが復元する。Cancelも保存しないことを確認する。 |
| GSM-009 saved themes | System/Dark/Lightを各々保存し通常終了・再起動。mainとSettingsの両方、selected/focused/disabled controlsが読め、固定themeは保存値になる。 |
| GSM-002/003/009 Light colors | alpha一致の通常/選択/current rowをpointerとDownで操作。FILE/DIR/LINK/OTHERのうち安全に構成可能なkind label、一致文字、row fill、隣接controlsをLight/Darkで比較する。安全なlink fixtureを作れない場合は権限を変えず当該subcaseをNOT RUNとして返す。 |
| GSM-008 / FileList confirmation footer | preset編集の作成/保存/削除確認Cancel、FileList確認Cancelのfooter配置を確認する。rootは所有fixtureのみ。外部動作や不要なFileList書き込みを実行しない。 |
| GSM-009 actual OS theme notification | セッションに承認がある場合のみ、元OS外観を記録して切り替える。Systemが実通知で追随し、固定Dark/Lightは変わらないことを候補実画面で観測する。最後に元のOS設定へ戻す。初期色だけでこの項目をPASSにしない。 |

入力/scroll/再起動は同じtask所有window/processを一意に指定する。終了は通常Closeを使い、そのPIDの終了を確認する。異常時に無関係なプロセスをまとめてkillしない。profileやfixtureの削除は所有root内の絶対path確認とアプリ終了確認の後に限る。

## 返却物

環境、binding照合、各subflowのPASS/FAIL/NOT RUN、正確な操作/時刻/画面証跡locator、raw timings/fixture inventory/median比、PE結果、通常終了とcleanupをsanitized Markdown/JSONで返す。製品FAILは最初の観測を保ち、click dispatch/request/workerの観測有無とunknownを区別する。headless結果でnative FAILを上書きしない。

未確認のWindows native、TC-193、macOS実OS外観通知、独立reviewはそのまま残す。別versionの例外を継承しない。タグ・公開操作、他chatへの送信はこのパケットからは許可しない。
