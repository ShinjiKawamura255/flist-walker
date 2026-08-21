# CI and Machine-PR Operations

FlistWalker は AI agent と dependency automation による機械 PR を標準の変更経路とする。人手の approving review は要求しないが、`master` への直接 push と admin bypass は許可しない。

## Merge contract

- すべての変更は PR にし、required check は `CI Gate` と `CI Policy Guardian` とする。
- required approving review は 0 件とする。PR を作成した agent は `gh pr merge --auto --rebase --delete-branch` 相当を1回だけ登録する。merge commit と squash merge は許可せず、`master` は linear history を維持する。
- `CI Gate` は change detection、CI policy、Windows/macOS/Linux test/build、Windows GNU test-channel artifact の Windows 上 headless sandbox self-update、clippy/coverage、および条件付き Cargo audit を集約する。GNU updater E2E は loopback feed、test-only key、使い捨て sentinel-owned sandbox に限定し、production release workflow/鍵へ混入させない。Cargo 関連変更で audit が skipped の場合は gate を失敗させ、非 Cargo 変更での skipped だけを正常とする。
- `CI Policy Guardian` は `pull_request_target` で default branch の trusted checker を checkoutし、PR head の workflow/pin/Dependabot policy blob だけを GitHub API から一時領域へ取得して data として検査する。PR head の checkout/実行、secret、cache、artifact、write permission は使用しない。
- workflow一式、Dependabot設定、toolchain定義、audit exception設定、checker本体とtestはfail-closedなtrusted policy setとし、通常PRではrunner世代、Rust/Cargo tool version、full-SHA Action pinだけを変更できる。構造変更やaccepted advisory変更は設定snapshot、独立agent review、一時的required-check変更、即時復元、protected-route再検証を一体で行う専用rolloutとする。
- ローカルの意味あるコミット境界・順序・message・author は rebase merge で保持する。GitHub が新しい commit SHA と committer metadata を生成することは許容する。
- feature branch は任意のタイミングで push してよい。履歴整理が必要な場合の force-with-lease は非保護 feature branch に限り、`master` の force push、branch deletion、直接 push、admin bypass で gate を回避してはならない。merge 済み remote feature branch は GitHub が自動削除する。
- ローカル変更は、clean な `master` で `git fetch origin --prune`、`git pull --ff-only origin master`、現在 branch が `master`、かつ `master == origin/master` を確認してから、最初の commit 前に feature branch を作成して開始する。この事前同期に失敗した場合は branch 作成も停止し、既存の分岐した `master` は別の明示的回復手順へ委ねる。auto-merge の実完了を確認した active task は [`skills/flistwalker-pr-lifecycle/SKILL.md`](../skills/flistwalker-pr-lifecycle/SKILL.md) に従い、clean worktree のみ `git fetch origin --prune`、`git switch master`、`git pull --ff-only origin master` を実行する。現在 branch が `master` で `master == origin/master` を再確認できた場合に限る。PR の head branch は通常、`master` に到達可能で他 worktree に使用されていない場合だけ `git branch -d` でローカルから削除する。GitHub rebase が SHA を書き換えて通常削除だけが拒否された場合に限り、PR 番号で再照会した `MERGED` / `mergedAt` / base `master` / exact head branch、一致した同期済み `master`、対象が `master` 以外、対象 branch を使用する worktree がないこと、`git rev-list --merges origin/master..refs/heads/<head-branch>` が空であること、`git log --cherry-pick --right-only --no-merges origin/master...refs/heads/<head-branch>` が空であることを全て確認してから、local branch に限り `git branch -D -- <head-branch>` を実行してよい。任意の確認コマンドの失敗または非空出力では同期・削除を行わず状態を報告する。`git reset --hard`、`master` への rebase/merge、remote branch の手動削除はこの後処理で使わない。
- Dependabot PR は CI 成功後に `.github/workflows/dependabot-auto-merge.yml` が同じ rebase auto-merge を1回だけ登録する。
- 失敗を再実行だけで消してはならない。runner/network/cache など外部一時障害と判断できる証跡がある場合に限り、run URL と判断を残して再実行する。

## Version-addressed required environment

| Surface | Required value |
| --- | --- |
| Rust | `1.97.1` |
| Linux runner generation | `ubuntu-24.04` |
| Windows runner generation | `windows-2025-vs2026` |
| macOS arm64 runner generation | `macos-26` |
| macOS x64 release runner generation | `macos-26-intel` |
| cargo-audit | `0.22.2` |
| cargo-llvm-cov | `0.8.7` |

GitHub-hosted runner の番号付き label は runner 世代を固定するが、image 内の OS package までは immutable にしない。各 job は `ImageOS` と `ImageVersion` を step summary に残す。Actions は full commit SHA で固定し、cache は Cargo download data に限定して tool binary と `rust/target` を共有しない。

## Security and latest-version signals

- Cargo 関連 path は任意階層の `Cargo.toml` / `Cargo.lock`、`rust/.cargo/audit.toml`、required/security audit workflow、CI policy checker/test とする。該当 PR は required gate 内で `cargo audit` を実行する。
- scheduled security audit は毎日実行し、後日公開された advisory も検知する。default branch の失敗時だけ dedicated issue を同じ run で作成または更新し、agent は 24 時間以内に原因を分類する。default branch の後続 run が成功した場合は、完全一致タイトルかつ `github-actions` bot 所有の open issue だけを recovery run URL 付きで自動 close する。
- latest canary は週次で `ubuntu-latest` / `windows-latest` / `macos-latest` と Rust stable を検証する。default branch の失敗時だけ dedicated issue を同じ run で作成または更新し、agent は 7 日以内に原因を分類する。default branch の後続 run が成功した場合は security audit と同じ bot 所有・完全一致タイトル条件で dedicated issue を自動 close する。
- canary と scheduled audit は branch protection の required check に追加しない。前者は将来互換性、後者は時間経過で変化する security intelligence を観測する。

## Stateful endurance signal

- `.github/workflows/stateful-endurance.yml` は水曜 19:00 UTC の週次実行と手動 dispatch で、拡張 deterministic corpus と実 worker soak を実行する。通常 PR は `CI Cross Platform` 内の短い fixed/seeded profile を gate とし、この workflow は required check に追加しない。
- 週次既定値は deterministic `base_seed=0x18400000`、`seed_count=1000`、`steps=1000`、実 worker soak `1200` 秒とする。手動 dispatch では seed count 10,000、steps 100,000、soak 1,800 秒の安全上限内で上書きできる。
- deterministic 失敗は log の seed と replay command を使って `FLISTWALKER_ENDURANCE_SEED=<seed> cargo test --locked stateful_endurance_replay --lib -- --ignored --nocapture` で再現する。artifact `stateful-endurance-<run_id>` は deterministic / real-worker log を 14 日保持する。
- real-worker profile は runner の temporary root だけを使用し、外部 action、updater、network endpoint を呼ばない。失敗時は artifact と runner image を確認し、product regression、hosted image drift、resource exhaustion、external transient に分類して 7 日以内に追跡する。
- workflow/checker は Guardian の immutable trusted policy set に属する。新設または構造変更の merge は通常 PR の Guardian 失敗を期待値とし、設定 snapshot、独立 review、exact head の `CI Gate` 成功、競合 PR と base/head 不変、一時的 required-check 変更、merge 直後の完全復元/read-back、通常保護経路の後続 PR を一体で実施する。

### First hosted proof and scale baseline (2026-08-20)

- Default branch `f1800aa9` の manual dispatch [run 32340613009](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/32340613009) は 23分42秒で成功した。deterministic 1,000 seeds x 1,000 steps、real-worker 1,200秒 soak、artifact upload、post処理がすべて成功した。
- Artifact `stateful-endurance-32340613009` は 5,873 bytes、14日保持（expiry `2026-09-03T07:04:41Z`）として API read-back 済みである。初回 hosted proof は workflow が required check ではないという運用を変更しない。
- TC-185 は既存 heavy weekly perf command の一部として、exactly 1,000,000 candidates、selective/dense の2 shape x 7 samples、nearest-rank p50/p95/p99、4 RSS phaseを stable `tc_185` labelで出力する。RSSは allocator/host差を観測する baselineであり、十分なhosted履歴が蓄積するまで閾値違反として扱わない。

## Pin update triggers and promotion

次のいずれかで pin 更新を検討する。

1. latest canary が失敗し、現行 pin と latest の互換差が判明した。
2. latest canary が 2 回連続成功し、通常の追随更新時期になった。
3. Rust、runner image、Action、CI tool の security notice、EOL、deprecation deadline が公開された。
4. dependency の MSRV または build requirement が現行 Rust/runnerを上回った。
5. hosted runner image version の更新後に required CI の挙動差が観測された。

通常の追随更新は 2 回連続の scheduled canary 成功を必要とする。security/EOL/deprecation の期限対応はこの待機を省略できるが、いずれも candidate PR で CI policy test、`CI Policy Guardian`、`CI Gate` を通してから pin を変更する。runner/action/tool の更新と製品依存更新は、原因とrollback単位を分離できる限り別 PR にする。

## Failure handling and rollback

- `CI Gate` / `CI Policy Guardian` failure は最小の failed job とログを特定し、product regression、policy violation、security advisory、hosted image drift、external transient に分類する。
- monitor issue の自動 close は workflow 本体の成功にだけ連動させる。別タイトル、利用者作成 issue、失敗継続中の issue は close せず、復旧確認後も open のままなら workflow の exact-title / bot-owner query と `issues: write` permission を確認する。
- version promotion が失敗した場合は candidate PR を閉じ、required pin を維持する。既に merge 済みなら、直前の version table と full action SHA へ戻す revert PR を作る。
- branch protection 変更前は repository 設定と protection 全体を取得し、変更後は merge method、linear history、feature branch自動削除、PR requirement、approval count、required context/source、force-push/deletion、auto-merge、および非対象フィールドを read back する。
- repository policy の rollout record はこの文書へ残す。record には変更前後の要点、protection/ruleset identifier、旧 auto-merge 値、復元方法、protected auto-merge PR を含める。
- trusted policy の構造変更を戻す場合も、reverse head の独立レビュー、`CI Gate` 成功、競合PRなし、base/head不変を確認する。`CI Policy Guardian` をrequiredから一時的に外してrevert PRをrebase mergeし、直ちにGuardianと変更前のrepository/protection payloadを復元して全項目をread backする。

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
