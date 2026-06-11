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
    Write-Host 'Windows GNU clean-build dependencies are available. No clean or build was run.'
    exit 0
}
$environment = Initialize-FlistWalkerWindowsGnu -Mode $mode

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$rustDir = Join-Path $repoRoot 'rust'
Push-Location $rustDir
try {
    Write-Host '==> Cleaning Windows GNU target'
    Invoke-FlistWalkerChecked -FilePath $environment.Cargo -ArgumentList @('clean', '--target', $environment.Target)
} finally {
    Pop-Location
}

& (Join-Path $PSScriptRoot 'build-rust-win.ps1') -NoInstall @CargoArgs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
