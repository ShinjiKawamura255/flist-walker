[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoDir = Split-Path -Parent $ScriptDir
$Target = 'x86_64-pc-windows-gnu'
$SourceExe = Join-Path $RepoDir "rust\target\$Target\release\FlistWalker.exe"
$SafeVersion = if ($Version.StartsWith('v')) { $Version.Substring(1) } else { $Version }
$AssetBaseName = "FlistWalker-$SafeVersion-windows-x86_64"
$ExeName = "$AssetBaseName.exe"
$ZipName = "$AssetBaseName.zip"
$ZipExeName = "flistwalker.exe"
$ReadmeSideName = "$AssetBaseName.README.txt"
$LicenseSideName = "$AssetBaseName.LICENSE.txt"
$NoticesSideName = "$AssetBaseName.THIRD_PARTY_NOTICES.txt"
$OutDir = Join-Path $RepoDir "dist\$Version"
$RootLicense = Join-Path $RepoDir 'LICENSE'
$RootNotices = Join-Path $RepoDir 'THIRD_PARTY_NOTICES.txt'

if (-not (Test-Path -LiteralPath $SourceExe)) {
    Write-Error "EXE not found: $SourceExe`nRun scripts/build-rust-win.sh first."
    exit 1
}
if (-not (Test-Path -LiteralPath $RootLicense) -or -not (Test-Path -LiteralPath $RootNotices)) {
    Write-Error "LICENSE / THIRD_PARTY_NOTICES.txt not found."
    exit 1
}

New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
$WorkDir = Join-Path ([System.IO.Path]::GetTempPath()) ("flistwalker-release-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $WorkDir -Force | Out-Null

try {
    Copy-Item -LiteralPath $SourceExe -Destination (Join-Path $OutDir $ExeName) -Force
    Copy-Item -LiteralPath $SourceExe -Destination (Join-Path $WorkDir $ZipExeName) -Force
    $ReadmeSidePath = Join-Path $OutDir $ReadmeSideName
    @"
FlistWalker $Version

Contents:
- $ZipExeName
- README.txt
- LICENSE.txt
- THIRD_PARTY_NOTICES.txt

Run:
- PowerShell: .\$ZipExeName
- CMD: $ZipExeName

English:
- Type in the search box to narrow files and folders.
- Press Enter to open or execute. Press Shift+Enter to open the containing folder. Press Tab to pin rows. Press Ctrl+Shift+C to copy selected paths.
- Use Browse... to switch the root.

Search hints:
- Tokens are ANDed by default, for example main py
- abc|foo|bar means OR
- exact match uses a leading apostrophe, for example 'main.py
- !term excludes a token, for example main !test
- ^term prefers prefix matches, for example ^src
- suffix match uses a trailing $, for example .rs$
- Turn on Regex to use regular-expression search

Ignore list:
- Put flistwalker.ignore.txt in the same folder as the executable.
- Blank lines and lines starting with # are ignored.
- Each token is treated like a search exclusion, so old and ~ behave like !old !~
- The Ignore List checkbox controls whether these rules apply. It is on by default.

Runtime config:
- Runtime settings are stored in ~/.flistwalker_config.json in your home directory.
- On first launch, if the file is missing, FlistWalker creates it from the current FLISTWALKER_* environment values.
- Once the file exists, it becomes the source of truth for runtime settings and the matching environment variables are only an initial seed.
- The file is JSON and can be edited directly.
- Only the commonly useful toggles are documented here. Advanced keys are intentionally undocumented.
- `walker_max_entries` is also documented here because it affects large-root scans.

Keyboard shortcuts:
- Up/Down or Ctrl+P/Ctrl+N: move the current row
- Ctrl+V / Alt+V: page down / page up
- Enter or Ctrl+J/Ctrl+M: open or execute selected item(s)
- Shift+Enter: open the containing folder(s) for selected item(s), deduplicated by directory
- Tab / Shift+Tab: toggle pin on the current row
- Ctrl+Shift+C: copy selected path(s)
- Ctrl+R: fuzzy-search query history and load the selected entry
- Esc / Ctrl+G: clear query and pinned selection, or cancel history search
- Ctrl+L: focus the query input

Query history:
- Search history is shared across tabs and persisted up to 100 entries.
- History is committed after a short idle period or when result navigation starts.
- Intermediate IME composition text is not stored; only committed query text is saved.

Tab restore (opt-in):
- Set FLISTWALKER_RESTORE_TABS=1 to restore the previous tab state on the next launch.
- Restored fields include root, query, Use FileList, Regex, Files, Folders, and the active tab.
- If you explicitly pass --root or a startup query, those values take precedence.
- When this feature is enabled, Set as default is disabled because the startup root is determined by tab restore.

How to enable FLISTWALKER_RESTORE_TABS:
- Windows (PowerShell, persist per-user): [Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", "1", "User")
- Windows (PowerShell, current session): `$env:FLISTWALKER_RESTORE_TABS = "1"`
- Windows (CMD, current session): set FLISTWALKER_RESTORE_TABS=1
- macOS (zsh/bash, current session): export FLISTWALKER_RESTORE_TABS=1
- macOS (zsh persist): echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.zshrc
- Linux (bash, current session): export FLISTWALKER_RESTORE_TABS=1
- Linux (bash persist): echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.bashrc

Index options:
- Use FileList: prefer FileList.txt / filelist.txt at the repository root
- Files / Folders: toggle visibility without reindexing
- Refresh Index: reindex the current root

Walker tuning (Environment variables):
- FLISTWALKER_WALKER_MAX_ENTRIES: walker maximum entry count (default: 500000)
- FLISTWALKER_WALKER_THREADS: walker parallel thread count (default: 2, 1 means serial)

日本語:
- 起動後に検索窓へ文字を入力すると、ファイル/フォルダを絞り込みます。
- Enter で開く/実行、Shift+Enter で格納フォルダを開く（同一フォルダは1回のみ）、Tab でピン留め複数選択、Ctrl+Shift+C でパスコピー。
- Root は左上の Browse... から切り替え可能です。
- トークンは AND 条件（例: main py）
- abc|foo|bar : OR 条件（スペースなしの | で連結）
- 完全一致トークンは先頭に ' を付ける（例: 'main.py）
- !term : 除外トークン（例: main !test）
- ^term : 先頭一致を優先（例: ^src）
- 末尾一致は末尾に $ を付ける（例: .rs$）
- Regex チェックON時は正規表現検索

Ignore List:
- flistwalker.ignore.txt を実行ファイルと同じフォルダに置きます。
- 空行と # で始まる行は無視されます。
- 各トークンは検索の除外条件として扱われるため、old や ~ は !old !~ と同じ挙動になります。
- Ignore List チェックボックスで適用の ON/OFF を切り替えます。既定は ON です。

Runtime config:
- runtime settings は home directory の ~/.flistwalker_config.json に保存されます。
- 初回起動でファイルが無い場合は、現在の FLISTWALKER_* 環境変数を seed にして自動生成します。
- 一度ファイルができたら、その内容が runtime settings の source of truth になり、同名 env は初期 seed としてのみ使われます。
 - ここでは一般的に使う項目だけを案内しています。高度な項目は意図的に記載していません。
- 検索履歴は全タブ共通で最大100件まで保持され、短い待機後または結果移動開始時に確定します。
- FLISTWALKER_RESTORE_TABS=1 を設定すると、終了時のタブ状態を次回起動時に復元できます。
- Use FileList はルート直下の FileList.txt / filelist.txt を優先使用します。
- Refresh Index は現在Rootで再インデックスします。
"@ | Set-Content -LiteralPath $ReadmeSidePath -Encoding UTF8
    Copy-Item -LiteralPath $RootLicense -Destination (Join-Path $OutDir $LicenseSideName) -Force
    Copy-Item -LiteralPath $RootNotices -Destination (Join-Path $OutDir $NoticesSideName) -Force
    Copy-Item -LiteralPath $ReadmeSidePath -Destination (Join-Path $WorkDir 'README.txt') -Force
    Copy-Item -LiteralPath $RootLicense -Destination (Join-Path $WorkDir 'LICENSE.txt') -Force
    Copy-Item -LiteralPath $RootNotices -Destination (Join-Path $WorkDir 'THIRD_PARTY_NOTICES.txt') -Force

    $ZipPath = Join-Path $OutDir $ZipName
    if (Test-Path -LiteralPath $ZipPath) {
        Remove-Item -LiteralPath $ZipPath -Force
    }
    Compress-Archive -Path (Join-Path $WorkDir $ZipExeName), $ReadmeSidePath, (Join-Path $WorkDir 'LICENSE.txt'), (Join-Path $WorkDir 'THIRD_PARTY_NOTICES.txt') -DestinationPath $ZipPath -CompressionLevel Optimal

    $ExeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $OutDir $ExeName)).Hash.ToLowerInvariant()
    $ZipHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ZipPath).Hash.ToLowerInvariant()
    $ReadmeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ReadmeSidePath).Hash.ToLowerInvariant()
    $LicenseHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $OutDir $LicenseSideName)).Hash.ToLowerInvariant()
    $NoticesHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $OutDir $NoticesSideName)).Hash.ToLowerInvariant()
    $SumsPath = Join-Path $OutDir 'SHA256SUMS'

    @(
        "$ExeHash  $ExeName"
        "$ZipHash  $ZipName"
        "$ReadmeHash  $ReadmeSideName"
        "$LicenseHash  $LicenseSideName"
        "$NoticesHash  $NoticesSideName"
    ) | Set-Content -LiteralPath $SumsPath -Encoding ASCII

    if ($env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX) {
        $manifestPath = Join-Path $OutDir 'SHA256SUMS'
        $sigPath = Join-Path $OutDir 'SHA256SUMS.sig'
        cargo run --manifest-path (Join-Path $RepoDir 'rust\Cargo.toml') --quiet --bin sign_update_manifest -- $manifestPath $sigPath
        if ($LASTEXITCODE -ne 0) {
            throw "failed to sign update manifest"
        }
    }
}
finally {
    if (Test-Path -LiteralPath $WorkDir) {
        Remove-Item -LiteralPath $WorkDir -Recurse -Force
    }
}

Write-Host "Release assets created: $OutDir"
Write-Host "- $ExeName"
Write-Host "- $ZipName"
Write-Host "- $ReadmeSideName"
Write-Host "- $LicenseSideName"
Write-Host "- $NoticesSideName"
Write-Host "- SHA256SUMS"
if ($env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX) {
    Write-Host "- SHA256SUMS.sig"
}
