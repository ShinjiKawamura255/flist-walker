# 運用復旧性・構造レビュー 2026-10-01

## 対象と基準

- 基準: `bede48e65e02c07cf0f59c95db89b0f71c4efda2`。直近の snapshot freshness と preview keyboard controls を含む Rust GUI/CLI/TUI を診断した。
- 環境: macOS arm64、Rust 1.98.1。clean `master == origin/master` から `new-change` preflight を通し、`codex/resilience-owner-boundaries` で対応した。
- 重要状態: committed index/preview、request/tab ownership、UI-state/history/saved-roots、FileList、update marker/backup。
- 外部境界: filesystem/OS lock、worker channel/process、OS actions、update HTTP/trust/activation、release distribution。
- RTO/RPO の製品全体数値は未定義。既存 timeout/容量/保持契約だけを評価した。
- 代表的な障害のレビューであり、全関数・全障害組合せの監査ではない。利用者データ、実インストール、外部アプリへ障害注入していない。

## 優先指摘と対応

| ID | Severity | Confidence | Scenario / 影響 / 原因 | Evidence | Disposition / 検証境界 |
| --- | --- | --- | --- | --- | --- |
| OR-01 | Medium | High | worker panic を join が捨て、全員 join 済みなら正常 `shutdown_complete` を出していた。終了診断が処理失敗を隠し、復旧時の原因判断を誤る | [runtime](../../rust/src/app/worker/runtime.rs) の `join_all_with_timeout` / `shutdown_workers_with_timeout`、[shutdown tests](../../rust/src/app/tests/shutdown.rs) | **Fix + Instrument + Test**。`panicked` と `pending` を分離、名前付き warning/stderr、既知 panic で正常終了ログを抑止。正常/timeout/panic/混在を隔離 thread で検証 |
| AR-01 | Low | High | [persistence worker](../../rust/src/persistence/worker.rs) が受付、順序制御、文書検証・マージ、履歴、複数ファイル rollback を混在させ、保存変更の波及境界が不明瞭 | 変更前902行、異なる責務の関数群、TC-167/168 | **Fix**。actor、[document owner](../../rust/src/persistence/worker/document.rs)、[settings transaction owner](../../rust/src/persistence/worker/settings.rs) に分割。既存42テストで意味保存、[構造ガード](../../rust/tests/architecture_boundaries.rs) で再混在を防止 |
| AR-02 | Low | High | recent freshness / preview control の ownership が短い architecture map から追えない | [architecture](../ARCHITECTURE.md)、[preview flow](../../rust/src/app/paged_preview_flow.rs)、[freshness](../../rust/src/app/freshness.rs) | **Document**。model・probe・UI adapter・共通 command seam を現構成へ反映 |

OR-01 の再現テストは修正前、`panicked` の最小データ定義だけを追加した段階で空配列と期待名の不一致で失敗した。修正後は named panic と blocked worker を区別できる。panic 発生後の全 in-flight request の成功や自動復旧を保証する修正ではなく、障害を診断から隠さない修正である。

## Failure / recovery matrix

`Proven` は今回の実行で assertion した隔離条件に限定する。native 運用や電源断まで意味を拡張しない。

| Scenario | Expected behavior | Detection | Containment | Recovery | Evidence | Gap |
| --- | --- | --- | --- | --- | --- | --- |
| worker panic / join timeout | 既知panicを正常終了扱いせず、UI終了を無期限に待たせない | worker名付き `shutdown_panicked` / `shutdown_timeout` と stderr | panic結果と未終了名を分離、既存join予算 | 再起動。副作用を自動再実行しない | shutdown の新2テスト、既存normal/timeout、TC-150/151/153 | Partially proven。隔離threadの結果分類はProven、native shutdown/全worker故障組合せはUnknown |
| stale / canceled search・index・preview | last-goodと現在tabを旧応答で巻き戻さない | request/epoch、error/取消status | generation/tab/path一致、固定worker・queue・reclaimer | 最新要求の実行、明示refresh | [search failure](../../rust/src/app/tests/search_failure.rs)、TC-151/153、freshness/preview suite | 隔離条件はProven。実UNCでのOS呼出し停止時間はUnknown |
| root FileList削除/置換/アクセス不可・probe遅延 | snapshotの取得時刻を維持、変更/確認不能を通知 | fingerprint比較、5秒間隔、2秒論理timeout、非modal通知 | tab/root/generation/path一致、1物理probeをtimeout後も保持 | 利用者のrefresh。suspended snapshotのprobe再起動なし | [freshness tests](../../rust/src/app/tests/freshness.rs)、[refresh tests](../../rust/src/app/tests/freshness_refresh.rs)、[worker tests](../../rust/src/app/freshness/worker.rs) | Probe停止と遅延応答隔離はProven。OS I/O自体を強制cancelできず、永続停止中は他snapshotのcheckも待つ |
| preview読込途中の変更・文字コード不正 | 既存表示を壊さずreload/失敗を明示 | PreviewPageError、request identity | bounded read、identity照合、失敗page非commit | reload / 別file選択 | [paged preview](../../rust/src/ui_model/paged_preview.rs)、[controls tests](../../rust/src/app/tests/paged_preview.rs) | fixtureはProven。native key delivery/IME/scroll体感はUnknown |
| 不正UI-state / startup読込失敗 | 元bytesと関連rootsを保全、default fallbackを保存権限にしない | persistence status / commit / flush error | typed validation、startup protection、write前拒否 | [保存復旧手順](../SUPPORT.md#ui-state-persistence-recovery) に従い保全・修復・再起動。runtime failureは修復後retry | TC-167/168、[document](../../rust/src/persistence/worker/document.rs) | 隔離保全・修復後retryはProven。実ユーザrestore訓練はDocumented only |
| lock contention / 継続保存失敗 / queue Full | frame待機なし、accepted generation保持、受付上限64 | status/flush error、full rejection | bounded admission・barrier、worker内retry | lock解放・保存先修復後retry | persistence42件、sustained failure / contention / frame latency fixture | fixtureはProven。実disk-full/長期media故障はUnknown、process crash前の未flush変更は保証外 |
| settings第2write / replace後sync / rollback失敗 | 成功通知せず、復元を試み各errorを保持 | request結果・元errorとrollback error | 文書検証が副作用より先、個別atomic write、runtime rollback | 結果と実bytesを照合し明示再操作 | TC-167 rollback/post-replace/両restore失敗、[settings](../../rust/src/persistence/worker/settings.rs) | fault writerでProven。2ファイル間のprocess crash/power lossにjournalなし、cross-file原子性は保証しない |
| FileList生成・祖先更新の途中失敗/panic | 不完全成功を隠さず先行変更の復元を試す | structured report/exit code | plan検証、atomic replacement、rollback | 結果確認後retryまたは手動修復 | TC-165、[indexer tests](../../rust/src/indexer/tests/mod.rs) | inert fixtureはProven。実媒体障害はUnknown |
| actionの再認可失敗/途中エラー | 残件停止、既開始数を通知、root外を拒否 | partial/blocked report | 引数配列、直前path再認可、request revocation | 利用者が副作用確認後に再操作 | TC-164、[action commands](../../rust/src/app/tests/action_commands.rs)、[actions](../../rust/src/actions.rs) | recording backendはProven。外部Open/Revealと副作用rollbackは未実施/保証外 |
| update timeout/巨大body/署名不正/partial download | 信頼検証前に適用せず、部分stageを限定cleanup | 明示error、byte/time限界 | trust-first、bounded streaming/redirect、owned temp | retry | TC-157、[staging](../../rust/src/updater/staging.rs) | scripted body/fault readerはProven。実配布endpoint障害はUnknown |
| update activation/restart失敗・曖昧marker/hash | 旧bundle復元または証跡保持、曖昧状態を改変しない | recovery outcome/診断 | binary-last、marker/hash/path再検証、backup保持 | [updater recovery](../UPDATER_RECOVERY.md) / [release incident runbook](../RELEASE_INCIDENT_RUNBOOK.md) | TC-159、[transaction tests](../../rust/src/updater/transaction/tests.rs) | inert dummy bundleはProven。実配布物のWindows replacement/再起動/restoreはDocumented only・今回NOT RUN |

## モノリシック構造の判定

**製品全体をモノリス化したという証拠はなく、大規模再構成は不要。保存ownerの局所分割は必要と判断して実施した。**

- [app/mod.rs](../../rust/src/app/mod.rs) はbootstrap/frame/exitのcoordinatorで、recent機能の処理本体はfreshness/workerとpaged_preview_flowにある。新機能を起点にtop-levelへI/Oや判定が集約されてはいない。
- index/search/query、actions、persistence、updater は別owner。既存architecture testsは保存層のGUI依存とresult policyのapp/worker依存を禁止する。
- `pipeline.rs`、`tabs.rs`、`state.rs` は依然大きいが、index publication、tab lifecycle、state定義という役割があり、今回の調査で行数だけを理由に分割する利益は示せなかった。全関数の依存解析を完了したとの主張ではない。
- 保存actorは902→592行、document208行、settings133行に分割した。公開API・永続schema・lock順序・retry・capacityを維持する。新しいgeneric storage frameworkは導入しない。
- 構造テストはprivate document/settings ownerのscheduling/thread/actor state依存を禁止し、actorへの文書マージ/rollback本体再混入も検出する。機械的抽出には新しい外部振る舞いがないため、新規失敗テストの代わりに既存characterizationの変更前後比較を用いた。

## 検証・review coverage

| Check | Result / 範囲 |
| --- | --- |
| `validate_change.py --base origin/master --plan` | VM-001/002/008。各intent checklist/detailを確認 |
| `validate_change.py --base origin/master --full` (host) | PASS: repository contract、format、`cargo test --locked`、clippy全target warning gate |
| Rust suite | PASS: lib1439、fw8、architecture3、CLI47、path-key2 = 1499件。macOS quit menu harnessもPASS。child writer1件は重複加算しない。ignored14件はNOT RUN |
| `cargo test --locked persistence:: --lib` 変更前/後 | PASS: 42/42。未知field、複数writer、contention、nonblocking/frame latency、queue上限、startup保護、rollbackを維持 |
| `cargo test --locked worker_runtime --lib` | PASS: 4件（正常、timeout、panic、panic+blocked） |
| TC-150/151/153 focused | PASS: 3/15/10件 |
| freshness / paged_preview focused | PASS: 25/39件。fingerprint分離、timeout物理slot、inactive/stale、pointer/key command共通条件を含む |
| TC-157/159/164/165/168 focused | PASS: 12/25/14/27/19件。信頼境界、dummy bundle recovery、再認可、FileList部分失敗、保存失敗を含む |
| `scripts/gui-smoke-fixture.sh` | PASS: canonical hashes/counts。fixture生成はnative interaction/liveness証拠ではない |
| `git diff --check` / docsリンク・owner照合 | PASS |

sandbox全体testの初回はUnix socket bindの`Operation not permitted`で既存2件が失敗した。テストを変更せずhost側で同じfull検証を実行し、両方を含むsuiteが成功した。環境制約とproduct失敗を分離した。

Native GUI描画・操作は今回変更しておらず、保存構造/終了診断に対して既存settings/session/workerの自動テストを適用した。GSM-013相当の保存条件はdeterministic testで確認、native interaction/livenessはNOT RUN。生成したlocal reportは補助資料だけで、ここに保持したsanitized結果が本件の証跡となる。

独立read-only final review: **指摘なし**（blocking/major/minor 0件）。実装への事前関与なし。基準commitからのworktree差分と未追跡owner/reportを確認し、抽出14関数のsignature/body同一性、shutdown分類、TC契約、host検証ログ、記録のProven範囲を照合した。

終了warning/正常eventのsubscriberによる直接captureは未実施。結果分類は実行テスト、ログ出力条件は独立した静的確認による。

## Recommended handoff / residual risk

- 採用したOR-01、AR-01、AR-02はローカル対応対象。実装・テスト・設計mapをそろえる。
- 本件ではproduct全体RTO/RPO、保存のcross-file crash原子性、外部副作用のrollbackを新たに定義しない。要件化する場合は仕様判断と別の検証境界が必要。
- 非cancelable OS I/Oの物理slot保持はboundednessを優先する既存契約としてAccept。無期限I/O下では監視の自動復旧を保証しない。
- Linux/Windows、native focus/IME/DPI/複数画面、実UNC、実disk-full/電源断、実updater restart、remote CI/releaseはNOT RUN。必要なら承認された隔離native session/VMでExerciseする。既存runbookを実証済みrestoreと混同しない。
- push、PR、auto-merge、release、repository設定変更、外部アプリ起動は本件の実施対象に含めていない。
