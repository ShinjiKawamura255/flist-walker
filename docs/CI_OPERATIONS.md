# CI and Machine-PR Operations

FlistWalker は AI agent と dependency automation による機械 PR を標準の変更経路とする。人手の approving review は要求しないが、`master` への直接 push と admin bypass は許可しない。

## Merge contract

- すべての変更は PR にし、required check は `CI Gate` と `CI Policy Guardian` とする。
- required approving review は 0 件とする。PR を作成した agent は `gh pr merge --auto --rebase --delete-branch` 相当を1回だけ登録する。merge commit と squash merge は許可せず、`master` は linear history を維持する。
- `CI Gate` は change detection、CI policy、Windows/macOS/Linux test/build、Windows GNU test-channel artifact の Windows 上 headless sandbox self-update、clippy/coverage、および条件付き Cargo audit を集約する。heavy CIをskipできるのは、追加・変更（`A`/`M`）された全pathが`docs/**`、指定top-level文書、issue/release templateのallowlist内にある場合だけとする。rename/delete、unknown path、base SHA不明、diff失敗はheavy CI実行へfail closedする。GNU updater buildはnative matrixから独立させ、E2Eはそのproducerだけを待つ。GNU updater E2E は loopback feed、test-only key、使い捨て sentinel-owned sandbox に限定し、production release workflow/鍵へ混入させない。Cargo 関連変更で audit が skipped の場合は gate を失敗させ、非 Cargo 変更での skipped だけを正常とする。
- heavy PR CIはLinuxのclippy/coverageに加えてmacOS/Windows native jobでlocked clippyを実行し、platform cfg固有warningをtag作成前にblockする。tag release preflightはLinux/macOS/Windows nativeの各jobでlocked test/clippyを再実行し、release policy checker/testはclippy stepを特定platformへ限定するconditionを拒否する。
- `Release Tagged Build` の manual candidate は default branch の明示versionだけを受理し、read-only jobでtag pushと同じnative preflight、audit、4 platform asset build、bundle/signature/inventory/N-1検証を実行してvalidated artifactを14日保持する。`contents: write`と`gh release create`はtag push専用jobだけに置き、manual runでは必ずskipする。これによりtag作成前に実物bundleを検証しつつ、candidate modeからreleaseを公開できないようにする。
- `CI Policy Guardian` は `pull_request_target` で default branch の trusted checker を checkoutし、PR head の workflow/pin/Dependabot policy blob だけを GitHub API から一時領域へ取得して data として検査する。PR head の checkout/実行、secret、cache、artifact、write permission は使用しない。
- `CI Policy` job は候補 branch 上で `scripts/tests/` の全 Python unit test を discovery 実行し、続けて repository contract を CLI として実行する。trusted checker は両コマンドを必須 token とし、CI policy test は片方でも削除された workflow を拒否する。
- workflow一式、Dependabot設定、toolchain定義、audit exception設定、checker本体とtestはfail-closedなtrusted policy setとし、通常PRではrunner世代、Rust/Cargo tool version、full-SHA Action pinだけを変更できる。構造変更やaccepted advisory変更は設定snapshot、独立agent review、一時的required-check変更、即時復元、protected-route再検証を一体で行う専用rolloutとする。
- ローカルの意味あるコミット境界・順序・message・author は rebase merge で保持する。GitHub が新しい commit SHA と committer metadata を生成することは許容する。
- feature branch は任意のタイミングで push してよい。履歴整理が必要な場合の force-with-lease は非保護 feature branch に限り、`master` の force push、branch deletion、直接 push、admin bypass で gate を回避してはならない。merge 済み remote feature branch は GitHub が自動削除する。
- ローカル変更は、clean な `master` で `git fetch origin --prune`、`git pull --ff-only origin master`、現在 branch が `master`、かつ `master == origin/master` を確認してから、最初の commit 前に feature branch を作成して開始する。この事前同期に失敗した場合は branch 作成も停止し、既存の分岐した `master` は別の明示的回復手順へ委ねる。auto-merge の実完了を確認した active task は [`skills/flistwalker-pr-lifecycle/SKILL.md`](../skills/flistwalker-pr-lifecycle/SKILL.md) に従い、clean worktree のみ `git fetch origin --prune`、`git switch master`、`git pull --ff-only origin master` を実行する。現在 branch が `master` で `master == origin/master` を再確認できた場合に限る。PR の head branch は通常、`master` に到達可能で他 worktree に使用されていない場合だけ `git branch -d` でローカルから削除する。GitHub rebase が SHA を書き換えて通常削除だけが拒否された場合に限り、PR 番号で再照会した `MERGED` / `mergedAt` / base `master` / exact head branch、一致した同期済み `master`、対象が `master` 以外、対象 branch を使用する worktree がないこと、`git rev-list --merges origin/master..refs/heads/<head-branch>` が空であること、`git log --cherry-pick --right-only --no-merges origin/master...refs/heads/<head-branch>` が空であることを全て確認してから、local branch に限り `git branch -D -- <head-branch>` を実行してよい。任意の確認コマンドの失敗または非空出力では同期・削除を行わず状態を報告する。`git reset --hard`、`master` への rebase/merge、remote branch の手動削除はこの後処理で使わない。
- Dependabot PR は CI 成功後に `.github/workflows/dependabot-auto-merge.yml` が同じ rebase auto-merge を1回だけ登録する。
- 失敗を再実行だけで消してはならない。runner/network/cache など外部一時障害と判断できる証跡がある場合に限り、run URL と判断を残して再実行する。

### Regression Guard: updater E2E variant completeness

- Scenario: updater E2EをUniversal単独からUniversal/Fwへ拡張した後もtrusted checkerが旧single-artifact pathと単一payload markerを要求し、新workflowを拒否する一方でfw側の欠落を検出できない。
- Expected Behavior: VM-009はUniversal/Fw両方のexact artifact path、別々のpayload marker、両variant定義、`-Variant`付きsandbox invocationのいずれか1つでも欠ければ失敗する。
- Non-goals: production release feedへの接続、test-channel keyのrelease workflowへの導入、同一sandboxのvariant間共有。
- Related Tests: `test_ci_contract_requires_both_windows_gnu_updater_variants_regression`。
- Notes for Future Changes: updater variantを追加または改名する場合はworkflow、trusted checker、各tokenのnegative testを同一変更で更新する。

### Regression Guard: fail-closed documentation skip and updater DAG

- Scenario: docs-only変更でも全platform release buildとupdater E2Eが約18-19分走る一方、単純なRust path denylistへ置き換えるとunknown/rename/deleteを誤ってskipできる。またE2EがGNU artifactだけを使うのにnative matrix全体を待つとcritical pathが直列化する。
- Expected Behavior: 通常のallowlisted documentation `A`/`M`だけはnative matrix、GNU producer/E2E、clippy/coverageをすべてskipし、`CI Gate`が全jobの`skipped`を確認する。それ以外は全jobの`success`を要求する。GNU E2Eは`windows-gnu-updater-build`だけをartifact producerとして待つ。
- Non-goals: platform test、coverage threshold、Cargo audit、Universal/Fw E2Eの削除、unknown pathの推測skip、hosted queue時間の保証。
- Related Tests: `test_heavy_ci_change_classification_regression`, `test_heavy_ci_result_truth_table_regression`, `test_ci_contract_requires_fail_closed_heavy_ci_skip_regression`, `test_ci_contract_requires_both_windows_gnu_updater_variants_regression`。updater N-1 checkerは、Guardianで不変化された`test_required_policy_regression_executes_updater_checker_golden_contract`が通常PRの`CI Policy`内で候補checkerを直接実行して保護する。
- Notes for Future Changes: allowlist拡張はpathの実行可能性を確認し、classificationとjob-result truth tableのnegative testを同一変更で追加する。workflow/checker/testの構造変更は[Controlled trusted-policy rollout](#controlled-trusted-policy-rollout)を使う。

### Regression Guard: repository tooling coverage

- Scenario: required CI が immutable policy test だけを明示実行し、worktree preflight、repository contract、validation routing の unit regression がローカル検証にしか現れない。
- Expected Behavior: `CI Policy` job は `python -m unittest discover -s scripts/tests` と `python scripts/check_repo_contract.py` を実行し、trusted checker/test はどちらか一方の command 削除も拒否する。
- Non-goals: Rust test matrix、coverage threshold、Guardian の trusted-base / read-only 境界、repository contract の責務内容の変更。
- Related Tests: `test_ci_contract_requires_repository_tooling_suite_regression` と discovery 配下の全 test。
- Notes for Future Changes: test layout または canonical entrypoint を変更する場合は workflow、checker、negative test、VM-009 を同一 controlled rollout で更新する。

### Timing baseline and hosted acceptance (2026-08-25)

[Historical evidence](history/ci-rollouts.md#timing-baseline-and-hosted-acceptance-2026-08-25).

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
- TC-185 は既存 heavy weekly perf command の一部として、exactly 1,000,000 candidates、selective/dense の2 shape x 7 samples、nearest-rank p50/p95/p99、4 RSS phaseを stable `tc_185` labelで出力する。RSSは allocator/host差を観測する baselineであり、十分なhosted履歴が蓄積するまで閾値違反として扱わない。

### First hosted proof and scale baseline (2026-08-20)

[Historical evidence](history/ci-rollouts.md#first-hosted-proof-and-scale-baseline-2026-08-20).

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
- repository policy の rollout record は [CI rollout history](history/ci-rollouts.md) へ残す。record には変更前後の要点、protection/ruleset identifier、旧 auto-merge 値、復元方法、protected auto-merge PR を含める。
- trusted policy の構造変更を戻す場合も、reverse head の独立レビュー、`CI Gate` 成功、競合PRなし、base/head不変を確認する。`CI Policy Guardian` をrequiredから一時的に外してrevert PRをrebase mergeし、直ちにGuardianと変更前のrepository/protection payloadを復元して全項目をread backする。

## Controlled trusted-policy rollout

trusted policy の構造変更では、明示承認、設定 snapshot、独立 final review、exact-head `CI Gate` 成功、競合なし、base/head 不変、一時的 required-check 変更、rebase merge、即時復元/read-back、通常保護経路の proof PR を省略しない。実行条件、control と watchdog、完全復元、2 commit 以上の proof の証跡要件は [VM-009](testplan/validation/vm-009.md#immutable-policy-rollout-evidence) に従う。

現在の手順と制約はこの文書が所有する。過去の snapshot、PR/run、commit 対応、所要時間は [CI rollout history](history/ci-rollouts.md) に記録する。過去の実施記録を現在の承認や現在 HEAD の検証結果として扱わない。

## Repository policy rollout record (2026-07-28)

[Historical evidence](history/ci-rollouts.md#repository-policy-rollout-record-2026-07-28).

## Guardian controlled rollout record (2026-07-28)

[Historical evidence](history/ci-rollouts.md#guardian-controlled-rollout-record-2026-07-28).

## Rebase-only rollout record (2026-07-28)

[Historical evidence](history/ci-rollouts.md#rebase-only-rollout-record-2026-07-28).

## Scheduled monitor recovery rollout record (2026-08-19)

[Historical evidence](history/ci-rollouts.md#scheduled-monitor-recovery-rollout-record-2026-08-19).

## Stateful endurance Guardian rollout record (2026-08-20)

[Historical evidence](history/ci-rollouts.md#stateful-endurance-guardian-rollout-record-2026-08-20).

## Windows GNU updater E2E Guardian rollout record (2026-08-22)

[Historical evidence](history/ci-rollouts.md#windows-gnu-updater-e2e-guardian-rollout-record-2026-08-22).

## Updater and UI hardening Guardian rollout record (2026-08-25)

[Historical evidence](history/ci-rollouts.md#updater-and-ui-hardening-guardian-rollout-record-2026-08-25).

## Updater checksum and CI timing Guardian rollout record (2026-08-26)

[Historical evidence](history/ci-rollouts.md#updater-checksum-and-ci-timing-guardian-rollout-record-2026-08-26).

## Native release warning gate Guardian rollout record (2026-08-26)

[Historical evidence](history/ci-rollouts.md#native-release-warning-gate-guardian-rollout-record-2026-08-26).

## Post-release review Guardian rollout record (2026-08-26)

[Historical evidence](history/ci-rollouts.md#post-release-review-guardian-rollout-record-2026-08-26).

## Pre-tag release candidate Guardian rollout record (2026-09-04)

[Historical evidence](history/ci-rollouts.md#pre-tag-release-candidate-guardian-rollout-record-2026-09-04).

## Repository tooling coverage Guardian rollout record (2026-09-17)

[Historical evidence](history/ci-rollouts.md#repository-tooling-coverage-guardian-rollout-record-2026-09-17).
