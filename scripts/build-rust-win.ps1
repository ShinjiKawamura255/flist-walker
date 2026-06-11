[CmdletBinding()]
param(
    [switch]$CheckOnly,
    [switch]$NoInstall,
    [switch]$InstallMissing,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs = @()
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common-win-gnu.ps1')

$mode = Get-FlistWalkerInstallMode -CheckOnly:$CheckOnly -NoInstall:$NoInstall -InstallMissing:$InstallMissing
if ($mode -eq 'CheckOnly') {
    [void](Invoke-FlistWalkerDependencyCheck)
    Write-Host 'Windows GNU build dependencies are available. No build was run.'
    exit 0
}
$environment = Initialize-FlistWalkerWindowsGnu -Mode $mode

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$rustDir = Join-Path $repoRoot 'rust'
$releaseDir = Join-Path $rustDir 'target\x86_64-pc-windows-gnu\release'
$sourceExe = Join-Path $releaseDir 'flistwalker.exe'
$displayExe = Join-Path $releaseDir 'FlistWalker.exe'
$sourceFull = [System.IO.Path]::GetFullPath($sourceExe)
$displayFull = [System.IO.Path]::GetFullPath($displayExe)

Remove-FlistWalkerBuildArtifact -Path $sourceExe
if (-not $sourceFull.Equals($displayFull, [System.StringComparison]::OrdinalIgnoreCase)) {
    Remove-FlistWalkerBuildArtifact -Path $displayExe
}

Push-Location $rustDir
try {
    Write-Host '==> Building Windows GNU release'
    Invoke-FlistWalkerChecked -FilePath $environment.Cargo -ArgumentList (
        @('build', '--release', '--locked', '--target', $environment.Target) + $CargoArgs
    )
} finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
    throw "Build completed without the expected executable: $sourceExe"
}

if (-not $sourceFull.Equals($displayFull, [System.StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath $sourceExe -Destination $displayExe -Force
}

$stripTarget = if (Test-Path -LiteralPath $displayExe) { $displayExe } else { $sourceExe }
Write-Host '==> Stripping Windows executable'
Invoke-FlistWalkerChecked -FilePath $environment.Tools.Strip -ArgumentList @($stripTarget)

if (-not $sourceFull.Equals($displayFull, [System.StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath $stripTarget -Destination $sourceExe -Force
}
if (-not (Test-Path -LiteralPath $sourceExe) -or -not (Test-Path -LiteralPath $displayExe)) {
    throw 'Final Windows executable aliases are missing.'
}

$sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceExe).Hash
$displayHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $displayExe).Hash
if ($sourceHash -ne $displayHash) {
    throw 'flistwalker.exe and FlistWalker.exe differ after strip.'
}

Write-Host "Windows build complete: $displayExe"
