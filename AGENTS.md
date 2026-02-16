# AGENTS.md for FastFileFinder

このファイルはプロジェクト固有のエージェント指示です。ワークスペース共通の方針と矛盾する場合は本ファイルを優先します。

## 1. 目的と範囲
- 目的: `fzf --walker` 相当の体験で、ファイル/フォルダを高速にファジー検索し、選択結果を実行またはオープンできるツールを開発する。
- スコープ In:
- Python での GUI プロトタイプ実装と仕様確定
- Rust での本実装（GUI/CLI）
- `FileList.txt` / `filelist.txt` 優先読み込み
- File/Folder walker の分離
- fzf 互換クエリ（`'` 完全一致、`!` 除外、`^`/`$` の先頭末尾条件）
- 検索ハイライト、非マッチ非表示、複数選択と一括操作
- Windows 向けクロスコンパイル運用（`cargo-xwin`）
- `docs/` の SDD + TDD 文書保守
- スコープ Out:
- ネットワークドライブ向け最適化
- 配布インストーラ作成

## 2. ユーザ指示（原文）
- このプロジェクトでは、fzfのwalkerと同様にファイルとフォルダを高速にファジーサーチして実行・オープンできるツールを作りたいです。
- また、FileList.txt(すべて小文字も認める)が存在する場合、それを読み込んでファジーサーチして、ファイルを実行や、フォルダのオープンもしたいです。
- 開発はまずはPython(任意のライブラリ使用可)で行い、大まかなUI含めた実装イメージを固めたのち、Rustで本実装を行いたいです。

## 3. 解釈・補足
- 現在フェーズ: Python GUI プロトタイプで固めた仕様を Rust 実装へ反映する。
- 優先順位1: `FileList.txt` / `filelist.txt` の両対応と walker 走査の切替仕様を維持する。
- 優先順位2: Python/Rust 間で検索仕様・キー操作仕様を一致させる。
- 優先順位3: Windows 実行を前提に、PowerShell から実行可能なスクリプトを維持する。
- 前提条件:
- Python 開発環境は Python 3.11+、テストは `pytest`。
- Rust 開発環境は `rustup` 管理、クロスビルドは `cargo-xwin` を利用。

## 4. 重要な制約・品質特性
- 対応環境: Windows/macOS/Linux の主要 OS。
- 性能: 10万件候補で検索応答 100ms 未満を目標（試作時は目安、Rust で本格最適化）。
- 品質: TDD を基本とし、主要機能は unit test で保証する。
- セキュリティ: 外部コマンド実行は配列引数で呼び出し、シェル展開依存を避ける。
- 運用:
- GUI 実行は `scripts/run-gui.sh` / `scripts/run-gui.ps1` を利用する。
- Windows 向け Rust ビルドは `scripts/build-rust-win.sh` / `scripts/build-rust-win.ps1` を利用する。

## 5. ドキュメント/プロセス
- `docs/` に `REQUIREMENTS.md` / `SPEC.md` / `DESIGN.md` / `TESTPLAN.md` を配置。
- ID は `FR-###` / `NFR-###` / `CON-###` / `SP-###` / `DES-###` / `TC-###` を付与。
- SPEC は MUST/SHOULD で規範化し、TDD を徹底する。

## 6. トレース（抜粋）
- FR-### → SP-### → DES-### → TC-###
