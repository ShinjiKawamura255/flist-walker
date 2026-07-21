# Operations, Trade-offs, and Traceability

## Error handling / timeout / logging / metrics
- エラー戦略: ファイルアクセス失敗、実行失敗、正規表現不正を分類して表示。
- タイムアウト: 外部プロセス起動はブロッキング待機しない。
- ログ: 現状は標準出力/標準エラー中心。必要に応じて構造化ログへ拡張。
- ログ補足: worker-side supportability trace は `RUST_LOG` で opt-in し、canonical field (`flow`, `event`, `request_id`) を優先する。GUI/session/input diagnostics は `FLISTWALKER_WINDOW_TRACE=1` の file trace を使い分ける。
- メトリクス: 検索遅延(ms)と候補件数を測定対象とする。

## Migration / rollback
- 移行: Rust 本実装を正として機能追加する。
- ロールバック: 不安定な変更は小さな単位で revert し、仕様ID単位で影響範囲を判断する。
- updater staging は trust/bounds/cleanup を独立 rollback 単位とし、activation は `VerifiedUpdateBundle` 以降の transaction/recovery を別単位とする。
- updater runtime rollback は binary commit 前と restart 生成失敗で旧 bundle を hash 検証付きで復元する。binary commit 後の完全な新 bundleは維持し、ambiguous state は自動変更せず marker/backup を recovery 証跡として残す。

## Trade-offs
- GUI フレームワークは `egui/eframe` を採用し、クロスプラットフォーム性と開発速度を優先。
- 検索アルゴリズムは完全互換より操作体験優先で調整可能とするが、SP-003 の演算子契約は維持する。

## Traceability (excerpt)
- DES-001 -> TC-001 (SP-001)
- DES-002 -> TC-002 (SP-002)
- DES-003 -> TC-003 (SP-003)
- DES-003 -> TC-092 (SP-003, SP-010)
- DES-003, DES-008 -> TC-155 (SP-003, SP-009)
- DES-004 -> TC-004, TC-005, TC-050, TC-051 (SP-004, SP-005)
- DES-005 -> TC-006 (SP-006)
- DES-006 -> TC-007, TC-156 (SP-007)
- DES-006, DES-007, DES-009 -> TC-150, TC-151, TC-152, TC-153 (SP-010)
- DES-007 -> TC-008, TC-050, TC-051 (SP-004, SP-008)
- DES-008 -> TC-009 (SP-009)
- DES-009 -> TC-010 (SP-010)
- DES-009 -> TC-012B (SP-010)
- DES-009 -> TC-046A (SP-010)
- DES-009 -> TC-128 (SP-010)
- DES-009 -> TC-129 (SP-010)
- DES-009 -> TC-130 (SP-010)
- DES-009 -> TC-131 (SP-010)
- DES-009 -> TC-132 (SP-010)
- DES-009 -> TC-133 (SP-010)
- DES-009 -> TC-134 (SP-010)
- DES-009 -> TC-135 (SP-010)
- DES-009 -> TC-136 (SP-010)
- DES-009 -> TC-137 (SP-010)
- DES-009 -> TC-138 (SP-010)
- DES-009 -> TC-139 (SP-010)
- DES-009 -> TC-144 (SP-010)
- DES-009 -> TC-149, TC-154 (SP-010)
- DES-009 -> TC-141, TC-142, TC-143 (SP-010, SP-016)
- DES-009 -> TC-068 (SP-010)
- DES-009 -> TC-069 (SP-010)
- DES-010 -> TC-011 (SP-011)
- DES-011 -> TC-020 (SP-010, SP-011)
- DES-012 -> TC-056 (SP-012)
- DES-013 -> TC-057, TC-058, TC-059, TC-060 (SP-013)
- DES-014 -> TC-074, TC-075, TC-076, TC-077, TC-078, TC-081, TC-140, TC-157, TC-158, TC-159, TC-160 (SP-014)
- DES-015 -> TC-120 (SP-010, SP-014)
- DES-016 -> TC-110, TC-112, TC-117 (SP-015)
- DES-017 -> TC-111 (SP-016)
- DES-017 -> TC-127 (SP-016)
- DES-017 -> TC-141, TC-142, TC-143 (SP-010, SP-016)
- DES-018 -> TC-113, TC-114 (SP-017)
- DES-019 -> TC-145, TC-146, TC-147, TC-148 (SP-018)
