---
name: flistwalker-release-preflight
description: FlistWalker の release 前に version 更新漏れを防ぎたいときに使う。特に rust/Cargo.toml と rust/Cargo.lock、CHANGELOG、tag の整合確認と更新手順を明確化する。
---

# FlistWalker Release Preflight

## 参照元
- `rust/Cargo.toml`
- `rust/Cargo.lock`
- `CHANGELOG.md`
- `docs/RELEASE.md`
- `AGENTS.md`

## この skill の主目的
- release 前に `rust/Cargo.toml` と `rust/Cargo.lock` の version 更新漏れを防ぐ。
- 配布物の版数と changelog / tag / release note の不整合を出さない。
- 更新手順を毎回同じ順序で実施する。

## 手順
1. 対象 version を `vX.Y.Z` 形式で確定する。
2. tag をまだ作っていないことを確認する。tag 済みなら、version 不一致を黙認せずユーザへ明示する。
3. `rust/Cargo.toml` の `[package].version` を `X.Y.Z` へ更新する。
4. `rust/Cargo.lock` 内の `name = "flist-walker"` 節の `version = "X.Y.Z"` を確認し、不一致なら更新する。
5. `CHANGELOG.md` に対象 version の節と日付、主要変更を反映する。
6. `cargo test` を実行し、少なくとも version 更新で壊れていないことを確認する。
7. tag 名 `vX.Y.Z`、`CHANGELOG.md`、`rust/Cargo.toml`、`rust/Cargo.lock` の version が一致していることを確認する。
8. この確認が終わるまで tag 作成・push・draft release publish を行わない。
9. release 本文が必要なら `skills/flistwalker-release-notes/SKILL.md` を続けて使う。

## チェック
- `rust/Cargo.toml` の version が対象 release と一致しているか。
- `rust/Cargo.lock` の root package `flist-walker` version が一致しているか。
- `CHANGELOG.md` の対象節が同じ version になっているか。
- tag 作成前か。すでに tag があるなら不一致のまま進めていないか。
- `cargo test` が成功する状態か。

## 禁止事項
- `Cargo.toml` だけ更新して `Cargo.lock` を未確認のまま release しない。
- `CHANGELOG.md` の版数と tag をずらしたまま release しない。
- 複数 version の変更を 1 回の release 用更新に混在させない。
- version 整合確認前に tag を作成したり publish しない。
