# アーキテクチャ残課題の補強 2026-09-21

## 対象と判断

基点は `d6f9eed780eb708ab3e1fac635abbe16107ec010`。保存時のデータ保全、active filter のフレーム負荷、保存要求の蓄積、保存失敗の表示を対象とする。保存形式、依存関係、CI方針、FileList検出規則は変更しない。

実装コミットはfilterの `62bd4fd4afad666b2f6c7846dbade347539cb8aa` と保存の `b7fa04e22c48b24544b76a6964bce9f222ff0443`。ローカルbranchは `codex/architecture-residual-hardening`。

| 問題 | 対応 | 契約・検証 |
| --- | --- | --- |
| 構文上有効でも型不正な設定をdefault fallbackから上書きできる | 原文・merge後の型検証。起動時読込失敗はprocess内で保持し、修復後も再起動までautosave/settingsを停止 | SP-016 → DES-017 → TC-168 |
| active filterの全件scan/clone/dropがUI frameへ集中する | tab所有continuationで1poll最大512候補。kind待ち最大4096、旧表示維持、exact identity確認、既存reclaimerへの所有権移譲 | SP-010 → DES-007/DES-009 → TC-151 |
| 保存失敗中にqueueとpendingが増え続ける | queuedとretryingを合わせたautosave最大64件、command channel72件、非blocking拒否、受理後だけhistory baseline更新 | SP-016 → DES-017 → TC-168 |
| enqueue後の保存失敗がGUIから見えない | accepted/persisted世代と単一statusをpoll。失敗をfooterへ優先表示し、復旧を表示。未受理の変更はdirtyに残して再送 | SP-016 → DES-017 → TC-168 |

候補準備中は索引の取込みを待機し、固定された候補集合を保護する。bounded mailboxへbackpressureをかけ、種類判定・保存など他workerのpollとUI入力は継続する。検索語や並び順が途中で変わった場合、完了時の最新条件を用いる。旧条件の途中結果は公開しない。

保存に失敗してからの復旧方法は [SUPPORT](../SUPPORT.md) が所有する。正常に読み込めていた起動の一時的保存失敗は再試行し、起動時fallback由来の状態とは区別する。型検証は未知JSON fieldを排除しない。

## 検証

環境はWindows GNU / Rust 1.97.1。Rust commandは `--locked --offline --target x86_64-pc-windows-gnu` を使用する。`scripts/validate_change.py --base d6f9eed780eb708ab3e1fac635abbe16107ec010 --plan` の選択はVM-001/002/003/007/008。

- 修正前に、型不正設定の書換え、起動時fallbackから修復済み設定の上書き、10,000件の無制限受付、GUI保存失敗の非表示、大量候補の即時snapshot置換を再現した。
- `cargo clippy --all-targets -- -D warnings`: PASS。
- `scripts/gui-smoke-fixture.sh`: canonical hashes / FileList counts PASS。
- `scripts/gui-deterministic-scenarios.ps1`: 14群・305実行PASS。GSM-001/002/003/005/007/008/010/011/013の影響範囲を決定的テストで確認し、native操作とは区別した。
- VM-003のignored性能ガード2件: PASS。30,000行FileListの既存metadata probe対比8.03倍、33,025件Walker分類の既存eager metadata対比37.76倍。比較対象は既存のcontrolであり、今回の変更による速度向上を示す値ではない。
- releaseのTC-154タブ遷移ガード: 100,000件・50回のp95は0.002ms、既存上限50ms未満でPASS。測定対象はcoordinatorの所有権移動であり、実画面の体感遅延とは異なる。
- `cargo test`: 1478件PASS（library1419、fw8、architecture2、CLI47、path2）、15件ignored。macOS専用harnessは対象OS外のためSKIPPED。
- focused TC-150/151/152/153/167/168と`run_ui_frame`: それぞれ3/15/10/10/21/18/1件PASS。Full中のquery/sort変更、tab復帰、worker上のscratch解放、GUI dirty再送と保存失敗の実描画を含む。
- 保存先取得失敗でdirtyとエラー表示を保持するTC-168をさらに1件追加し、最終全体テストとclippyで再確認した。性能・GUI検証時からproduction sourceは同一である。
- 既存large incremental testは、同一frame完了の前提を旧snapshot維持と分割後の正確な2048件へ変更した。tab復帰テストは手動seedした完成snapshotへSuccess lifecycleを付与し、意図しないDormant再読込みを除いた。
- SDDの既存ID・参照、変更文書のdiff、TC table構造、SUPPORTのredaction文言と禁止override名の非混入を確認した。文書更新には動作TDDを適用せず、これらの整合確認を用いた。

## レビューと残る確認範囲

実装から独立したread-onlyレビューでは、処理待ち中の検索語変更による旧empty-query公開とTC tableの崩れを指摘。前者は現query/sort scopeでcontinuationを再検証し、後者は既存TCのセル内へ修正した。修正後のfocused再確認で、26個のRust sourceのSHA一致、全検証証跡、未検証範囲を照合し、最終checkpointを通過した。未解決のblocking/major/minor指摘はない。reviewerの以前の関与は実装前の計画レビューのみで、実装・テスト実行・Git変更には関与していない。

Native GUI操作、IME/DPI/UNC、WSL/Linux、macOSはNOT RUN。eguiの描画・入力の決定的テストをnative表示や体感応答性のPASSとは扱わない。ユーザーデータの修復、OS既定アプリ起動、network update、push/PR/tag/releaseは行っていない。

受理前の最新変更と未flush履歴はprocess異常終了で失われ得る。物理的な電源断、filesystemのdurability保証、sidecar lockを使わないexternal writerとの競合は既存の残余リスクである。rollbackは保存形式の移行を伴わず、保存関連とfilter関連のローカル変更を単位として戻せる。
