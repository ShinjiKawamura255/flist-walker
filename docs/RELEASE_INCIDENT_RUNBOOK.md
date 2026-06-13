# Release Incident Runbook

公開済みreleaseに重大な欠陥、改ざん疑い、署名不整合、誤ったassetを検出した場合の被害抑制手順。

## 起動条件
- Critical: 署名またはchecksum不整合、改ざん疑い、任意コード実行、データ破壊。直ちに新規取得を停止する。
- High: 起動不能、自動更新不能、主要操作の広範な破損。影響を確認し、原則として新規取得を停止する。
- Medium以下: 回避策があり安全性・データ整合性へ影響しない。本文警告とpatch releaseを優先し、停止要否をrelease ownerが判断する。

## 担当
- Incident owner: release作業を実行したmaintainer。
- Security reviewer: 署名、checksum、改ざん、秘密鍵影響を判定する。
- Release reviewer: OS別asset、更新feed、利用者影響、patch release readinessを判定する。

## 手順
1. UTC/JSTの検出時刻、release URL、tag object SHA、peeled commit SHA、全asset名とSHA-256、workflow run URLを `docs/releases/incidents/<date>-<tag>.md` に保存する。
2. Critical、または安全でない取得が継続するHighでは、GitHub Releaseをdraftへ戻して新規取得を停止する。公開済みtagとassetは更新・削除・上書きしない。
3. Release本文の先頭へ影響範囲、回避策、取得停止状態を警告として追記する。draftへ戻した場合も記録用本文へ同じ警告を保持する。
4. GitHub latest release APIとアプリの更新確認経路を確認する。問題releaseがlatestとして返る場合はdraft化でfeedから除外されたことを確認し、既取得済み利用者への影響を記録する。
5. 署名秘密鍵の漏えいが疑われる場合は利用を停止し、GitHub secretをローテーションする。旧鍵で新規assetを再署名しない。
6. 修正は新しいpatch version、commit、annotated tag、assetで行う。公開済みtag/assetを再利用しない。
7. 利用者へ対象version、影響、回避策、修正版予定または修正版URLをGitHub Release本文と通常の告知経路で通知する。

## 終結基準
- 問題releaseからの新規取得が停止または安全と確認済み。
- 影響範囲と既取得済み利用者への案内が公開済み。
- patch releaseまたは「修正不要」の判断、根拠、承認者が記録済み。
- asset SHA、時刻、実施操作、review結果がincident記録へ保存済み。
- 秘密鍵影響がある場合はローテーションと新公開鍵の配布方針が完了済み。
