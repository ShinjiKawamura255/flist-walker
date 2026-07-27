# CI and Machine-PR Operations

FlistWalker は AI agent と dependency automation による機械 PR を標準の変更経路とする。人手の approving review は要求しないが、`master` への直接 push と admin bypass は許可しない。

## Merge contract

- すべての変更は PR にし、required check は `CI Gate` と `CI Policy Guardian` とする。
- required approving review は 0 件とする。PR を作成した agent は `gh pr merge --auto --merge` 相当で PR ごとの auto-merge を明示登録する。
- `CI Gate` は change detection、CI policy、Windows/macOS/Linux test/build、clippy/coverage、および条件付き Cargo audit を集約する。Cargo 関連変更で audit が skipped の場合は gate を失敗させ、非 Cargo 変更での skipped だけを正常とする。
- `CI Policy Guardian` は `pull_request_target` で default branch の trusted checker を checkoutし、PR head の workflow/pin/Dependabot policy blob だけを GitHub API から一時領域へ取得して data として検査する。PR head の checkout/実行、secret、cache、artifact、write permission は使用しない。
- workflow一式、Dependabot設定、toolchain定義、checker本体はfail-closedなtrusted policy setとし、通常PRではrunner世代、Rust/Cargo tool version、full-SHA Action pinだけを変更できる。構造変更は設定snapshot、独立agent review、一時的required-check変更、即時復元、protected-route再検証を一体で行う専用rolloutとする。
- force push、branch deletion、直接 push、admin bypass で gate を回避してはならない。
- Dependabot PR は CI 成功後に `.github/workflows/dependabot-auto-merge.yml` が同じ auto-merge を登録する。
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
- scheduled security audit は毎日実行し、後日公開された advisory も検知する。失敗時は dedicated issue を同じ run で作成または更新し、agent は 24 時間以内に原因を分類する。
- latest canary は週次で `ubuntu-latest` / `windows-latest` / `macos-latest` と Rust stable を検証する。失敗時は dedicated issue を同じ run で作成または更新し、agent は 7 日以内に原因を分類する。
- canary と scheduled audit は branch protection の required check に追加しない。前者は将来互換性、後者は時間経過で変化する security intelligence を観測する。

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
- version promotion が失敗した場合は candidate PR を閉じ、required pin を維持する。既に merge 済みなら、直前の version table と full action SHA へ戻す revert PR を作る。
- branch protection 変更前は現行設定を取得し、変更後は PR requirement、approval count、required context/source、force-push/deletion、auto-merge を read back する。
- repository policy の rollout record はこの文書へ残す。record には変更前後の要点、protection/ruleset identifier、旧 auto-merge 値、復元方法、protected auto-merge PR を含める。

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
