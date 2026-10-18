# v0.31.0 Windows返却結果

状態: NOT RUN（実施後に原観測を追記する）。タグ・公開判断は含めない。

- Source: 3194f03c27f9a72f06a836dbd02e9141d8740a26
- Run: 39087986481 / attempt1 / workflow_dispatch / master
- Artifact ID/digest/expiry、manifest/signature/both exe SHA256: bindingから照合した実値を記す。
- Windows edition/build/arch、CPU、Python/PowerShell、開始/終了時刻、CPU/空きメモリ前後: 未記入。
- GUI session承認範囲、隔離app/profile/fixture、元OS外観と実client寸法、対象PID: 未記入。

| Axis / subflow | Result | Observed behavior, time and evidence locator |
| --- | --- | --- |
| Identity/hash/version | NOT RUN | |
| Windows PE/resource/imports | NOT RUN | |
| TC-193 5warmup+25samples each | NOT RUN | Raw JSONL、25値/variant、median、fw/universal<=0.70、exit/stdout/stderr、fixture inventoryを添付 |
| Deterministic tests | NOT RUN / eligible exact CI REUSE only | 元run/job/source/結果と再利用根拠を記す。native PASSへ転用しない |
| Settings short-window/footer/dirty reload/error/Cancel | NOT RUN | |
| Parent/child disabled state, query restore OFF/ON and restart | NOT RUN | |
| Saved System/Dark/Light and both main/Settings render | NOT RUN | |
| Light/Dark matches/kind/normal-selected-current rows | NOT RUN | |
| Preset/FileList confirmation footer and Cancel | NOT RUN | |
| Actual OS theme notification; System vs fixed modes | NOT RUN | 元OS設定の復元結果も記す |
| Liveness / typing / scroll | NOT RUN | 観測したselected flowだけを記す |
| Normal close / exact PID exit / owned cleanup | NOT RUN | |

FAIL/timeout/NOT RUNには最初の操作、時刻、実表示、fixture、tool error、request/workerの観測有無とunknownを残す。再実行やoracle訂正は原記録と区別し、理由/承認を記す。過去versionのdeviationを転記しない。公開repoへ個人情報、秘密値、画面キャプチャそのものを入れない。
