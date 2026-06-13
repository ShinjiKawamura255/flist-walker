---
name: flistwalker-release-notes
description: FlistWalker の GitHub Release 本文を v0.9.0 記法で統一して作成・更新したいときに使う。CHANGELOG 更新時の整合確認にも使う。
---

# FlistWalker Release Notes

## 参照元
- `.github/release-template.md`
- `CHANGELOG.md`
- `docs/RELEASE.md`

## この skill の主目的
- GitHub Releases の本文を、常に「その版だけ」を対象に書く。
- 複数バージョン分をまとめ書きしない。
- 過度に省略せず、`v0.9.0` と同じ構成と粒度で書く。

## 変更ソースの確定
- Release note / CHANGELOG の本文は、必ず前回リリース tag から対象 tag までの差分を一次情報にする。
- tag 作成前の release 準備では、直近 tag を `git describe --tags --abbrev=0 HEAD` で確認し、`git log --oneline <直近tag>..HEAD` と `git diff --stat <直近tag>..HEAD` を変更ソースにする。
- tag 作成後または draft release 最終化時は、対象 tag の直前 tag を `git describe --tags --abbrev=0 <対象tag>^` で確認し、`git log --oneline <直前tag>..<対象tag>` と `git diff --stat <直前tag>..<対象tag>` を変更ソースにする。
- GitHub の `--generate-notes` で作られた draft 本文は下書きとして扱い、最終化前に上記 git range と `CHANGELOG.md` の対象節へ照合する。
- 最新数件の commit だけ、作業直前の変更だけ、または古い `[Unreleased]` compare link だけを根拠に本文を作ってはならない。

## 差分抽出の手順
- release note を作る前に、まず `git log --oneline <前回tag>..<対象tag>` を commit 単位の入力一覧として扱う。
- その一覧の各 commit について、ユーザ向けの変更か、テスト/文書/依存整理か、内部実装のみかを 1 件ずつ仕分ける。
- `Changed` / `Fixed` / `Added` の各項目は、commit subject と `git diff --stat` で見える変更を最低 1 回ずつ照合してから書く。
- 変更分類に入らない commit がある場合は、なぜ本文から外すのかを自分で説明できる状態にしてから確定する。
- 1 つでも commit が「どの bullet にも対応していない」なら、本文は未完成として扱い、分類か要約を見直す。
- 人間の記憶で足した追記は、必ず commit 範囲の再確認で裏取りする。

## GitHub Release 本文の固定フォーマット
- `Summary`
- `Downloads`
- `Added`
- `Changed`
- `Fixed`
- `Breaking`
- `Deprecated`
- `Security`
- `Known issues`
- `Verify checksum`

## GitHub Release 本文のルール
- `Summary` には対象バージョンと公開日を書く。
- `Downloads` には、その回に実際に添付したアセットだけを書く。
- 変更分類は `Added` / `Changed` / `Fixed` を基本とし、該当がなくても見出しは維持する。
- 1項目 = 1行で簡潔に書く。ただし、v0.9.0 と同程度の情報量は維持する。
- その版でユーザが認識すべき変更を優先し、内部事情だけで埋めない。
- 破壊的変更、非推奨、セキュリティ影響、既知の制約は該当時に明示する。
- `Verify checksum` には `.github/release-template.md` の検証例を使う。
- Windows-only 公開時は、Windows アセットと `SHA256SUMS` のみを列挙し、macOS 未提供理由を `Known issues` か `Summary` に明記する。
- GitHub Actions の tagged release build で macOS アセットも生成される通常リリースでは、macOS 未提供前提の `Known issues` を書かない。

## CHANGELOG との関係
- `CHANGELOG.md` は版ごとの履歴として更新する。
- GitHub Release 本文は `CHANGELOG.md` の要約ではなく、その版専用の公開ノートとして書く。
- `CHANGELOG.md` と GitHub Release 本文で、対象バージョン・変更分類・既知の問題の整合を取る。
- release 準備時は `[Unreleased]` の compare link が最新リリース tag から `HEAD` を指していることを確認し、古ければ更新する。

## 禁止事項
- 複数バージョン分の変更を1つの release note に混在させない。
- `Summary` や `Downloads` を省略しない。
- 実際に添付していないアセットを `Downloads` に書かない。
- `CHANGELOG.md` の節をそのまま大量転記して冗長にしない。
- 前回リリース tag より前の変更を新しい release note に混ぜない。

## チェック
- 版数、日付、アセット名が一致しているか。
- GitHub Release 本文が単一バージョンだけを対象にしているか。
- 変更ソースの git range が `<前回リリースtag>..<対象tagまたはHEAD>` になっているか。
- `CHANGELOG.md`、タグ、リリース本文で同じ変更分類になっているか。
- `SHA256SUMS` の案内が実際の配布物と一致しているか。
- `git log` の各 commit が、少なくとも `Summary` / `Changed` / `Fixed` / `Added` のどれかに反映されているか。
