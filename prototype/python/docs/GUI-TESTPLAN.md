# GUI TESTPLAN

## Scope
- Target version: Python prototype v0.2.0 (GUI first draft)
- Screens/flows:
- 検索画面起動
- 検索入力 -> 結果更新
- 結果選択 -> プレビュー表示
- 実行/オープン
- インデックス再読込
- Priority: P0（主要フローのみ）

## Environment
- OS: Windows 11 / Ubuntu 22.04 / macOS 14（推奨）
- Browser: N/A (desktop app)
- Device: Desktop

## Test data
- Preparation steps:
1. テスト用ルートに `FileList.txt` あり/なしの2ケースを準備。
2. 実行可能ファイル1件、通常テキストファイル2件、フォルダ2件を作成。
3. テキストファイルのうち1件は20行以上にしてプレビュー上限を確認。

## Test cases
| ID | Flow | Steps | Expected |
| --- | --- | --- | --- |
| GUI-001 | 起動 | `flistwalker --gui --root <dir>` を実行 | ウィンドウが開き、Root と Source が表示される |
| GUI-002 | 検索 | クエリ入力欄に文字を入力 | 120ms 程度で結果が再描画される |
| GUI-003 | プレビュー | 結果リストを上下で選択 | 右ペインにファイル内容/フォルダ情報が更新される |
| GUI-004 | アクション | `Open / Execute` またはダブルクリック | ファイルは実行またはオープン、フォルダはオープンされる |
| GUI-005 | 再読込 | ファイル追加後 `Refresh Index` を押す | 新規項目が検索結果に反映される |
| GUI-006 | エラー表示 | 無効な権限/関連付けで操作 | エラーダイアログで失敗理由が表示される |

## Risks
- OS の既定アプリ関連付け差で GUI-004 の実結果が異なる。軽減策: 期待値を「外部起動の成功」に寄せ、アプリ種別は問わない。
