# Quality, Constraints, and Risks

### Non-functional (NFR)
- NFR-001: 10万件候補での検索処理は 100ms 未満を目標（SHOULD）とする。
- NFR-002: 例外時はユーザ向けに原因を表示し、非ゼロ終了コードを返すこと。
- NFR-003: モジュールはテスト可能に分離され、主要機能に unit test を持つこと。
- NFR-004: GUI の主要フロー（検索、選択、アクション、再読込）は回帰手順を定義すること。
- NFR-005: セキュリティ衛生として、CI は release 対象 OS を継続検証し、依存関係の脆弱性検査を自動実行しなければならない。
- NFR-006: ソート追加後もインデクシング速度は既存実装より劣化させないこと。
- NFR-007: 更新確認・ダウンロードは UI スレッドをブロックせず、失敗時も通常検索操作を継続可能でなければならない。

### Constraints (CON)
- CON-001: 本実装は Rust で行う。
- CON-002: 外部コマンド実行は配列引数で呼び出し、シェル展開依存を避ける。
- CON-003: root 配下判定はインデクシング経路へ導入せず、実行/オープン直前に行う。
- CON-004: 自己更新は既存 release asset 命名規則（`FlistWalker-<version>-<platform>-<arch>` と `SHA256SUMS`）を前提とし、追加の常駐 updater 事前配置を要求してはならない。

## Risks
- R-001: OS ごとのオープン/実行差異により挙動不一致が発生する。軽減策: 実行/オープン分岐を抽象化しテストで検証する。
- R-002: 大規模ディレクトリで走査コストが増大する。軽減策: FileList 優先、非同期処理、バッチ更新を維持する。
- R-003: FileList に root 外パスが含まれると、意図しない実行対象が一覧へ混入する。軽減策: 表示は許容しつつ、実行/オープン直前に root 配下判定で拒否する。
- R-004: query history の平文永続化は、運用によっては機微な検索語を残す。軽減策: 永続化無効化設定と注意書きを提供する。
- R-005: 依存脆弱性や release 対象 OS の退行が CI で検知されない。軽減策: Linux 追加と `cargo audit` を必須化する。
- R-006: 日付ソートのために全候補へ `metadata()` を導入すると index/search の体感が悪化する。軽減策: 結果スナップショット限定の遅延解決と上限付きキャッシュを採用する。
- R-007: GitHub API 一時障害やネットワーク不通で起動時更新確認が失敗する。軽減策: 非同期確認として失敗を notice に閉じ込め、検索機能は継続する。
- R-008: 実行中バイナリの置換に失敗すると更新後再起動できない。軽減策: Windows は別 updater、Linux は一時スクリプト経由で置換し、署名済み checksum manifest と整合する staged binary のみ使用する。
- R-009: ignore list の解釈が query とずれると、検索結果と UI 表示が不一致になる。軽減策: 除外判定は query の `!` と同じ非 fuzzy の比較ルールに寄せ、既定有効/切替状態を session に保持する。
- R-010: runtime config file の自動生成や seed-only 挙動が不明瞭だと、環境変数での一時的な変更が効かず、起動時設定の期待が外れる。軽減策: 初回生成と既存ファイル優先、Windows での `%LocalAppData%\flistwalker\` と Linux/macOS の `~/.flistwalker/` を README / release README / SPEC に明記し、起動時に file が source of truth であることを固定する。UI state、saved roots、window trace も同じ保存先ルールに揃える。旧保存先からの移行は transition period に限って維持し、後続版で削除できるようにする。
- R-011: sample ignore list を release asset にのみ依存させると、自己更新時や特殊な配布形態で sample が欠落しうる。軽減策: sample を埋め込み、起動時に local 実体を自動生成し、既存 ignore list は上書きしない。
