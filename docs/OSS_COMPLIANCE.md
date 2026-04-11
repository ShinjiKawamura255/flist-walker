# OSS Compliance

依存追加・依存更新・配布導線変更のたびに、OSS 配布義務を同一変更で確認するための運用メモ。

## Scope
- Rust 依存の追加・削除・更新（`rust/Cargo.toml`, `rust/Cargo.lock`）
- release script / workflow / updater の変更で、配布物へ含める OSS 文書や asset 構成が変わる場合
- `LICENSE`, `THIRD_PARTY_NOTICES.txt`, sidecar asset, archive 同梱物に影響する変更

## Required outputs
- repo root の `LICENSE`
- repo root の `THIRD_PARTY_NOTICES.txt`
- release asset に含める sidecar:
  - `*.LICENSE.txt`
  - `*.THIRD_PARTY_NOTICES.txt`
- archive / app bundle / self-update 配置先へ入る:
  - `LICENSE.txt`
  - `THIRD_PARTY_NOTICES.txt`

## Checklist
1. 追加・更新した依存を洗い出す。
   - `git diff -- rust/Cargo.toml rust/Cargo.lock`
   - 依存増減だけでなく feature 変更も対象にする。
2. 追加依存の license / notice 要否を確認する。
   - crates.io / upstream repository / license file を見て、再配布条件と notice 要件を確認する。
   - copyleft や追加条件付き license は、そのまま採用せず影響を整理する。
   - 観測性・診断用依存（例: `tracing`, `tracing-subscriber`）も対象外にしない。
3. `THIRD_PARTY_NOTICES.txt` を更新する。
   - direct dependency と配布上重要な transitive dependency の変化を反映する。
   - release / self-update / sidecar 導線で配る内容と矛盾させない。
4. 配布導線を確認する。
   - `scripts/prepare-release*.sh|ps1`
   - `.github/workflows/release-tagged.yml`
   - `rust/src/updater.rs`
   - archive / sidecar / self-update 復元先に `LICENSE` / `THIRD_PARTY_NOTICES` が残ることを確認する。
5. docs を更新する。
   - 運用や配布物の前提が変わる場合は `docs/RELEASE.md`, `.github/release-template.md`, `AGENTS.md` を同一変更で更新する。
6. 検証結果を残す。
   - PR または作業報告に「どの依存が増えたか」「どの OSS 対応を更新したか」「何で確認したか」を明記する。

## Minimum evidence in completion report
- 追加・更新した依存名
- 更新した OSS 関連ファイル
- 確認した配布導線
- 実行した検証コマンド

## Repository-specific notes
- release asset と standalone 配布では `LICENSE` / `THIRD_PARTY_NOTICES` の sidecar を必須とする。
- self-update 後も配置先ディレクトリに `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` が残る必要がある。
- 依存変更時は、少なくとも `docs/RELEASE.md` に書かれている配布物一覧と矛盾しないことを確認する。
