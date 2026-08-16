---
name: flistwalker-pr-lifecycle
description: FlistWalker の GitHub PR を作成して rebase auto-merge を登録するとき、または auto-merge 完了後にローカル master を安全に同期して作業 branch を整理するときに使う。
---

# FlistWalker PR Lifecycle

PR の作成から rebase auto-merge、merge 後のローカル同期と branch 整理までを一貫して扱う。GitHub 側の merge 完了を確認するまで、ローカル状態を変えない。

## PR 作成と auto-merge 登録

1. `git status --short` が空であることを確認し、local `master` に移動する。`git fetch origin --prune`、`git pull --ff-only origin master` を実行し、現在 branch が `master`、かつ `git rev-parse master` と `git rev-parse origin/master` が一致することを確認する。いずれかに失敗した場合は branch を作成せず停止する。
2. 最初の commit 前に `git switch -c codex/<topic>` で feature branch を作成する。local `master` で commit を作成してはならない。
3. `git status --short` で作業ツリーを確認し、PR に含める変更だけを commit / push する。
4. PR を作成し、PR 番号と head branch 名を作業記録に保持する。対象 PR に対して `gh pr merge <number> --auto --rebase --delete-branch` を 1 回だけ登録する。
5. 登録結果を PR 情報で確認する。merge method の変更、admin bypass、`master` への直接 push は行わない。

## マージ完了の確認

1. GitHub を正本として `gh pr view <number> --json state,mergedAt,baseRefName,headRefName,url` を確認する。`state` が `MERGED`、`mergedAt` が存在し、`baseRefName` が `master`、`headRefName` が記録済みの削除候補 branch と完全一致することを必須にする。
2. `OPEN`、`CLOSED`、merge 待ち、チェック失敗、または API 取得失敗の場合はローカル同期・削除を行わず、状態と次の確認条件を報告する。
3. auto-merge は active task の終了後に完了し得る。終了済み task はローカルを自動操作できないため、マージ完了を待つ task または次の明示的な後処理 task でこの確認から再開する。

## ローカル同期と branch 整理

1. `git status --short` が空であることを確認する。空でなければ停止し、利用者の変更を触らない。
2. `git fetch origin --prune` を実行し、PR の head branch と `origin/master` の状態を更新する。
3. local `master` に移動し、`git pull --ff-only origin master` を実行する。fast-forward できなければ停止し、rebase、merge commit、reset で解消しない。
4. 現在 branch が `master` であること、および `git rev-parse master` と `git rev-parse origin/master` が一致することを確認する。確認不能なら停止する。
5. 削除候補が `master` 以外であることを確認する。`git worktree list --porcelain` の `branch refs/heads/<head-branch>` を確認し、候補 branch を使用する worktree がないことを確認する。
6. `master` に到達可能であれば `git branch -d -- <head-branch>` で通常削除する。rebase によりこの通常削除だけが拒否された場合、記録済み PR 番号に対して `gh pr view <number> --json state,mergedAt,baseRefName,headRefName,url` を改めて実行する。次の全てを確認する: PR が `MERGED`、`mergedAt` が存在、base が `master`、head が候補 branch と完全一致すること; current `master == origin/master`; 対象が `master` 以外で未使用であること; `git rev-list --merges origin/master..refs/heads/<head-branch>` が空であること; `git log --cherry-pick --right-only --no-merges origin/master...refs/heads/<head-branch>` が空であること。全条件を満たすときだけ、local branch に `git branch -D -- <head-branch>` を実行してよい。確認コマンドが失敗した場合または出力が空でない場合は停止する。
7. remote feature branch は GitHub の auto-delete に委ねる。手動削除しない。

## 禁止事項

- 未マージ PR、`master` 以外を base にした PR、dirty worktree、同期失敗、または branch 使用中で後処理を続行しない。
- `git reset --hard`、`master` への rebase/merge、`git push --delete`、`master` への直接 push、admin bypass を使わない。
- `git branch -D` は前節の rebase 書き換え後の local feature branch だけに許可される例外であり、`master`、dirty state、未マージまたはPR不一致、同期不能、worktree 使用中、merge commit を含む branch、または patch 非等価 branch には使わない。
- auto-merge の登録を重複させない。

## 完了報告

次を簡潔に報告する: PR 番号と merge 確認結果、同期した `master` の commit、削除した local branch、または停止理由。GitHub 側の merge 完了確認とローカル cleanup の成否を混同しない。
