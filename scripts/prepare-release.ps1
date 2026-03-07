[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoDir = Split-Path -Parent $ScriptDir
$Target = 'x86_64-pc-windows-msvc'
$SourceExe = Join-Path $RepoDir "rust\target\$Target\release\FlistWalker.exe"
$SafeVersion = if ($Version.StartsWith('v')) { $Version.Substring(1) } else { $Version }
$AssetBaseName = "FlistWalker-$SafeVersion-windows-x86_64"
$ExeName = "$AssetBaseName.exe"
$ZipName = "$AssetBaseName.zip"
$ZipExeName = "flistwalker.exe"
$OutDir = Join-Path $RepoDir "dist\$Version"

if (-not (Test-Path -LiteralPath $SourceExe)) {
    Write-Error "EXE not found: $SourceExe`nRun scripts/build-rust-win.ps1 first."
    exit 1
}

New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
$WorkDir = Join-Path ([System.IO.Path]::GetTempPath()) ("flistwalker-release-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $WorkDir -Force | Out-Null

try {
    Copy-Item -LiteralPath $SourceExe -Destination (Join-Path $OutDir $ExeName) -Force
    Copy-Item -LiteralPath $SourceExe -Destination (Join-Path $WorkDir $ZipExeName) -Force

    $ReadmePath = Join-Path $WorkDir 'README.txt'
    @"
FlistWalker $Version

Contents:
- $ZipExeName
- README.txt

Run:
- PowerShell: .\$ZipExeName
- CMD: $ZipExeName

Basic usage:
- 起動後に検索窓へ文字を入力すると、ファイル/フォルダを絞り込みます。
- Enter で開く/実行、Shift+Enter で格納フォルダを開く（同一フォルダは1回のみ）、Tab でピン留め複数選択、Ctrl+Shift+C でパスコピー。
- Root は左上の Browse... から切り替え可能です。

Search hints:
- トークンは AND 条件（例: main py）
- abc|foo|bar : OR 条件（スペースなしの | で連結）
- 'term : 完全一致トークン（例: 'main.py）
- !term : 除外トークン（例: main !test）
- ^term : 先頭一致を優先（例: ^src）
- term$ : 末尾一致を優先（例: .rs$）
- Regex チェックON時は正規表現検索

Keyboard shortcuts:
- Up/Down or Ctrl+P/Ctrl+N: move current row
- Ctrl+V / Alt+V: page down / page up
- Enter (or Ctrl+J/Ctrl+M): open/execute selected item(s)
- Shift+Enter: open containing folder(s) for selected item(s), deduplicated by directory
- Tab / Shift+Tab: toggle pin on current row
- Ctrl+Shift+C: copy selected path(s)
- Ctrl+R: fuzzy-search query history and load the selected entry
- Esc / Ctrl+G: clear query and pinned selection, or cancel history search
- Ctrl+L: focus query input

Query history:
- Search history is shared across tabs and persisted up to 100 entries.
- History is committed after a short idle period or when result navigation starts.
- IME intermediate composition text is not stored; only committed query text is saved.

Index options:
- Use FileList: ルート直下の FileList.txt / filelist.txt を優先使用
- Files / Folders: 表示フィルタ（再インデックスなしで即時反映）
- Refresh Index: 現在Rootで再インデックス

Walker tuning (Environment variables):
- FLISTWALKER_WALKER_MAX_ENTRIES: Walkerの最大走査件数（既定: 500000）
- FLISTWALKER_WALKER_THREADS: Walkerの並列スレッド数（既定: 2、1でシリアル）
"@ | Set-Content -LiteralPath $ReadmePath -Encoding UTF8

    $ZipPath = Join-Path $OutDir $ZipName
    if (Test-Path -LiteralPath $ZipPath) {
        Remove-Item -LiteralPath $ZipPath -Force
    }
    Compress-Archive -Path (Join-Path $WorkDir $ZipExeName), $ReadmePath -DestinationPath $ZipPath -CompressionLevel Optimal

    $ExeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $OutDir $ExeName)).Hash.ToLowerInvariant()
    $ZipHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ZipPath).Hash.ToLowerInvariant()
    $SumsPath = Join-Path $OutDir 'SHA256SUMS'

    @(
        "$ExeHash  $ExeName"
        "$ZipHash  $ZipName"
    ) | Set-Content -LiteralPath $SumsPath -Encoding ASCII
}
finally {
    if (Test-Path -LiteralPath $WorkDir) {
        Remove-Item -LiteralPath $WorkDir -Recurse -Force
    }
}

Write-Host "Release assets created: $OutDir"
Write-Host "- $ExeName"
Write-Host "- $ZipName"
Write-Host "- SHA256SUMS"
