---
name: flistwalker-pr-lifecycle
description: FlistWalker の GitHub PR を作成して rebase auto-merge を登録するとき、または auto-merge 完了後にローカル master を安全に同期して作業 branch を整理するときに使う。
---

# FlistWalker PR Lifecycle

PR の作成から rebase auto-merge、merge 後のローカル同期と branch 整理までを一貫して扱う。GitHub 側の merge 完了を確認するまで、ローカル状態を変えない。

## PR 作成と auto-merge 登録

1. `git status --short` で作業ツリーを確認し、PR に含める変更だけを commit / push する。
2. PR を作成し、対象 PR に対して `gh pr merge <number> --auto --rebase --delete-branch` を 1 回だけ登録する。
3. 登録結果を PR 情報で確認する。merge method の変更、admin bypass、`master` への直接 push は行わない。

## マージ完了の確認

1. GitHub を正本として `gh pr view <number> --json state,mergedAt,baseRefName,headRefName,url` を確認する。`state` が `MERGED`、`mergedAt` が存在し、`baseRefName` が `master` であることを必須にする。
2. `OPEN`、`CLOSED`、merge 待ち、チェック失敗、または API 取得失敗の場合はローカル同期・削除を行わず、状態と次の確認条件を報告する。
3. auto-merge は active task の終了後に完了し得る。終了済み task はローカルを自動操作できないため、マージ完了を待つ task または次の明示的な後処理 task でこの確認から再開する。

## ローカル同期と branch 整理

1. `git status --short` が空であることを確認する。空でなければ停止し、利用者の変更を触らない。
2. `git fetch origin --prune` を実行し、PR の head branch と `origin/master` の状態を更新する。
3. local `master` に移動し、`git pull --ff-only origin master` を実行する。fast-forward できなければ停止し、rebase、merge commit、reset で解消しない。
4. `HEAD` と `origin/master` が一致することを確認する。
5. `git worktree list --porcelain` で PR の head branch が他 worktree で使用されていないことを確認する。head branch が `master` に到達可能である場合だけ `git branch -d <head-branch>` を実行する。削除対象が `master` でないことを確認する。
6. remote feature branch は GitHub の auto-delete に委ねる。手動削除しない。

## 禁止事項

- 未マージ PR、`master` 以外を base にした PR、dirty worktree、同期失敗、または branch 使用中で後処理を続行しない。
- `git reset --hard`、`git branch -D`、`git push --delete`、`master` への直接 push、admin bypass を使わない。
- auto-merge の登録を重複させない。

## 完了報告

次を簡潔に報告する: PR 番号と merge 確認結果、同期した `master` の commit、削除した local branch、または停止理由。GitHub 側の merge 完了確認とローカル cleanup の成否を混同しない。
