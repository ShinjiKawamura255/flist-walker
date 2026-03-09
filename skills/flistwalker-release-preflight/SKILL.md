---
name: flistwalker-release-preflight
description: FlistWalker の release 前に version 更新漏れを防ぎたいときに使う。特に rust/Cargo.toml と rust/Cargo.lock、CHANGELOG、tag の整合確認と更新手順を明確化する。
---

# FlistWalker Release Preflight

## 参照元
- `/mnt/d/work/flistwalker/rust/Cargo.toml`
- `/mnt/d/work/flistwalker/rust/Cargo.lock`
- `/mnt/d/work/flistwalker/CHANGELOG.md`
- `/mnt/d/work/flistwalker/docs/RELEASE.md`

## この skill の主目的
- release 前に `rust/Cargo.toml` と `rust/Cargo.lock` の version 更新漏れを防ぐ。
- 配布物の版数と changelog / tag / release note の不整合を出さない。
- 更新手順を毎回同じ順序で実施する。

## 手順
1. 対象 version を `vX.Y.Z` 形式で確定する。
2. `rust/Cargo.toml` の `[package].version` を `X.Y.Z` へ更新する。
3. `cd /mnt/d/work/flistwalker/rust && cargo check --locked` を実行し、`Cargo.lock` が古くて失敗するか確認する。
4. `--locked` で失敗した場合、`cd /mnt/d/work/flistwalker/rust && cargo check` を実行して `Cargo.lock` を更新する。
5. `rust/Cargo.lock` 内の `name = "flist-walker"` 節の `version = "X.Y.Z"` を確認する。
6. `CHANGELOG.md` に対象 version の節と日付、主要変更を反映する。
7. tag 名 `vX.Y.Z`、`CHANGELOG.md`、`rust/Cargo.toml`、`rust/Cargo.lock` の version が一致していることを確認する。
8. release 本文が必要なら `skills/flistwalker-release-notes/SKILL.md` を続けて使う。

## チェック
- `rust/Cargo.toml` の version が対象 release と一致しているか。
- `rust/Cargo.lock` の root package `flist-walker` version が一致しているか。
- `CHANGELOG.md` の対象節が同じ version になっているか。
- `cargo check --locked` または `cargo build --release --locked` が成功する状態か。

## 禁止事項
- `Cargo.toml` だけ更新して `Cargo.lock` を未確認のまま release しない。
- `CHANGELOG.md` の版数と tag をずらしたまま release しない。
- 複数 version の変更を 1 回の release 用更新に混在させない。
