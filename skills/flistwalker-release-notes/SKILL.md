---
name: flistwalker-release-notes
description: FlistWalker の GitHub Release 本文を v0.9.0 記法で統一して作成・更新したいときに使う。CHANGELOG 更新時の整合確認にも使う。
---

# FlistWalker Release Notes

## 参照元
- `/mnt/d/work/flistwalker/.github/release-template.md`
- `/mnt/d/work/flistwalker/CHANGELOG.md`
- `/mnt/d/work/flistwalker/docs/RELEASE.md`

## この skill の主目的
- GitHub Releases の本文を、常に「その版だけ」を対象に書く。
- 複数バージョン分をまとめ書きしない。
- 過度に省略せず、`v0.9.0` と同じ構成と粒度で書く。

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

## CHANGELOG との関係
- `CHANGELOG.md` は版ごとの履歴として更新する。
- GitHub Release 本文は `CHANGELOG.md` の要約ではなく、その版専用の公開ノートとして書く。
- `CHANGELOG.md` と GitHub Release 本文で、対象バージョン・変更分類・既知の問題の整合を取る。

## 禁止事項
- 複数バージョン分の変更を1つの release note に混在させない。
- `Summary` や `Downloads` を省略しない。
- 実際に添付していないアセットを `Downloads` に書かない。
- `CHANGELOG.md` の節をそのまま大量転記して冗長にしない。

## チェック
- 版数、日付、アセット名が一致しているか。
- GitHub Release 本文が単一バージョンだけを対象にしているか。
- `CHANGELOG.md`、タグ、リリース本文で同じ変更分類になっているか。
- `SHA256SUMS` の案内が実際の配布物と一致しているか。
