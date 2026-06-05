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

## Trade-offs
- GUI フレームワークは `egui/eframe` を採用し、クロスプラットフォーム性と開発速度を優先。
- 検索アルゴリズムは完全互換より操作体験優先で調整可能とするが、SP-003 の演算子契約は維持する。

## Traceability (excerpt)
- DES-001 -> TC-001 (SP-001)
- DES-002 -> TC-002 (SP-002)
- DES-003 -> TC-003 (SP-003)
- DES-003 -> TC-092 (SP-003, SP-010)
- DES-004 -> TC-004, TC-005 (SP-004, SP-005)
- DES-005 -> TC-006 (SP-006)
- DES-006 -> TC-007 (SP-007)
- DES-007 -> TC-008 (SP-008)
- DES-008 -> TC-009 (SP-009)
- DES-009 -> TC-010 (SP-010)
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
- DES-009 -> TC-141, TC-142, TC-143 (SP-010, SP-016)
- DES-009 -> TC-068 (SP-010)
- DES-009 -> TC-069 (SP-010)
- DES-010 -> TC-011 (SP-011)
- DES-011 -> TC-020 (SP-010, SP-011)
- DES-012 -> TC-056 (SP-012)
- DES-013 -> TC-057, TC-058, TC-059, TC-060 (SP-013)
- DES-014 -> TC-074, TC-075, TC-076, TC-077, TC-078, TC-081, TC-140 (SP-014)
- DES-015 -> TC-120 (SP-010, SP-014)
- DES-016 -> TC-110, TC-112, TC-117 (SP-015)
- DES-017 -> TC-111 (SP-016)
- DES-017 -> TC-127 (SP-016)
- DES-017 -> TC-141, TC-142, TC-143 (SP-010, SP-016)
- DES-018 -> TC-113, TC-114 (SP-017)
