---
name: flistwalker-release-preflight
description: FlistWalker の release/tag/publish 前に使う。version 更新漏れ、docs 追従、文書間整合、OSS notice、asset 名、GUI 証跡、CI gate の release readiness を確認する。
---

# FlistWalker Release Preflight

## 参照元
- `rust/Cargo.toml`
- `rust/Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `README-ja.md`
- `.github/release-template.md`
- `.github/workflows/release-tagged.yml`
- `docs/RELEASE.md`
- `docs/TESTPLAN.md`
- `docs/OSS_COMPLIANCE.md`
- `AGENTS.md`

## この skill の主目的
- release 前に `rust/Cargo.toml` と `rust/Cargo.lock` の version 更新漏れを防ぐ。
- 配布物の版数と changelog / tag / release note の不整合を出さない。
- 変更内容に対して docs / tests / release asset / OSS notice が追従しているか確認する。
- release 前の手動確認を `docs/TESTPLAN.md` の Validation Matrix と一致させる。
- ID 付きの表・規範セクションが `docs/TESTPLAN.md` の ID Ordering ルールに従い、後続メンテナンスを妨げる順序崩れを残さないようにする。
- 更新手順を毎回同じ順序で実施する。

## 手順
1. 対象 version を `vX.Y.Z` 形式で確定する。
2. tag をまだ作っていないことを確認する。tag 済みなら、version 不一致を黙認せずユーザへ明示する。
3. 前回 release tag を確認し、`git diff --name-only <前回tag>..HEAD` と `git diff --stat <前回tag>..HEAD` で release 対象差分を分類する。
4. 差分を `docs/TESTPLAN.md` の Validation Matrix に対応付け、VM-001 から VM-008 のどれを実行・確認すべきか決める。
5. `rust/Cargo.toml` の `[package].version` を `X.Y.Z` へ更新する。
6. `rust/Cargo.lock` 内の `name = "flist-walker"` 節の `version = "X.Y.Z"` を確認し、不一致なら更新する。
7. `CHANGELOG.md` に対象 version の節と日付、主要変更を反映する。
8. 仕様・設計・ユーザ挙動・検証条件が変わった差分では、`docs/REQUIREMENTS.md` / `docs/SPEC.md` / `docs/DESIGN.md` / `docs/TESTPLAN.md` の該当 FR/NFR/CON、SP、DES、TC と trace 抜粋が追従しているか確認する。更新不要なら理由を作業報告に残す。
9. `docs/TESTPLAN.md` の ID Ordering ルールに従い、少なくとも `docs/testplan/test-cases.md`、`docs/testplan/manual-regression-traceability.md`、`docs/testplan/validation-matrix.md`、`docs/spec/gui-behavior.md`、`docs/spec/operations-release-config.md` の ID 付き表・規範セクションが昇順であることを確認する。時系列・リスク順・トピック順を意図する文書は例外としてよいが、理由を作業報告に残す。
10. release / updater / asset / workflow / packaging を変えた差分では、`docs/RELEASE.md`、`.github/release-template.md`、`.github/workflows/release-tagged.yml`、`scripts/prepare-release*.sh|ps1` の asset 名、対象 OS、sidecar、`SHA256SUMS` / `SHA256SUMS.sig` の記述が一致しているか確認する。
11. 依存関係、release script、workflow、updater、sidecar、archive 同梱物を変えた差分では、`docs/OSS_COMPLIANCE.md` に従い `THIRD_PARTY_NOTICES.txt` と配布導線を確認する。
12. 公開向け文書へ開発・手動試験専用 update override 名が混入していないことを確認する。
13. `cargo test --locked` を実行し、少なくとも version 更新と release 対象差分で壊れていないことを確認する。
14. `cargo clippy --all-targets -- -D warnings` を実行し、Rust warning / clippy warning が残っていないことを確認する。
15. `cargo audit` を実行し、accepted transitive warning が出る場合は `docs/OSS_COMPLIANCE.md` の owner / review cadence / re-evaluation trigger と一致しているか確認する。
16. release candidate では coverage gate と GUI headful smoke / `GSM-*` 証跡の要否を確認し、必要な PASS / FAIL / SKIPPED と証跡パスを残す。
17. release asset build または GitHub Actions の release build logs に warning が出ていないことを確認する。warning が残る場合は publish 前に修正するか、release blocker ではない理由と follow-up を明記する。
18. tag 名 `vX.Y.Z`、`CHANGELOG.md`、`rust/Cargo.toml`、`rust/Cargo.lock` の version が一致していることを確認する。
19. この確認が終わるまで tag 作成・push・draft release publish を行わない。
20. release 本文が必要なら `skills/flistwalker-release-notes/SKILL.md` を続けて使う。

## 必須 grep
- 公開向け文書の禁止 update override 名:
  - `rg -n "FLISTWALKER_UPDATE_FEED_URL|FLISTWALKER_UPDATE_ALLOW_SAME_VERSION|FLISTWALKER_UPDATE_ALLOW_DOWNGRADE" README.md README-ja.md docs/RELEASE.md .github/release-template.md`
- release asset / sidecar 名の照合:
  - `rg -n "FlistWalker-|README|LICENSE|THIRD_PARTY_NOTICES|SHA256SUMS|app\\.zip|tar\\.gz|zip" docs/RELEASE.md .github/release-template.md .github/workflows/release-tagged.yml scripts/prepare-release.sh scripts/prepare-release-linux.sh scripts/prepare-release-macos.sh scripts/prepare-release.ps1`
- SDD / TDD ID 参照確認:
  - `rg -n "\\b(FR|NFR|CON|AC|SP|DES|TC)-[0-9]+[A-Z]?\\b" docs/REQUIREMENTS.md docs/SPEC.md docs/DESIGN.md docs/TESTPLAN.md`
- ID Ordering 対象の入口確認:
  - `rg -n "^\\| TC-|^- TC-|^\\| VM-|^## SP-" docs/testplan/test-cases.md docs/testplan/manual-regression-traceability.md docs/testplan/validation-matrix.md docs/spec/gui-behavior.md docs/spec/operations-release-config.md`

## チェック
- `rust/Cargo.toml` の version が対象 release と一致しているか。
- `rust/Cargo.lock` の root package `flist-walker` version が一致しているか。
- `CHANGELOG.md` の対象節が同じ version になっているか。
- tag 作成前か。すでに tag があるなら不一致のまま進めていないか。
- release 対象差分に対して Validation Matrix の VM が選択され、実行結果または未実行理由が残っているか。
- 仕様・設計・ユーザ挙動・検証条件の変更に docs 4文書と trace が追従しているか。
- `docs/TESTPLAN.md` の ID Ordering ルールに反して、ID 付きの表・規範セクションが後続メンテナンスを妨げる順序崩れになっていないか。
- release asset 名、対象 OS、sidecar、checksum / signature、notarization の扱いが release docs / template / workflow / scripts 間で一致しているか。
- 公開向け文書に開発・手動試験専用 update override 名が混入していないか。
- `THIRD_PARTY_NOTICES.txt` と `docs/OSS_COMPLIANCE.md` が依存・配布導線の変更に追従しているか。
- `cargo test --locked` が成功する状態か。
- `cargo clippy --all-targets -- -D warnings` が成功し、release build logs に warning が残っていないか。
- `cargo audit` が成功するか。accepted warning がある場合は `docs/OSS_COMPLIANCE.md` の記録と一致しているか。
- GUI-adjacent 変更または release candidate で `GSM-*` 証跡が必要な場合、証跡パスが作業報告に残っているか。

## 禁止事項
- `Cargo.toml` だけ更新して `Cargo.lock` を未確認のまま release しない。
- `CHANGELOG.md` の版数と tag をずらしたまま release しない。
- 複数 version の変更を 1 回の release 用更新に混在させない。
- version 整合確認前に tag を作成したり publish しない。
- `docs/TESTPLAN.md` の Validation Matrix を見ずに、手癖で `cargo test` だけを release 前確認として完了扱いしない。
- 公開向け文書や GitHub Release 本文へ、開発・手動試験専用 update override 名を記載しない。
- release / updater / asset 変更時に、release docs、template、workflow、scripts の一部だけを更新しない。
