# CI Rollout History

Completed rollout records and point-in-time CI measurements. These records preserve the evidence and procedure used at the time; they are not current instructions or authorization. Use [CI Operations](../CI_OPERATIONS.md) for current policy and [Maintenance History](INDEX.md) for other records.

## Timing baseline and hosted acceptance (2026-08-25)

- PR runs `32772324004`, `32774669250`, `32788084487` は約18-19分で、GNU cross build自体は約3-4分だったが、E2Eは約11分のWindows native job完了後に開始して約7-8分を追加していた。
- Rust-impacting proof [PR #76](https://github.com/ShinjiKawamura255/flist-walker/pull/76) のrun `32865822819`は、Windows native 11分02秒と、GNU build 3分32秒から直ちに開始したE2E 7分37秒を並列化し、開始から`CI Gate`成功まで11分27秒だった。全heavy jobを維持したまま旧baselineから約7分短縮し、`max(native matrix, GNU build + E2E) + 2分`以内を満たした。
- controlled rollout後のdocs-only proofは本記録を追加するPRとし、queue待ちを除き3分未満で`CI Gate`まで完了し、native matrix、GNU producer/E2E、clippy/coverageの4 heavy jobがすべて`skipped`であることを確認する。最終結果はPR recordを正本とする。
- trusted policy変更は下記recordのcontrolled rolloutで有効化した。今後の構造変更も、設定snapshot、独立final review、exact-head `CI Gate`、競合なし、一時的required-check変更、rebase merge、即時復元/read-back、通常保護経路のproof PRを省略しない。

## First hosted proof and scale baseline (2026-08-20)

- Default branch `f1800aa9` の manual dispatch [run 32340613009](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/32340613009) は 23分42秒で成功した。deterministic 1,000 seeds x 1,000 steps、real-worker 1,200秒 soak、artifact upload、post処理がすべて成功した。
- Artifact `stateful-endurance-32340613009` は 5,873 bytes、14日保持（expiry `2026-09-03T07:04:41Z`）として API read-back 済みである。初回 hosted proof は workflow が required check ではないという運用を変更しない。
- TC-185 は既存 heavy weekly perf command の一部として、exactly 1,000,000 candidates、selective/dense の2 shape x 7 samples、nearest-rank p50/p95/p99、4 RSS phaseを stable `tc_185` labelで出力する。RSSは allocator/host差を観測する baselineであり、十分なhosted履歴が蓄積するまで閾値違反として扱わない。

## Repository policy rollout record (2026-07-28)

変更前は repository auto-merge が `false`、`master` branch protection は未設定だった。`420520c` のmaster pushで `CI Gate` が成功した後、次の設定を適用してread backした。

| Setting | Active value |
| --- | --- |
| Repository auto-merge | enabled |
| Branch protection identifier | `repos/ShinjiKawamura255/flist-walker/branches/master/protection` |
| Pull request required | yes |
| Required approving reviews | `0` |
| Required status checks | `CI Gate`, `CI Policy Guardian` |
| Required check source | GitHub Actions app ID `15368` |
| Require up-to-date branch | yes (`strict: true`) |
| Apply to administrators | yes |
| Force push / branch deletion | disabled / disabled |

変更前状態へ戻す必要がある場合は、先に影響中のPRを確認し、repository admin権限でbranch protection endpointを`DELETE`してからrepositoryの`allow_auto_merge`を`false`へ戻す。通常のCI不具合では保護を外さず、workflowをrevert PRで復旧する。

Protected routeの検証記録は[PR #10](https://github.com/ShinjiKawamura255/flist-walker/pull/10)とする。branch protection適用後に通常権限でPRを作成し、2026-07-28にmerge method `MERGE`のper-PR auto-merge登録が受理された。approvalやadmin bypassは使用せず、strict branch更新と`CI Gate`の最終結果・merge outcomeはGitHubのPR recordを正本とする。

`CI Policy Guardian`は[PR #11](https://github.com/ShinjiKawamura255/flist-walker/pull/11)で`CI Gate`通過後にmergeし、同じGitHub Actions app ID `15368`の第2 required checkとして追加した。追加後のprotection read-backはstrict `true`、approval `0`、administrators適用、force-push/deletion禁止を維持している。

Guardian有効化後のprotected route証跡は[PR #12](https://github.com/ShinjiKawamura255/flist-walker/pull/12)とし、`CI Policy Guardian`と`CI Gate`の両方をrequiredにした状態でper-PR auto-mergeを登録する。最終checkとmerge outcomeはPR recordを正本とする。

run `30289068993`では、同一treeのPR runが成功した後にmacOSだけ`capped_walker_finished_drains_large_backlog_without_long_tail_regression`が失敗した。原因はproductionの4ms frame budgetと固定8回pollを同時に使ったtestがhost速度を暗黙前提にしたことだった。production budgetは維持し、testだけ明示budgetを注入してentry capの契約を決定的に検証する。

## Guardian controlled rollout record (2026-07-28)

[PR #13](https://github.com/ShinjiKawamura255/flist-walker/pull/13)でaudit exceptionとCI policy testをimmutable trusted setへ追加した。現行guardianはworkflow/checkerの構造変更を期待どおりfail-closedにし、run `30293703506`は失敗した。独立agent reviewで指摘0件、head `d99a550`の`CI Gate` run `30293703807`成功を確認してから、required checksを一時的に`CI Gate`だけへ変更した。auto-mergeで`fbce654`へmerge後、直ちに`CI Policy Guardian`を復元した。

復元後のread-backはrequired checksが`CI Gate` / `CI Policy Guardian`（ともにapp ID `15368`）、strict `true`、approval `0`、administrators適用、force-push/deletion禁止、repository auto-merge有効である。新guardianのprotected-route証跡は[PR #14](https://github.com/ShinjiKawamura255/flist-walker/pull/14)とし、最終checkとmerge outcomeはPR recordを正本とする。

## Rebase-only rollout record (2026-07-28)

変更前snapshotではmerge commit / squash / rebaseが有効、merge済みbranch自動削除とlinear historyが無効だった。[PR #17](https://github.com/ShinjiKawamura255/flist-walker/pull/17)のexact head `33e213a`を独立reviewして`CI Gate` run `30363440720`の成功とGuardianの期待どおりのfail-closedを確認後、repositoryをrebase-only、merge済みbranch自動削除有効、`master`をlinear history必須へ変更した。required checkを一時的に`CI Gate`だけへ限定してreview済みheadをrebase mergeし、直ちに`CI Gate` / `CI Policy Guardian`（app ID `15368`、strict `true`）を復元した。

完全なbefore/after read-backで、repository設定差分はmerge commit無効、squash無効、merge済みbranch自動削除有効（およびmergeに伴う`pushed_at` / `size`）だけ、protection差分はlinear history有効だけだった。approval `0`、administrators適用、master force-push/deletion禁止、signature/conversation/restriction/lock/block/fork設定は不変である。PR #17のremote branchは自動削除され、2つのsource commitは順序とtreeを保った1-parent commitとして`master`へ追加された。

両required checkを通常状態で通すprotected-route証跡は[PR #18](https://github.com/ShinjiKawamura255/flist-walker/pull/18)とする。release note補完と本rollout recordを別commitにし、rebase auto-merge後のcommit数・順序・message・author・patch/tree対応・parent数とbranch自動削除を検証する。最終checkとmerge outcomeはPR recordを正本とする。

## Scheduled monitor recovery rollout record (2026-08-19)

[PR #54](https://github.com/ShinjiKawamura255/flist-walker/pull/54)でscheduled security auditとlatest canaryのmonitor issue recoveryをfail-closedにし、trusted checkerとadversarial testを更新した。旧guardianは構造変更を期待どおり拒否し（run `32246109796`）、独立agent reviewで指摘0件、exact head `0704541f74c68bdf519a81e544a63c249c3aacba`の`CI Gate` run `32246109768`成功、base/head不変、競合PRなしを確認した。

変更前snapshotはprotection endpoint `repos/ShinjiKawamura255/flist-walker/branches/master/protection`、required checks `CI Gate` / `CI Policy Guardian`（ともにGitHub Actions app ID `15368`、strict `true`）、approval `0`、administrators適用、linear history必須、force-push/deletion禁止だった。repositoryはauto-merge有効、rebase-only、merge済みbranch自動削除有効で、PR #54にはrebase auto-mergeが登録済みだった。

required checksを一時的に`CI Gate`だけへ限定し、PR #54を`b2c0c959b7ee98cd14e9f839b19b8a6ae5b95d1d`へauto-mergeした。復元時の最初のCLI requestは不正な配列payloadとして拒否され、protectionは`CI Gate`のみのまま不変だったため、明示JSON payloadで直ちに`CI Policy Guardian`を復元した。完全なread-backでrequired checks、strict、approval、administrators適用、linear history、force-push/deletion、repository auto-merge、rebase-only、branch自動削除、および非対象設定が変更前snapshotと一致することを確認した。

復元後の新guardian protected-route証跡は本記録を追加するPRとし、`CI Policy Guardian`と`CI Gate`の両方をrequiredにした通常状態でrebase auto-mergeする。最終checkとmerge outcomeはGitHubのPR recordを正本とする。

## Stateful endurance Guardian rollout record (2026-08-20)

[PR #58](https://github.com/ShinjiKawamura255/flist-walker/pull/58)でstateful endurance workflowを追加し、workflow/checkerをimmutable trusted policy setへ登録した。旧guardianは構造変更を期待どおり拒否し（runs `32336410588` / `32336458555`）、独立agentの最終reviewで未解決P1/P2が0件、exact head `268e490d84430428c899272699e9d9c3855d749e`の`CI Gate` run `32336410209`成功、base/head不変、競合PRなしを確認した。

変更前snapshotはprotection endpoint `repos/ShinjiKawamura255/flist-walker/branches/master/protection`、required checks `CI Gate` / `CI Policy Guardian`（ともにGitHub Actions app ID `15368`、strict `true`）、approval `0`、administrators適用、linear history必須、force-push/deletion禁止だった。repositoryはauto-merge有効、rebase-only、merge済みbranch自動削除有効で、PR #58にはrebase auto-mergeが登録済みだった。

required checksを一時的に`CI Gate`だけへ限定し、PR #58を`a2b4e3de1debdfd751541d6c22685428d74f3b92`へauto-mergeした。同じ制御処理の`finally`で直ちに`CI Policy Guardian`を復元し、required checksが`CI Gate` / `CI Policy Guardian`（app ID `15368`、strict `true`）であることをread backした。続く完全なread-backでapproval、administrators適用、linear history、force-push/deletion、signature/conversation/restriction/lock/block/fork設定、repository auto-merge、rebase-only、branch自動削除が変更前snapshotと一致し、remote feature branchの自動削除も確認した。

復元後の新guardian protected-route証跡は本記録を追加する[PR #59](https://github.com/ShinjiKawamura255/flist-walker/pull/59)とし、`CI Policy Guardian`と`CI Gate`の両方をrequiredにした通常状態でrebase auto-mergeする。最終checkとmerge outcomeはGitHubのPR recordを正本とする。

## Windows GNU updater E2E Guardian rollout record (2026-08-22)

[PR #66](https://github.com/ShinjiKawamura255/flist-walker/pull/66) で updater lifecycle、配布 asset 検証、Windows GNU updater E2E、immutable trusted checker/test を更新した。exact head `a3f0b36cc068740409327f1d8b5ef103e8247b6a` / base `316a91b35a2dcc01eb1cfd3999cb073b58ed6f65` に対し、旧 Guardian は `.github/workflows/ci-cross-platform.yml`、`scripts/check_ci_policy.py`、`scripts/tests/test_check_ci_policy.py` の構造変更だけを期待どおり拒否した（run `32500802857`）。同じ head の Cross Platform run `32500805033` では `CI Gate` job `96834642077` と Windows GNU Updater E2E job `96832657034` を含む全 job が成功した。PR #66 は唯一の open master PR で auto-merge 未登録、独立 pre-mutation review は有限時間 control と recovery を確認して blocking / major / minor 0 の GO とした。

変更前 snapshot は required checks が `CI Gate` / `CI Policy Guardian`（ともに GitHub Actions app ID `15368`、`strict: true`）、approval `0`、administrators 適用、linear history 必須、force-push/deletion 禁止だった。signature/conversation/lock/block/fork 設定は無効で、repository は auto-merge 有効、rebase-only、merge 済み branch 自動削除有効だった。

review 済み control で PR #66 の rebase auto-merge を1回だけ登録し、required checks を一時的に `CI Gate` のみに限定した。PR は4.5秒後に `d13c71ebca6b6518e8f5b25279f9e3f9e3ad117f` へ merge され、同じ `finally` の最初の restore で `CI Policy Guardian` を復元した。完全な read-back で required checks、strict、approval、administrators 適用、linear history、force-push/deletion、signature/conversation/lock/block/fork 設定、repository auto-merge、rebase-only、branch 自動削除が変更前 snapshot と一致し、remote feature branch の自動削除も確認した。

rebase audit では source / merged が各5 commit で順序・message・author・stable patch ID が一致し、最初の rebased commit の parent は exact base、merge commit は0件、最終 tree は source / master ともに `806e860208399e5d0d32032bf3d93524854bc818` だった。復元後の新 Guardian protected-route 証跡は本記録を追加する[PR #67](https://github.com/ShinjiKawamura255/flist-walker/pull/67)とし、`CI Gate` と `CI Policy Guardian` の両方を required にした状態で rebase auto-mergeする。最終 check と merge outcome はGitHubのPR recordを正本とする。

## Updater and UI hardening Guardian rollout record (2026-08-25)

[PR #73](https://github.com/ShinjiKawamura255/flist-walker/pull/73) で Universal / `fw` updater E2E、N-1 release gate、immutable trusted checker/test、TUI discovery、GUI state を更新した。exact head `466f8bf22641700229adfc5a9417095b4b423ec6` / base `7a98b7b6bea5829b83dfd308940edd8859c3a542` に対し、旧 Guardian は trusted workflow/checker の構造変更を期待どおり拒否した（run `32772323932`）。独立 agent review は未解決 blocking / major 0件で CLOSED、同じ head の Cross Platform run `32772324004` は Windows GNU Updater E2E job `97578601497` と `CI Gate` job `97581024181` を含む全 job が成功した。PR #73 は唯一の open master PR で、rebase auto-merge は1回だけ登録済みだった。

変更前 snapshot は protection endpoint `repos/ShinjiKawamura255/flist-walker/branches/master/protection`、required checks `CI Gate` / `CI Policy Guardian`（ともに GitHub Actions app ID `15368`、`strict: true`）、approval `0`、administrators 適用、linear history 必須、force-push/deletion 禁止だった。repository は auto-merge 有効、rebase-only、merge済み branch 自動削除有効で、merge commit / squash は無効だった。

required checks を一時的に `CI Gate` のみに限定し、PR #73 を `e2c7f6b58870f236b53cfc129d04703b66c7cb12` へ auto-mergeした。merge 確認後に直ちに `CI Policy Guardian` を同じ app ID で復元した。完全な read-back で required checks、strict、approval、administrators 適用、linear history、force-push/deletion、conversation/restriction/lock/fork 設定、repository auto-merge、rebase-only、merge済み branch 自動削除が変更前 snapshot と一致し、remote feature branch の自動削除も確認した。

rebase audit では source / merged は各1 commitで、parent、tree `e4712726d5ffae5cb12530612ecfe54176395532`、message、author、stable patch ID `f1c94a235851cb7ef6ad9bbf9dc16197fc004814` が一致し、merge commit は0件だった。復元後の新 Guardian protected-route 証跡は本記録を追加する[PR #74](https://github.com/ShinjiKawamura255/flist-walker/pull/74)とし、`CI Gate` と `CI Policy Guardian` の両方を required にした通常状態でrebase auto-mergeする。最終checkとmerge outcomeはGitHubのPR recordを正本とする。

## Updater checksum and CI timing Guardian rollout record (2026-08-26)

[PR #76](https://github.com/ShinjiKawamura255/flist-walker/pull/76)でN-1 checksum manifest gate、required immutable regression、fail-closed docs-only classification、Windows GNU updater producer/E2E DAGを更新した。exact head `afd8e48e814a1057d33655b0a3264e945a5f8942` / base `bb1ceabfd422414e241267a40b3981109f5d8587`に対し、旧Guardianはtrusted workflow/checker/testの構造変更を期待どおり拒否した（run `32865822752`）。独立final reviewは修正後blocking / major / minor 0件でGO、同じheadのCross Platform run `32865822819`はWindows GNU Updater E2E job `97861998391`、Windows native job `97860754959`、`CI Gate` job `97864639973`を含む全heavy jobが成功した。PR #76は唯一のopen master PRで、rebase auto-mergeは1回だけ登録済みだった。

変更前snapshotはrequired checks `CI Gate` / `CI Policy Guardian`（ともにGitHub Actions app ID `15368`、strict `true`）、approval `0`、administrators適用、linear history必須、force-push/deletion禁止だった。signature/conversation/restriction/lock/block/fork設定は無効で、repositoryはauto-merge有効、rebase-only、merge済みbranch自動削除有効、merge commit / squash無効だった。

ユーザの明示承認後、単一controlの`try/finally`でrequired checksを一時的に`CI Gate`だけへ限定し、PR #76を`010307b82e3ff7136469bcb066d37ddb28593fad`へauto-mergeした。同じcontrolの`finally`で直ちに`CI Policy Guardian`をapp ID `15368`で復元した。完全なafter read-backでrequired checks、strict、approval、administrators適用、linear history、force-push/deletion、signature/conversation/restriction/lock/block/fork設定、repository auto-merge、rebase-only、merge済みbranch自動削除が変更前snapshotと一致し、remote feature branchの自動削除も確認した。

rebase auditではsource `300eb86a2369997fdf3c55ee1563838b11ac4f06` / `afd8e48e814a1057d33655b0a3264e945a5f8942`とmerged `ca33ddf4b32dd88507ec5f3a9381bc46bea4c1f1` / `010307b82e3ff7136469bcb066d37ddb28593fad`が各2 commitで順序、message、author、tree、stable patch ID `062f3784274113949c1ace7ee8d76d6264d42623` / `677b401634f7d67018e0c9a8c8d592bc7d4e6d79`を保持し、最初のparentはexact base、merge commitは0件だった。復元後の新Guardianとdocs-only skipのprotected-route証跡は本記録を追加するPRとし、両required checksが有効な通常状態でrebase auto-mergeする。最終check、skip結果、merge outcomeはGitHubのPR recordを正本とする。

## Native release warning gate Guardian rollout record (2026-08-26)

[PR #78](https://github.com/ShinjiKawamura255/flist-walker/pull/78)でmacOS / Windows native clippyとtag release native clippyをblocking gateにし、immutable checker/testとSDD traceを更新した。exact head `64e305ac4d9fc922ed82157a374c7445fd7899c3` / base `0e7aa48c5317cd7ef1cb94898a92bb1f099e7cef`に対し、旧Guardianは`.github/workflows/ci-cross-platform.yml`、`.github/workflows/release-tagged.yml`、`scripts/check_ci_policy.py`、`scripts/tests/test_check_ci_policy.py`の4 pathだけを期待どおり拒否した（run `32877407386`）。同じheadのCross Platform run `32877407347`は`CI Gate`、macOS / Windowsの`Run platform clippy`、Windows GNU Updater E2Eを含む全jobが成功した。PR #78は唯一のopen master PRで、登録済みauto-mergeはcontrolled rollout前に解除した。独立reviewはreview済みcontrol hash `53AD665694BAEFC18AB7B3045FFA993C1F327BC78EFC6F9DDC97912D117E66DA`に対して未解決blocking / major 0件となり、required-checks-only操作に残る最小raceをユーザが明示承認した。

変更前snapshotはrequired checks `CI Gate` / `CI Policy Guardian`（ともにGitHub Actions app ID `15368`、`strict: true`）、approval `0`、administrators適用、linear history必須、force-push/deletion禁止だった。signature/conversation/restriction/lock/block/fork設定は無効で、applicable branch rulesは空、repositoryはauto-merge有効、rebase-only、merge済みbranch自動削除有効、merge commit / squash無効だった。

hard timeout、embedded restore payload、`RestoreOnly`、bounded retryを持つ単一controlでrequired checksを一時的に`CI Gate`だけへ限定し、exact headを通常権限のrebase mergeで`9f3b0659eb34cc88e6a40721164ce6a454c12b0f`へmergeした。Guardian-off状態の確認から復元完了までは約8.1秒で、同じcontrolの`finally`のrestore attempt 1で`CI Policy Guardian`を復元した。完全なafter read-backでrequired checks、strict、approval、administrators適用、linear history、force-push/deletion、signature/conversation/restriction/lock/block/fork設定、applicable branch rules、repository auto-merge、rebase-only、merge済みbranch自動削除が変更前snapshotと一致し、remote feature branchの自動削除も確認した。

rebase auditではsource / mergedのparentがともにexact base、treeが`c2e8e536bd4bf1de804cdb71cdf0d545fc030dea`、stable patch IDが`73d6f501be25603be09bef353b6cc172af77d049`で一致し、messageとauthorも保持された。追加commitは1件、merge commitは0件だった。復元後の新Guardian protected-route証跡は、本記録を2 commitで完成させる[PR #79](https://github.com/ShinjiKawamura255/flist-walker/pull/79)とし、`CI Gate`と`CI Policy Guardian`の両方をrequiredにした通常状態でrebase auto-mergeする。第2 commitのexact headに対する最終checkとmerge outcomeはGitHubのPR recordを正本とする。

## Post-release review Guardian rollout record (2026-08-26)

[PR #81](https://github.com/ShinjiKawamura255/flist-walker/pull/81)で、updater N-1 gateの非増加version拒否、closed-tab restoreの複合index/search/sort/preview再発行、immutable golden test、関連SDD/test traceを修正した。exact head `79b0d514129db35e63e94c43c5f7950cf7bf6bc0` / base `777e293be20d71f5036088aaebbef4e614f71685`に対し、旧Guardianは`scripts/tests/test_check_ci_policy.py`のimmutable変更だけを期待どおり拒否した（run `32935107423`）。同じheadのCross Platform run `32935107427`は`CI Gate` job `98077015161`、Windows GNU Updater E2E、Linux / macOS / Windows native、clippy / coverage、Cargo auditを含む全jobが成功した。独立final reviewは製品差分とcontrolの両方で未解決blocking / major 0件のGOとなり、required-checks-only操作に残る最小raceをユーザが明示承認した。

変更前snapshotはrequired checks `CI Gate` / `CI Policy Guardian`（ともにGitHub Actions app ID `15368`、`strict: true`）、approval `0`、administrators適用、linear history必須、force-push/deletion禁止だった。signature/conversation/restriction/lock/block/fork設定は無効で、applicable branch rulesは空、repositoryはauto-merge有効、rebase-only、merge済みbranch自動削除有効、merge commit / squash無効だった。

最初のcontrolはdeadline値のPowerShell型処理でremote mutation前に停止し、直後のread-backでfull required checks、`autoMergeRequest == null`、PR head、master refがすべて不変と確認した。期限あり/期限切れself-testを追加した修正版control hash `47D20B1060E48E6EB50F5EDE8F07DA0855E6D27DC3239EC909CF7D2BC2A1BD91`を独立再reviewし、ユーザの再承認後に1回実行した。controlはwatchdog、hard deadline、mutator identity、abandoned mutex recovery、bounded retry、`RestoreOnly`、auto-merge settlementを備え、rebase auto-mergeを登録してrequired checksを一時的に`CI Gate`だけへ限定した。PR #81は`5be18f335e1a179df3d513c62e69063c9ad2a080`へmergeされ、Gate-only requestからfull-check restore検証までは約17.5秒、同じcontrolのrestore attempt 1で`CI Policy Guardian`を復元した。

完全なafter read-backでrequired checks、strict、approval、administrators適用、linear history、force-push/deletion、signature/conversation/restriction/lock/block/fork設定、applicable branch rules、repository auto-merge、rebase-only、merge済みbranch自動削除が変更前snapshotと一致し、remote feature branchの自動削除も確認した。rebase auditではsource / mergedのparentがともにexact base、tree `a627379c50495d527ab48f00c5c21e54a09d596f`、message、author、stable patch ID `3fbffd11b4e0a3a24af7b11b4459dbcd563dd587`が一致し、追加commitは1件、merge commitは0件だった。復元後の新Guardian protected-route証跡は本記録を追加する[PR #82](https://github.com/ShinjiKawamura255/flist-walker/pull/82)とし、`CI Gate`と`CI Policy Guardian`の両方をrequiredにした通常状態でdocs-only heavy skip、rebase auto-merge、最終merge outcomeを確認する。

## Pre-tag release candidate Guardian rollout record (2026-09-04)

[PR #106](https://github.com/ShinjiKawamura255/flist-walker/pull/106)で、tag作成前のmanual release candidate workflow、immutable policy checker/test、release運用文書を更新した。exact head `d7f4dda0548eadf19ea1c1068868930ff3c98a9a` / base `bee721197231bf602b8c8a21654d963a312b6807` に対し、旧Guardianは構造変更を期待どおりfail-closedとし（run `33832023584`）、同じheadのCross Platform run `33832023649`は`CI Gate`を含む全jobが成功した。独立final reviewは制御スクリプトSHA-256 `d0cfc8c4b04b5e4464b15810077929daa423a4327af91ce6103a5863a71ec372`に対してblocking / major / minor 0件のGOだった。

ユーザの明示承認後、有限時間watchdog、復元payload検証、bounded retry、`RestoreOnly`を備えた制御処理でrequired checksを一時的に`CI Gate`だけへ限定し、PR #106をrebase mergeした。同じ制御処理のrestore attempt 1で直ちに`CI Policy Guardian`を復元し、完全なread-backでrequired checks `CI Gate` / `CI Policy Guardian`（ともにGitHub Actions app ID `15368`、`strict: true`）、approval `0`、administrators適用、linear history必須、force-push/deletion禁止、repository auto-merge有効、rebase-only、merge済みbranch自動削除有効、および非対象設定が変更前snapshotと一致することを確認した。watchdogは復元markerを確認して正常終了し、remote feature branchも自動削除された。

rebase auditではsource `2f2746d` / `d7f4dda`とmerged `9a8e518` / `459783d`の2 commitが順序、message、author、stable patch ID、最終treeを保持し、各commitは1-parent、merge commitは0件だった。復元後の通常状態ではopen master PRが0件で、本記録を追加するrelease準備PRを`CI Gate`と`CI Policy Guardian`の両方をrequiredにしたままrebase auto-mergeし、manual candidateのprotected-route証跡を完成させる。

## Repository tooling coverage Guardian rollout record (2026-09-17)

[PR #126](https://github.com/ShinjiKawamura255/flist-walker/pull/126)で、候補 branch の `CI Policy` job が `scripts/tests/` 全体と repository contract を実行するようにし、両 command の削除を immutable checker/test が拒否する構成へ更新した。exact head `371a3eca150ab30691c0429d5e5464e63bd7ec85` / base `f8b9327a78be5d07956de8ffb39daaec4ad968c6` の Cross Platform run `35115723937` は `CI Gate` job `104865651715`を含む全 job が成功した。旧 Guardian run `35115723943` / job `104860529611` は `.github/workflows/ci-cross-platform.yml`、`scripts/check_ci_policy.py`、`scripts/tests/test_check_ci_policy.py` の3 pathだけを期待どおり拒否した。PR #126は唯一のopen master PRでauto-merge未登録、独立pre-mutation reviewはblocking / major / minor 0件のGOだった。

変更前 snapshot の SHA-256 は `bf7d7e86655e854e726076cddcaaf27d8826efd0b505621a096ea230dfddf05b`、手動復元 payload は `ec2669cb80b2722b2391dcd88f1607ca9b74fe0f72e6dc253605bd1027bc9193` だった。required checksは`CI Gate` / `CI Policy Guardian`（ともにGitHub Actions app ID `15368`、`strict: true`）、approval `0`、administrators適用、linear history必須、force-push/deletion禁止、applicable branch rulesは空だった。repositoryはauto-merge有効、rebase-only、merge済みbranch自動削除有効、merge commit / squash無効だった。

review済みcontrol SHA-256 `9c39c19fb20f0351cc8b09a84748a35dc9398b94afd9caf9ae2ec802c8dbd153` は、自己hashと固定snapshotをauto-merge登録直前およびrequired-check変更直前に再照合した。controlだけがPR #126のrebase auto-mergeを1回登録し、独立watchdogを起動してからrequired checksを一時的に`CI Gate`だけへ限定した。PRは`fd055f6d3e8abfadb1aa47b76d7622dc121f0aff`へmergeされ、同じcontrolの`finally`によるrestore attempt 1で約1.1秒後に`CI Policy Guardian`の復元を検証した。完全なafter read-backでbranch protection全体、repository policy、applicable rulesが変更前snapshotと一致し、required checks、PR、master refを再確認した。候補remote branchの自動削除も確認した。

rebase auditではsource / mergedのparentがともにexact base、tree `ca29e742efc84f66efbc0987dcf4574e296519a7`、message、author、stable patch ID `8881f7f6d89100f9bf88741f2cdf96a8c15c77f9`が一致し、追加commitは1件、merge commitは0件だった。復元後のprotected-route証跡は[PR #127](https://github.com/ShinjiKawamura255/flist-walker/pull/127)とし、VM-009の恒久的な証跡要件と本rollout recordを別commitにした。`CI Gate`と`CI Policy Guardian`の両方をrequiredにしたexact final headを通常のrebase auto-mergeで通し、最終check、commit対応、merge outcomeをPR recordに残す。

## Release audit toolchain warning Guardian rollout record (2026-10-02)

[PR #161](https://github.com/ShinjiKawamura255/flist-walker/pull/161)はrelease audit installにRust `+1.97.1`を明示し、implicit/explicit Cargo selectorと複数の隣接installに対するfail-closed tool pin検査を追加した。runtime、依存、配布形式、署名、版番号、Action/runner pin、permissionは変えていない。exact base `4131f9f223ac72fd3f3ea79563428b8b7d2ece0e` / head `8e60e175a3b1f0f03cfd5b0b277d2e9dd8de4f66`に対し、[CI run36968593269](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/36968593269)の全10jobとGate check `110720248457`が成功した。[ready Guardian run36969647669](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/36969647669) / check `110720737128`はworkflow、checker、checker testの3 pathだけを期待どおり拒否した。

独立コードreviewで同一行の後続install見落としを修正し、64 Python tests、Rust1.97.1 fmt / locked clippy `-D warnings` / locked tests1522件 / audit0脆弱性、全8workflow YAML、repository contract、N-1 regressionが成功した。独立control reviewで復元marker後の監視、lock取得後のlive再照合、locktimeout retry、monitor outage時の期限保持と復元、登録解除後のmaster/base再照合を補強し、15 mock self-testsと最終fixed packet reviewが成功した。mock結果を実GitHub上の復元成功の代わりにはしていない。

ユーザはGuardianだけを一時的にrequiredから外してmergeし、直ちに復元する操作を明示承認した。変更前は唯一のopen master PR、ready、auto-merge未登録、head/base不変だった。snapshot SHA-256 `58cb4ae17f5367086db05980b88bb3eb789f82105435fe2a2d5b21eabb640485`、restore payload `c9f0dd388dfccb3843e477da9d64ff332b67ce38829f23f8b30e73fae27dc99b`、controller `b564238f5644ec3a59dfcc7be660fd4fc0fe3466b8df4eaadd194380cd29d797`、config `022a2d7611f27ef1edbaa4ab238edddee60c58c9783b84f15c366af8ab30e7e3`を独立reviewと実行時の照合に固定した。

required checksは`CI Gate` / `CI Policy Guardian`（ともにGitHub Actions app ID `15368`、strict）、administrator適用、linear history必須、approval0、force-push/deletion禁止、signature/conversation/lock/block/fork-sync無効、rulesetsとapplied branch rulesは空だった。repositoryはauto-merge有効、rebase-only、merged branch自動削除有効、merge/squash無効だった。controllerは登録直前とlock取得後のrequired-check変更直前にfull settings / PR / master / exact latest checkを再照合し、rebase auto-mergeを1回登録した。Gate/strict/admin等を維持したGuardian-only除外後、PRは2026-10-02 05:41:12 UTCに`1a2305ce34fa0c075a490902ce2bba79b3a84e58`へmergeされた。

`finally`のrestore attempt1でGuardianを直ちに復元し、Gate-only requestからfull復元read-backまで32.694秒だった。protection全体、非対象repository設定、rulesets、applied rulesの完全比較が変更前と一致し、PR/masterも一致した。180秒の独立watchdogは復元marker後も監視を続け、05:43:42 UTCの最終full read-backもPASS、late restore0件だった。controllerはMERGED / SETTLED、watchdogはFINAL PASS。source/merged各1commitの順序、message、author、stable patch ID、tree、1 parentが一致し、merge commit0件、remote feature branchの自動削除を確認した。

復元後の通常保護経路のproofは、本regression guardと本recordを分けた意味のある2commitのmachine PRで行う。両required checkのexact-head success、通常rebase auto-merge、commit対応と最終merge結果はproof PR recordに記録する。候補再生成、実機GUI/Windows TC-193、候補ごとのwarning判断、tag/publicationは別gateであり、このrollout結果でrelease完了とはしない。


## Indexing observation Guardian rollout record (2026-10-05)

[PR #169](https://github.com/ShinjiKawamura255/flist-walker/pull/169) adds weekly/manual Ubuntu indexing observations to the existing perf workflow. Exact base `956976e2d5d0e717b2b4200e2342120621692a4a` / source head `2e4f6766867e8867693dabddd9b32e6eb2d000fb` passed [CI37224580165](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37224580165) on all native platforms and CI Gate. [Old Guardian37224578881](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37224578881) rejected exactly `.github/workflows/perf-regression.yml`, `scripts/check_ci_policy.py` and `scripts/tests/test_check_ci_policy.py`. The full-scale StableEdit admission and seven shape-owned smoke leaves retain work/oracle/cleanup contracts; the smoke's aggregate resource budget change is explicit in the [hosted observation record](indexing-perf-hosted-2026-10-05.md). Five successful candidate sessions retain1190 raw rows. No coherent three exploratory plus two reserved runner cohort exists, so timing remains observation-only.

Independent overall FINAL reviewed the exact candidate, full settings snapshot, restore payload, controller and evidence with blocking/major/minor0. Subject SHA256 `5c1c166ce26b3cad71f70194526f638121327bc56e1e0b38d0e4d2656659c7eb`, bundle `7c1c5d62593690602c38e2d3fbf5023bbf4cf70b52e00023a2da9d87a243e09d`, executing controller `1576925c2af81349bb64532790458814f57d63ee6f192d4d749a4265d91380ae` identify that review. The [retained control evidence](indexing-perf-rollout-2026-10-05/evidence-manifest.json) binds original payload/code/test/report/live audit bytes; its archive roundtrip was checked. Local18 state tests and six actual fake-API process paths are separate from live restoration evidence.

The user explicitly approved the fixed Guardian-only operation. A single invocation armed an independent watchdog, checked exact source/actor/sole PR/policy/CI before registration and before the temporary PATCH, registered expected-head REBASE once, and temporarily retained only strict `CI Gate` app15368. PR169 merged at `2026-10-04T21:03:02Z` to `3823a0c861c46900616b4f84b09c5aa9f2f1db06`. The controller's `finally` immediately restored strict `CI Gate` plus `CI Policy Guardian`, both app15368, and compared complete repository policy, branch protection, applied rules/rulesets, PR and master. The independent watchdog returned `verified-main-restoration`; a fresh main readback also matched the original full policy. No other setting was changed. The180-second deadline starts restoration rather than guaranteeing API completion by that time.

Three ordered source/rebased commits retained message, author, tree, stable patch ID and one parent each; no merge commits were added. [Merged audit](indexing-perf-rollout-2026-10-05/merged-audit.json) records the mapping and automatic remote feature deletion.

| Source commit | Rebased commit | Stable patch ID |
| --- | --- | --- |
| `1bd6b356c474a09c259d4049e1dd8583253fe329` | `94f7eb0f980c024010e08742b1ac83797bdb6c79` | `cfc0443c90c3aa0efd7e59459e0ac0801ef0ab3e` |
| `6e95f527a2fb56b097292d80d8c8e08016ec0dee` | `dcb1eca2e52d6fafb0e186bce7a78682cdc8a452` | `00d7e115fed81735ba11ef550fa3d9ef508130bc` |
| `2e4f6766867e8867693dabddd9b32e6eb2d000fb` | `3823a0c861c46900616b4f84b09c5aa9f2f1db06` | `500e9876b3f4e0e21d0abb28fa45429ad6509d27` |

The restored normal-protection proof is the machine PR containing two meaningful commits: retained hosted observations/failures and navigation; then maintainer procedure/current posture and this rollout record. Both required checks remain active. Its exact checks, registration, ordered source/rebase audit and merge outcome belong to that proof PR record. [Master observation37234612163](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37234612163) was dispatched once on actual merged source `3823a0c861c46900616b4f84b09c5aa9f2f1db06`; it is separate from candidate cohorts, and its result must be resolved from that exact run. This record does not infer native input/IME, full44-cell collector coverage, percentile guarantees or product timing ceilings.
