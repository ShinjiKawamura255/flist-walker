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
    Copy-Item -LiteralPath $SourceExe -Destination (Join-Path $WorkDir $ExeName) -Force

    $ReadmePath = Join-Path $WorkDir 'README.txt'
    @"
FlistWalker $Version

Contents:
- $ExeName

Run:
- Double-click on Windows
- or execute from PowerShell/CMD

Search hints:
- Tokens are AND-ed (example: main py)
- 'term = exact match token (example: 'main.py)
- !term = exclusion token (example: main !test)
- ^term = prefer prefix match (example: ^src)
- term$ = prefer suffix match (example: .rs$)

Keyboard shortcuts:
- Up/Down or Ctrl+P/Ctrl+N: move current row
- Enter (or Ctrl+J/Ctrl+M): open/execute selected item(s)
- Tab / Shift+Tab: toggle pin and move next/prev
- Ctrl+Shift+C: copy selected path(s)
- Ctrl+G: clear query and pinned selection
- Ctrl+L: focus query input
"@ | Set-Content -LiteralPath $ReadmePath -Encoding UTF8

    $ZipPath = Join-Path $OutDir $ZipName
    if (Test-Path -LiteralPath $ZipPath) {
        Remove-Item -LiteralPath $ZipPath -Force
    }
    Compress-Archive -Path (Join-Path $WorkDir $ExeName), $ReadmePath -DestinationPath $ZipPath -CompressionLevel Optimal

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
