# v0.31.0 Windows確認準備 — run39810727499

**準備のみ。候補生成は監査失敗で停止。Windows実画面・TC-193はNOT RUN。新しい実行用ZIPは未作成。** [候補記録](../REPORT.md)のsource `d1bf330fd599cc04a333fc903c64dec77fd50aee` / attempt1には、取得可能な配布artifactがない。旧run39087986481のZIP・binding・helperをこのrunへ流用しない。

## 配布物を受け取る前の停止条件

セキュリティ修正後の別候補が全必須CIを成功させ、28asset/26checksum、全archive/sidecar、実署名、直前公開版からのN-1、Windows PE、実compiler、警告分類の検証を終えるまで実行用handoffを発行しない。その候補のsource/run/attempt/artifact ID・expiry・ZIP digest、manifest/signatureと両Windows EXEのhashを新しいbindingへ固定する。旧結果は上書きしない。

現在の[timeout修正版helper](../../windows/verify-windows-candidate.py)は旧source/runを厳格に限定する。このまま新候補では実行できない。将来の新パケットへコピーして正確なidentityだけを更新し、当該コピーで既存mock testsを再実行する。新候補の実物とbindingがない段階で限定を緩めたりplaceholderの実行用bindingを作らない。

新ZIPにはWindows subset8ファイル（universal/fw EXE、universal ZIP、README/LICENSE/THIRD_PARTY_NOTICES、全体manifest/signature）、その候補専用helper/binding、本手順・返却様式を収める。CRC、全member名と全内容hashを照合し、ZIP外の記録へZIP digestを保存する。全28assetを含むという表現はしない。

## セッション準備

- Windows x86_64の通常負荷のあるロック解除済みdesktop、PowerShell、既存の標準Python3と検証ツールを用意する。追加インストール・保護機能無効化は行わない。
- GUI起動・OS外観変更は、将来の正確な候補・隔離sessionに対する明示承認を得てから行う。本依頼は確認準備であり、起動承認ではない。
- 新しい所有ディレクトリだけを使い、既存プロセス/設定/インストール済みアプリを確認して触らない。childだけに隔離HOME/USERPROFILE/LOCALAPPDATA/APPDATAと自己更新無効化を設定する。shell経由の起動、Open/Reveal/Exec/clipboard/実更新は対象外。
- 正確なsource/run/hash、profile/fixture/app絶対パス、Windows build/CPU/font/display scale、client領域、開始終了時刻を記録する。通常760x560と小さい640x400程度を使い、実寸も残す。既存の100/125/150/200% scaleが利用可能なら選択したscaleで確認し、未実施scaleはNOT RUN。物理monitor移動や全DPI認証を意味しない。

## 選択した実画面確認

| 対象 | 操作と期待結果 |
| --- | --- |
| GSM-001 / TC-227 Source | 所有rootの初回取得中、確定前Detecting… / Indexing…、Started後FileList名またはWalker、finalizationまで処理中、完了後に取得情報を表示。FileList/Walkerは別fixtureを使う。短時間で完了して観測できなければNOT RUNとし、待機を増やすため製品を変更しない。 |
| GSM-001/007 Sourceの隣接状態 | query/filter/tab切替と再取得でsource/statusが正しいtabに留まり、古い応答で戻らない。再取得中は前回成功snapshotとRefreshingを保持する。入力とscrollの応答を観測し、計測していない遅延値を記さない。 |
| GSM-007 tab末尾 | 所有rootのtabを複数作成し末尾へscroll。追加ボタンの右端がclipせず、query/隣接操作を隠さない。用意できるscaleと狭い/通常幅で確認する。 |
| GSM-001/007/008 Rootと共有行 / TC-010 | 日本語・ASCII・長いpathをRootで表示し上下glyph切れ/横の操作との重なりがない。saved-root/preset/Named Rootの代表面で選択行の文字、安定した高さ、親scroll clip、pointer/keyboard選択を確認する。長いリストはtop/middle/endでscrollし、offscreen行が重なったり戻り位置が崩れない。 |
| Root改行境界 | WindowsではCR/LF等を含む有効directory名を作れないため、POSIX改行pathの実root再現はNOT REQUIRED（Windows非対応fixture）。通常pathが変化しないことを確認する。macOS側の選択項目では実改行rootがliteral escapeで一行表示され、query/Resultsが見え、実pathを維持することを別途確認する。 |
| GSM-013 Settings/footer | 短いwindowでbodyをscrollしSave→Cancelが右下、JSON/Resetと分離。dirty draft/Reload確認Cancel、無効値、Save/Cancel、通知の可読性を確認。Open JSONは表示まで。 |
| GSM-007/013 query restore | 二つの所有rootと別query/filter/active tab。tabs OFFで子query設定disabled/値保持、tabs ON/query OFF保存→通常終了→同profile再起動でquery空、query ON保存→再起動で両query復元。再起動時にroot/query引数は渡さない。Cancelで保存しない。 |
| GSM-002/003/009 Light/Dark | 一致文字、FILE/DIRと安全に作れるLINK/OTHER、通常/選択/current rowをpointer/Downで確認。mainとSettingsで固定theme保存/再起動、disabled/focused controlsも読む。権限不足のlinkはNOT RUNとし権限変更しない。 |
| GSM-008 confirmation footer | preset編集の作成/保存/削除とFileList確認のCancel配置。所有fixtureだけを使い、不要なFileList書き込みや外部動作は行わない。 |
| GSM-009 OS通知 | exact session承認がある場合のみ元OS外観を記録して切り替え、System追随と固定Dark/Light維持を観測し元へ戻す。初期色だけでPASSにしない。 |

新しい共有描画により、旧macOS静止画PASSだけでWindowsのnative rasterizationやlist操作を認定できない。PR194/195のGUI操作timeoutはNOT RUNであり製品FAILではない。最初の製品FAILは原結果とclick dispatch/request/worker観測有無を保ち、推定原因はunknownとする。

## TC-193

新候補専用helperが確定してから、GUIとは別に実施する。固定shallow200-file（file_000.txt〜file_199.txt、内容`owned TC-193 fixture`と改行）、各variant5warmup+25測定、交互順序、universalだけ`--cli`、同一fixture/environment、direct argv child、stdout/stderr redirect、同期終了までのmonotonic timerを維持する。`fw median / universal median <= 0.70`、全60回exit0/stderr空、200行の内容/順序/framing一致が条件。alias/versionを含むraw総数とtimed60回を区別する。

測定直前/直後のCPU・空きmemoryと環境を残し、他build/install/testを並走しない。timeout30秒の部分stdout/stderr、role/argv、経過時間、timeout秒数、null exit、先行観測を保持する。raw書込は測定区間外。最初の結果/rawの上書き、合格まで反復、sample/warmup/閾値変更は禁止。mock5件PASSはnative性能証明ではない。

終了は対象PIDの通常Close/終了確認を使う。所有profile/fixtureの削除は絶対path・所有範囲・終了を確認してから行い、他アプリをまとめてkillしない。[返却様式](RETURN.template.md)へidentity/各subcase/原観測/raw/cleanupを記録する。タグ・公開は行わない。
