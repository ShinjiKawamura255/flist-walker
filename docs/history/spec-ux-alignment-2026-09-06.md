# 仕様整合・UX改善の検証記録（2026-09-06）

対象は `b3454a30ca599d1c272e3f25aae092ea63cbd883..c380f60`、作業ブランチは `codex/spec-ux-alignment`。実装とテストはmacOS / aarch64で実施した。これは当該変更の記録であり、将来のリリースや別OSの合格証跡ではない。

## 対応内容

| 対象 | 修正後の動作 | 主な仕様・検証 |
| --- | --- | --- |
| regex OR | exact枝とregex枝の混在を解釈し、空枝・空anchor exact枝を除外。group/class/escape/flagを維持 | SP-003 / TC-155 |
| field query | path/dirを表示形式と独立したroot相対pathへ照合。literal separatorとmultibyte highlight offsetを統一 | SP-020 / TC-175 |
| GUI Score復帰 | All matchesの部分集合からではなく全候補のScore上位を復元 | SP-013 / TC-057B |
| sort切替中の検索 | 最新mode/scopeで保留queryを再要求し、旧応答を排除 | SP-013 / TC-057 |
| 履歴・Esc | queryが変化した場合にScoreへ戻す。presetの明示sortは保持 | SP-010, SP-013 |
| batch Walker | 共有adaptive serial/classifierを使用。socket等を除外し、単一kind filterでlink targetを判定。batch件数を制限しない | SP-002 / TC-002 |
| nested FileList | canonicalな2種類の名前だけでsubtreeを更新。候補ごとに取消確認 | SP-001 / TC-030 |
| Windows depth | 通常とverbatim drive/UNC pathをlosslessな字句変換で比較 | SP-021 / TC-180 |
| 設定file open | 単一worker、1 pending、連打抑止。元tabへ完了・失敗通知。process envを再設定しない | SP-016 / TC-127 |
| directory preview | 4096項目＋1 lookahead、24行、sampleだけsort。下限件数・省略・部分errorを明示 | SP-010 / TC-012B |
| preview取消 | 処理中とdrain破棄の両方にterminalを返し、非active tabのbusyを解除、復帰時に再要求 | SP-010 / TC-106 |
| 一覧scroll | 履歴・preset・Named Rootの選択変更時だけ可視位置へ最小scroll | SP-010 / TC-010, TC-174 |
| tab overflow | 横scroll、active追従、追加/設定controls固定、完全root tooltip | SP-010 / TC-010 |
| window復元 | 実monitor矩形と保存scaleを使用。gap/負座標/混在scaleに対応。不明な位置はWM配置 | SP-010 / TC-020 |
| TUI退役処理 | producer公開前のArc guard、保持4＋queue4。重いpath配列の最終dropは入力loop外 | SP-006 / TC-162 |
| TUI Help・preset | 狭幅F1導線とF7管理。Nで保存、Delete→Enterで削除。atomic RMW、失敗draft保持、settlementと出力の順序を保証 | SP-006, SP-019 / TC-162, TC-174 |

要件・仕様・設計・テストの既存IDを維持し、関連SDDとREADMEを更新した。依存、CI policy、release資産は変更していない。

## 検証結果

| 検証 | 結果 |
| --- | --- |
| 最終 `cargo test --locked` | PASS: library 1138、binary 3、CLI contract 44。既存ignored 14 |
| TUI focused `cli_tui::` | PASS: 102 |
| GUI focused `regression_gui_` | PASS: 22 |
| canonical deterministic GUI wrapper | PASS: 12グループ、234実行。window font計測とtab性能はwrapperの明示除外で、tab性能は別途実行 |
| `stateful_endurance` | PASS: normal 14、既存ignored 3。実worker 10秒soakは6196 iterationで完了、pending/routing残留なし |
| fmt / all-targets clippy `-D warnings` | PASS |
| LLVM line coverage | PASS: 32463 / 39429 = 82.33%（gate 75%） |
| Python repository/CI/worktree/validation tests | PASS: 59 |
| validation selector / repository contract | PASS: VM-001/002/003/004/005/006/008/010を確認 |
| FileList / Walker ignored perf | PASS: metadata control比1.65x / eager-kind control比4.21x |
| adaptive local / release matrix | PASS: 件数一致、warm比較を実施 |
| 10万件検索shape | PASS: maximum 42ms、250ms未満。warm絞込みと結果整合も確認 |
| 100万件検索shape | PASS: 2 shape × 7 sample、4 RSS phase、結果整合。selective p95 297ms / dense p95 412ms。RSSは観測値 |
| release tab transition | PASS: 10万件、50 sample、p95 0.002ms |
| GUI fixture / Bash parser | PASS: hash/count、破損copy拒否、report保持、3 Bash script構文、文書参照 |
| macOS staged GUI liveness | PASS: isolated settings、120秒生存、実行file allowlistとupdate artifact不在 |
| macOS PTY TUI | PASS: 40列F1 Help、Help開閉、F7、catalog保存先なしのerror、Esc復帰、Ctrl+C exit 130、stdout空 |

`cargo llvm-cov`の初回はテスト成功後の相対report出力先だけが失敗したため、同じprofile dataから絶対pathへの`cargo llvm-cov report`でゲートを判定した。PTY初回のPython fork前処理はテストハーネスで待機し、テスト子プロセスが残っていないことを確認して終了した。`start_new_session`を使うハーネスで再実行し、上記を確認した。

初期GUI回帰の既存reclaimerテスト `tc_207_active_failure_debt_switches_to_background_and_cleans_routing` は1回失敗したが、単独・全体再実行と最終全体実行は成功した。失敗を隠す変更は行っていない。

## GUIの検証範囲と残る確認

| Surface | Deterministic | Native interaction | Liveness |
| --- | --- | --- | --- |
| GSM-001〜011の関連GUI owner groups | PASS | NOT RUN: 操作ツールがbare staged executableをappとして解決できず、キー/マウスの実画面確認ができなかった | PASS: staged macOS process 120秒 |
| 実multi-display / mixed DPI / IME | pure monitor/input fixture PASS | NOT RUN: 物理的な複数画面移動・IME操作は未実施 | 上記のみ |
| Windows/Linux | 共有ロジックのmacOSテストのみ | NOT RUN: 利用可能なRust targetはaarch64-apple-darwinのみ。Windows native/GNU/WSL、Linux native、PowerShell wrapper/parserは未実行 | NOT RUN |
| native Open/Reveal/editor | recording backend PASS | NOT RUN: 外部アプリ起動の実操作は対象外 | 対象外 |

Windowsのdrive/UNC depthとnative positioningはWindows環境、Linux positioningはLinux desktop、物理画面移動は複数displayのsessionで追加確認する必要がある。実Open/Reveal/editorの確認には、対象handler/sessionを明示した承認が必要となる。

## コミットとレビュー

- `98ac594`: 検索・索引・GUI sortの仕様整合。
- `25ba845`: 空anchor exact ORの追加修正。
- `9608e43`: 設定openとpreviewの非同期・上限・取消terminal。
- `1dc382d`: 一覧/tab scrollと実monitorに基づくwindow復元。
- `c380f60`: TUI retirement、Help、preset管理。

実装前の独立計画レビューを実施した。中間レビューのpreview未settle、空anchor exact、非active config通知の3件は失敗テストを追加して修正し、再検証した。最終独立レビューでは、実装に関与していないreviewerが `b3454a30ca599d1c272e3f25aae092ea63cbd883..c380f606c43254ddfb6ab5365d7d23bb046550fa` の実装5コミットとSDD・検証ログを確認し、新規のblocking／major／minor指摘はなかった。前回3件の修正を確認し、独立実行した `cargo test --locked --lib alignment_` も22件成功した。全体テスト・性能・GUI・soakのログとLCOVの82.33%を照合し、未実施のnative／他OS検証との区別も確認した。

ローカルの詳細ログは `/private/tmp/flist-alignment-*`、GUI reportは `rust/target/gui-smoke/evidence/GUI-DETERMINISTIC-20260905T145739Z-23029.local.md` と `GUI-HEADFUL-SMOKE-20260905T145555Z-22586.local.md` にある。これらは一時証跡であり、永続的な結果要約は本記録を参照する。
