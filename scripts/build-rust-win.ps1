[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoDir = Split-Path -Parent $ScriptDir
$RustDir = Join-Path $RepoDir 'rust'
$Target = 'x86_64-pc-windows-msvc'
$BuiltExePath = Join-Path $RustDir "target\$Target\release\flistwalker.exe"
$ExePath = Join-Path $RustDir "target\$Target\release\FlistWalker.exe"
$CargoBinCandidates = @()
if ($env:CARGO_HOME) {
    $CargoBinCandidates += (Join-Path $env:CARGO_HOME 'bin')
}
if ($env:USERPROFILE) {
    $CargoBinCandidates += (Join-Path $env:USERPROFILE '.cargo\bin')
}
$CargoBinCandidates = $CargoBinCandidates | Select-Object -Unique
foreach ($bin in $CargoBinCandidates) {
    if (Test-Path -LiteralPath $bin) {
        $env:PATH = "$bin;$env:PATH"
    }
}

function Resolve-Tool([string]$Name) {
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($bin in $CargoBinCandidates) {
        $candidate = Join-Path $bin ("$Name.exe")
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }
    return $null
}

function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    $prevErrorAction = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & $Exe @Arguments
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prevErrorAction
    return $code
}

if (-not (Test-Path -LiteralPath $RustDir)) {
    Write-Error "rust directory not found: $RustDir"
    exit 1
}

$RustupExe = Resolve-Tool 'rustup'
$CargoExe = Resolve-Tool 'cargo'
if (-not $RustupExe -or -not $CargoExe) {
    Write-Error "rustup/cargo not found on Windows PATH. Install Rust with rustup (msvc toolchain)."
    exit 1
}

Write-Host "==> Ensure target: $Target"
$targetAddExit = Invoke-Native $RustupExe @('target', 'add', $Target)
if ($targetAddExit -ne 0) {
    exit $targetAddExit
}

if (Test-Path -LiteralPath $ExePath) {
    try {
        Remove-Item -LiteralPath $ExePath -Force
    }
    catch {
        Write-Error "Could not delete existing EXE (possibly running): $ExePath"
        Write-Error "Close the app and run this script again."
        exit 1
    }
}
if (Test-Path -LiteralPath $BuiltExePath) {
    try {
        Remove-Item -LiteralPath $BuiltExePath -Force
    }
    catch {
        Write-Error "Could not delete existing EXE (possibly running): $BuiltExePath"
        Write-Error "Close the app and run this script again."
        exit 1
    }
}

Write-Host "==> Build (release): $Target"
Set-Location -LiteralPath $RustDir
$buildExit = Invoke-Native $CargoExe @('build', '--release', '--target', $Target)
if ($buildExit -ne 0) {
    exit $buildExit
}

if ((Test-Path -LiteralPath $BuiltExePath) -and ($BuiltExePath.ToLowerInvariant() -ne $ExePath.ToLowerInvariant())) {
    Copy-Item -LiteralPath $BuiltExePath -Destination $ExePath -Force
}

if (-not (Test-Path -LiteralPath $ExePath)) {
    Write-Error "Build finished but artifact not found: $ExePath"
    exit 1
}

Write-Host "==> Built: $ExePath"
